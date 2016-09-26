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
use std::env;
use std::ffi::CStr;
use std::fs::File;
use std::mem;
use std::os::unix::io::FromRawFd;
use std::ptr;

use libc::{self, winsize, c_int, pid_t, WNOHANG, WIFEXITED, WEXITSTATUS, SIGCHLD};

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

enum Relation {
    Child,
    Parent(pid_t)
}

fn fork() -> Relation {
    let res = unsafe {
        libc::fork()
    };

    if res < 0 {
        die!("fork failed");
    }

    if res == 0 {
        Relation::Child
    } else {
        Relation::Parent(res)
    }
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

#[cfg(target_os = "macos")]
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
        libc::ioctl(fd, libc::TIOCSCTTY as _, 0)
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
fn get_pw_entry<'a>(buf: &'a mut [i8; 1024]) -> Passwd<'a> {
    // Create zeroed passwd struct
    let mut entry: libc::passwd = unsafe { ::std::mem::uninitialized() };

    let mut res: *mut libc::passwd = ptr::null_mut();

    // Try and read the pw file.
    let uid = unsafe { libc::getuid() };
    let status = unsafe {
        libc::getpwuid_r(uid, &mut entry, buf.as_mut_ptr(), buf.len(), &mut res)
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
    //
    // Transmute is used here to conveniently cast from the raw CStr to a &str with the appropriate
    // lifetime.
    Passwd {
        name: unsafe { mem::transmute(CStr::from_ptr(entry.pw_name).to_str().unwrap()) },
        passwd: unsafe { mem::transmute(CStr::from_ptr(entry.pw_passwd).to_str().unwrap()) },
        uid: entry.pw_uid,
        gid: entry.pw_gid,
        gecos: unsafe { mem::transmute(CStr::from_ptr(entry.pw_gecos).to_str().unwrap()) },
        dir: unsafe { mem::transmute(CStr::from_ptr(entry.pw_dir).to_str().unwrap()) },
        shell: unsafe { mem::transmute(CStr::from_ptr(entry.pw_shell).to_str().unwrap()) },
    }
}

/// Exec a shell
fn execsh() -> ! {
    let mut buf = [0; 1024];
    let pw = get_pw_entry(&mut buf);

    // setup environment
    env::set_var("LOGNAME", pw.name);
    env::set_var("USER", pw.name);
    env::set_var("SHELL", pw.shell);
    env::set_var("HOME", pw.dir);
    env::set_var("TERM", "xterm-256color"); // sigh

    unsafe {
        libc::signal(libc::SIGCHLD, libc::SIG_DFL);
        libc::signal(libc::SIGHUP, libc::SIG_DFL);
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGQUIT, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
        libc::signal(libc::SIGALRM, libc::SIG_DFL);
    }

    // pw.shell is null terminated
    let shell = unsafe { CStr::from_ptr(pw.shell.as_ptr() as *const _) };

    let argv = [shell.as_ptr(), ptr::null()];

    let res = unsafe {
        libc::execvp(shell.as_ptr(), argv.as_ptr())
    };

    if res < 0 {
        die!("execvp failed: {}", errno());
    }

    ::std::process::exit(1);
}

/// Create a new tty and return a handle to interact with it.
pub fn new(rows: u8, cols: u8) -> Tty {
    let (master, slave) = openpty(rows, cols);

    match fork() {
        Relation::Child => {
            unsafe {
                // Create a new process group
                libc::setsid();

                // Duplicate pty slave to be child stdin, stdoud, and stderr
                libc::dup2(slave, 0);
                libc::dup2(slave, 1);
                libc::dup2(slave, 2);
            }

            set_controlling_terminal(slave);

            // No longer need slave/master fds
            unsafe {
                libc::close(slave);
                libc::close(master);
            }

            // Exec a shell!
            execsh();
        },
        Relation::Parent(pid) => {
            unsafe {
                // Set PID for SIGCHLD handler
                PID = pid;

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

            Tty { fd: master }
        }
    }
}

pub struct Tty {
    fd: c_int,
}

impl Tty {
    /// Get reader for the TTY
    ///
    /// XXX File is a bad abstraction here; it closes the fd on drop
    pub fn reader(&self) -> File {
        unsafe {
            File::from_raw_fd(self.fd)
        }
    }

    pub fn resize(&self, rows: usize, cols: usize, px_x: usize, px_y: usize) {
        let win = winsize {
            ws_row: rows as libc::c_ushort,
            ws_col: cols as libc::c_ushort,
            ws_xpixel: px_x as libc::c_ushort,
            ws_ypixel: px_y as libc::c_ushort,
        };

        let res = unsafe {
            libc::ioctl(self.fd, libc::TIOCSWINSZ, &win as *const _)
        };

        if res < 0 {
            die!("ioctl TIOCSWINSZ failed: {}", errno());
        }
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
    let pw = get_pw_entry(&mut buf);
    println!("{:?}", pw);
}
