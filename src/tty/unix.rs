// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//! tty related functionality
//!

use crate::tty::EventedReadWrite;
use crate::term::SizeInfo;
use crate::display::OnResize;
use crate::config::{Config, Shell};
use crate::cli::Options;
use mio;

use libc::{self, c_int, pid_t, winsize, SIGCHLD, TIOCSCTTY, WNOHANG};
use nix::pty::openpty;

use std::os::unix::{process::CommandExt, io::{FromRawFd, AsRawFd, RawFd}};
use std::fs::File;
use std::process::{Command, Stdio};
use std::ffi::CStr;
use std::ptr;
use mio::unix::EventedFd;
use std::io;

/// Process ID of child process
///
/// Necessary to put this in static storage for `sigchld` to have access
pub static mut PID: pid_t = 0;

/// Exit flag
///
/// Calling exit() in the SIGCHLD handler sometimes causes opengl to deadlock,
/// and the process hangs. Instead, this flag is set, and its status can be
/// checked via `process_should_exit`.
static mut SHOULD_EXIT: bool = false;

extern "C" fn sigchld(_a: c_int) {
    let mut status: c_int = 0;
    unsafe {
        let p = libc::waitpid(PID, &mut status, WNOHANG);
        if p < 0 {
            die!("Waiting for pid {} failed: {}\n", PID, errno());
        }

        if PID == p {
            SHOULD_EXIT = true;
        }
    }
}

pub fn process_should_exit() -> bool {
    unsafe { SHOULD_EXIT }
}

/// Get the current value of errno
fn errno() -> c_int {
    ::errno::errno().0
}

/// Get raw fds for master/slave ends of a new pty
fn make_pty(size: winsize) -> (RawFd, RawFd) {
    let mut win_size = size;
    win_size.ws_xpixel = 0;
    win_size.ws_ypixel = 0;

    let ends = openpty(Some(&win_size), None)
        .expect("openpty failed");

    (ends.master, ends.slave)
}

/// Really only needed on BSD, but should be fine elsewhere
fn set_controlling_terminal(fd: c_int) {
    let res = unsafe {
        // TIOSCTTY changes based on platform and the `ioctl` call is different
        // based on architecture (32/64). So a generic cast is used to make sure
        // there are no issues. To allow such a generic cast the clippy warning
        // is disabled.
        #[allow(clippy::cast_lossless)]
        libc::ioctl(fd, TIOCSCTTY as _, 0)
    };

    if res < 0 {
        die!("ioctl TIOCSCTTY failed: {}", errno());
    }
}

#[derive(Debug)]
struct Passwd<'a> {
    name: &'a str,
    passwd: &'a str,
    uid: libc::uid_t,
    gid: libc::gid_t,
    gecos: &'a str,
    dir: &'a str,
    shell: &'a str,
}

/// Return a Passwd struct with pointers into the provided buf
///
/// # Unsafety
///
/// If `buf` is changed while `Passwd` is alive, bad thing will almost certainly happen.
fn get_pw_entry(buf: &mut [i8; 1024]) -> Passwd<'_> {
    // Create zeroed passwd struct
    let mut entry: libc::passwd = unsafe { ::std::mem::uninitialized() };

    let mut res: *mut libc::passwd = ptr::null_mut();

    // Try and read the pw file.
    let uid = unsafe { libc::getuid() };
    let status = unsafe {
        libc::getpwuid_r(uid, &mut entry, buf.as_mut_ptr() as *mut _, buf.len(), &mut res)
    };

    if status < 0 {
        die!("getpwuid_r failed");
    }

    if res.is_null() {
        die!("pw not found");
    }

    // sanity check
    assert_eq!(entry.pw_uid, uid);

    // Build a borrowed Passwd struct
    Passwd {
        name: unsafe { CStr::from_ptr(entry.pw_name).to_str().unwrap() },
        passwd: unsafe { CStr::from_ptr(entry.pw_passwd).to_str().unwrap() },
        uid: entry.pw_uid,
        gid: entry.pw_gid,
        gecos: unsafe { CStr::from_ptr(entry.pw_gecos).to_str().unwrap() },
        dir: unsafe { CStr::from_ptr(entry.pw_dir).to_str().unwrap() },
        shell: unsafe { CStr::from_ptr(entry.pw_shell).to_str().unwrap() },
    }
}

pub struct Pty {
    pub fd: File,
    token: mio::Token,
}

impl Pty {
    /// Resize the pty
    ///
    /// Tells the kernel that the window size changed with the new pixel
    /// dimensions and line/column counts.
    pub fn resize<T: ToWinsize>(&self, size: &T) {
        let win = size.to_winsize();

        let res = unsafe {
            libc::ioctl(self.fd.as_raw_fd(), libc::TIOCSWINSZ, &win as *const _)
        };

        if res < 0 {
            die!("ioctl TIOCSWINSZ failed: {}", errno());
        }
    }
}

/// Create a new tty and return a handle to interact with it.
pub fn new<T: ToWinsize>(
    config: &Config,
    options: &Options,
    size: &T,
    window_id: Option<usize>,
) -> Pty {
    let win_size = size.to_winsize();
    let mut buf = [0; 1024];
    let pw = get_pw_entry(&mut buf);

    let (master, slave) = make_pty(win_size);

    let default_shell = if cfg!(target_os = "macos") {
        let shell_name = pw.shell.rsplit('/').next().unwrap();
        let argv = vec![
            String::from("-c"),
            format!("exec -a -{} {}", shell_name, pw.shell),
        ];

        Shell::new_with_args("/bin/bash", argv)
    } else {
        Shell::new(pw.shell)
    };
    let shell = config.shell().unwrap_or(&default_shell);

    let initial_command = options.command().unwrap_or(shell);

    let mut builder = Command::new(initial_command.program());
    for arg in initial_command.args() {
        builder.arg(arg);
    }

    // Setup child stdin/stdout/stderr as slave fd of pty
    // Ownership of fd is transferred to the Stdio structs and will be closed by them at the end of
    // this scope. (It is not an issue that the fd is closed three times since File::drop ignores
    // error on libc::close.)
    builder.stdin(unsafe { Stdio::from_raw_fd(slave) });
    builder.stderr(unsafe { Stdio::from_raw_fd(slave) });
    builder.stdout(unsafe { Stdio::from_raw_fd(slave) });

    // Setup shell environment
    builder.env("LOGNAME", pw.name);
    builder.env("USER", pw.name);
    builder.env("SHELL", pw.shell);
    builder.env("HOME", pw.dir);

    if let Some(window_id) = window_id {
        builder.env("WINDOWID", format!("{}", window_id));
    }

    builder.before_exec(move || {
        // Create a new process group
        unsafe {
            let err = libc::setsid();
            if err == -1 {
                die!("Failed to set session id: {}", errno());
            }
        }

        set_controlling_terminal(slave);

        // No longer need slave/master fds
        unsafe {
            libc::close(slave);
            libc::close(master);
        }

        unsafe {
            libc::signal(libc::SIGCHLD, libc::SIG_DFL);
            libc::signal(libc::SIGHUP, libc::SIG_DFL);
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            libc::signal(libc::SIGQUIT, libc::SIG_DFL);
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGALRM, libc::SIG_DFL);
        }
        Ok(())
    });

    // Handle set working directory option
    if let Some(ref dir) = options.working_dir {
        builder.current_dir(dir.as_path());
    }

    match builder.spawn() {
        Ok(child) => {
            unsafe {
                // Set PID for SIGCHLD handler
                PID = child.id() as _;

                // Handle SIGCHLD
                libc::signal(SIGCHLD, sigchld as _);
            }
            unsafe {
                // Maybe this should be done outside of this function so nonblocking
                // isn't forced upon consumers. Although maybe it should be?
                set_nonblocking(master);
            }

            let pty = Pty {
                fd: unsafe {File::from_raw_fd(master) },
                token: mio::Token::from(0)
            };
            pty.resize(size);
            pty
        },
        Err(err) => {
            die!("Failed to spawn command: {}", err);
        }
    }
}

impl EventedReadWrite for Pty {
    type Reader = File;
    type Writer = File;

    #[inline]
    fn register(
        &mut self,
        poll: &mio::Poll,
        token: &mut dyn Iterator<Item = mio::Token>,
        interest: mio::Ready,
        poll_opts: mio::PollOpt,
    ) -> io::Result<()> {
        self.token = token.next().unwrap();
        poll.register(
            &EventedFd(&self.fd.as_raw_fd()),
            self.token,
            interest,
            poll_opts
        )
    }

    #[inline]
    fn reregister(&mut self, poll: &mio::Poll, interest: mio::Ready, poll_opts: mio::PollOpt) -> io::Result<()> {
        poll.reregister(
            &EventedFd(&self.fd.as_raw_fd()),
            self.token,
            interest,
            poll_opts
        )
    }

    #[inline]
    fn deregister(&mut self, poll: &mio::Poll) -> io::Result<()> {
        poll.deregister(&EventedFd(&self.fd.as_raw_fd()))
    }

    #[inline]
    fn reader(&mut self) -> &mut File {
        &mut self.fd
    }

    #[inline]
    fn read_token(&self) -> mio::Token {
        self.token
    }

    #[inline]
    fn writer(&mut self) -> &mut File {
        &mut self.fd
    }

    #[inline]
    fn write_token(&self) -> mio::Token {
        self.token
    }
}

/// Types that can produce a `libc::winsize`
pub trait ToWinsize {
    /// Get a `libc::winsize`
    fn to_winsize(&self) -> winsize;
}

impl<'a> ToWinsize for &'a SizeInfo {
    fn to_winsize(&self) -> winsize {
        winsize {
            ws_row: self.lines().0 as libc::c_ushort,
            ws_col: self.cols().0 as libc::c_ushort,
            ws_xpixel: self.width as libc::c_ushort,
            ws_ypixel: self.height as libc::c_ushort,
        }
    }
}

impl OnResize for i32 {
    fn on_resize(&mut self, size: &SizeInfo) {
        let win = size.to_winsize();

        let res = unsafe {
            libc::ioctl(*self, libc::TIOCSWINSZ, &win as *const _)
        };

        if res < 0 {
            die!("ioctl TIOCSWINSZ failed: {}", errno());
        }
    }
}

unsafe fn set_nonblocking(fd: c_int) {
    use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};

    let res = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL, 0) | O_NONBLOCK);
    assert_eq!(res, 0);
}

#[test]
fn test_get_pw_entry() {
    let mut buf: [i8; 1024] = [0; 1024];
    let _pw = get_pw_entry(&mut buf);
}
