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
use std::ffi::CStr;
use std::fs::File;
use std::os::unix::io::FromRawFd;
use std::os::unix::process::CommandExt;
use std::ptr;
use std::process::{Command, Stdio};

use libc::{self, winsize, c_int, pid_t, WNOHANG, WIFEXITED, WEXITSTATUS, SIGCHLD, TIOCSCTTY};

use term::SizeInfo;
use display::OnResize;
use config::{Config, Shell};
use cli::Options;

/// Process ID of child process
///
/// Necessary to put this in static storage for `sigchld` to have access
static mut PID: pid_t = 0;

/// Exit flag
///
/// Calling exit() in the SIGCHLD handler sometimes causes opengl to deadlock,
/// and the process hangs. Instead, this flag is set, and its status can be
/// cheked via `process_should_exit`.
static mut SHOULD_EXIT: bool = false;

extern "C" fn sigchld(_a: c_int) {
    let mut status: c_int = 0;
    unsafe {
        let p = libc::waitpid(PID, &mut status, WNOHANG);
        if p < 0 {
            die!("Waiting for pid {} failed: {}\n", PID, errno());
        }

        if PID != p {
            return;
        }

        if !WIFEXITED(status) || WEXITSTATUS(status) != 0 {
            die!("child finished with error '{}'\n", status);
        }

        SHOULD_EXIT = true;
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
#[cfg(target_os = "linux")]
fn openpty(rows: u8, cols: u8) -> (c_int, c_int) {
    let mut master: c_int = 0;
    let mut slave: c_int = 0;

    let win = winsize {
        ws_row: rows as libc::c_ushort,
        ws_col: cols as libc::c_ushort,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let res = unsafe {
        libc::openpty(&mut master, &mut slave, ptr::null_mut(), ptr::null(), &win)
    };

    if res < 0 {
        die!("openpty failed");
    }

    (master, slave)
}

#[cfg(any(target_os = "macos",target_os = "freebsd"))]
fn openpty(rows: u8, cols: u8) -> (c_int, c_int) {
    let mut master: c_int = 0;
    let mut slave: c_int = 0;

    let mut win = winsize {
        ws_row: rows as libc::c_ushort,
        ws_col: cols as libc::c_ushort,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let res = unsafe {
        libc::openpty(&mut master, &mut slave, ptr::null_mut(), ptr::null_mut(), &mut win)
    };

    if res < 0 {
        die!("openpty failed");
    }

    (master, slave)
}

/// Really only needed on BSD, but should be fine elsewhere
fn set_controlling_terminal(fd: c_int) {
    let res = unsafe {
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
fn get_pw_entry(buf: &mut [i8; 1024]) -> Passwd {
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

/// Create a new tty and return a handle to interact with it.
pub fn new<T: ToWinsize>(config: &Config, options: &Options, size: T) -> Pty {
    let win = size.to_winsize();
    let mut buf = [0; 1024];
    let pw = get_pw_entry(&mut buf);

    let (master, slave) = openpty(win.ws_row as _, win.ws_col as _);

    let default_shell = &Shell::new(pw.shell);
    let shell = options.shell()
        .or_else(|| config.shell())
        .unwrap_or(&default_shell);

    let mut builder = Command::new(shell.program());
    for arg in shell.args() {
        builder.arg(arg);
    }

    // Setup child stdin/stdout/stderr as slave fd of pty
    builder.stdin(unsafe { Stdio::from_raw_fd(slave) });
    builder.stderr(unsafe { Stdio::from_raw_fd(slave) });
    builder.stdout(unsafe { Stdio::from_raw_fd(slave) });

    // Setup environment
    builder.env("LOGNAME", pw.name);
    builder.env("USER", pw.name);
    builder.env("SHELL", shell.program());
    builder.env("HOME", pw.dir);
    builder.env("TERM", "xterm-256color"); // sigh

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

    match builder.spawn() {
        Ok(child) => {
            unsafe {
                // Set PID for SIGCHLD handler
                PID = child.id() as _;

                // Handle SIGCHLD
                libc::signal(SIGCHLD, sigchld as _);

                // Parent doesn't need slave fd
                libc::close(slave);
            }
            unsafe {
                // Maybe this should be done outside of this function so nonblocking
                // isn't forced upon consumers. Although maybe it should be?
                set_nonblocking(master);
            }

            let pty = Pty { fd: master };
            pty.resize(size);
            pty
        },
        Err(err) => {
            die!("Command::spawn() failed: {}", err);
        }
    }
}

pub struct Pty {
    fd: c_int,
}

impl Pty {
    /// Get reader for the TTY
    ///
    /// XXX File is a bad abstraction here; it closes the fd on drop
    pub fn reader(&self) -> File {
        unsafe {
            File::from_raw_fd(self.fd)
        }
    }

    /// Resize the pty
    ///
    /// Tells the kernel that the window size changed with the new pixel
    /// dimensions and line/column counts.
    pub fn resize<T: ToWinsize>(&self, size: T) {
        let win = size.to_winsize();

        let res = unsafe {
            libc::ioctl(self.fd, libc::TIOCSWINSZ, &win as *const _)
        };

        if res < 0 {
            die!("ioctl TIOCSWINSZ failed: {}", errno());
        }
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

impl OnResize for Pty {
    fn on_resize(&mut self, size: &SizeInfo) {
        self.resize(size);
    }
}

unsafe fn set_nonblocking(fd: c_int) {
    use libc::{fcntl, F_SETFL, F_GETFL, O_NONBLOCK};

    let res = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL, 0) | O_NONBLOCK);
    assert_eq!(res, 0);
}

#[test]
fn test_get_pw_entry() {
    let mut buf: [i8; 1024] = [0; 1024];
    let _pw = get_pw_entry(&mut buf);
}
