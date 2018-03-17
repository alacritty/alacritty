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

use term::SizeInfo;
use display::OnResize;
use config::{Config, Shell};
use cli::Options;
use mio;
use std::os::raw::c_void;

#[cfg(windows)]
use winapi::um::synchapi::WaitForSingleObject;
#[cfg(windows)]
use winapi::um::winbase::{WAIT_OBJECT_0, FILE_FLAG_OVERLAPPED};
#[cfg(windows)]
use winapi::shared::winerror::WAIT_TIMEOUT;
#[cfg(windows)]
use mio_named_pipes::NamedPipe;
#[cfg(windows)]
use winpty::{ConfigFlags, MouseMode, SpawnConfig, SpawnFlags, Winpty};
#[cfg(windows)]
use winpty::Config as WinptyConfig;
#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use mio::Evented;
#[cfg(windows)]
use std::env;
#[cfg(windows)]
use std::cell::UnsafeCell;
#[cfg(windows)]
use dunce::canonicalize;

#[cfg(not(windows))]
use std::os::unix::io::FromRawFd;
#[cfg(not(windows))]
use std::fs::File;
#[cfg(not(windows))]
use std::os::unix::process::CommandExt;
#[cfg(not(windows))]
use libc::{self, c_int, pid_t, winsize, SIGCHLD, TIOCSCTTY, WNOHANG};
#[cfg(not(windows))]
use std::process::{Command, Stdio};
#[cfg(not(windows))]
use std::ffi::CStr;

// How long the agent should wait for any RPC request
// This is a placeholder value until we see how often long responses happen
#[cfg(windows)]
const AGENT_TIMEOUT: u32 = 10000;

/// Process ID of child process
///
/// Necessary to put this in static storage for `sigchld` to have access
#[cfg(not(windows))]
static mut PID: pid_t = 0;

// Handle to the winpty agent process. Required so we know when it closes.
static mut HANDLE: *mut c_void = 0usize as *mut c_void;

/// Exit flag
///
/// Calling exit() in the SIGCHLD handler sometimes causes opengl to deadlock,
/// and the process hangs. Instead, this flag is set, and its status can be
/// checked via `process_should_exit`.
#[cfg(not(windows))]
static mut SHOULD_EXIT: bool = false;

#[cfg(not(windows))]
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

#[cfg(not(windows))]
pub fn process_should_exit() -> bool {
    unsafe { SHOULD_EXIT }
}

#[cfg(windows)]
pub fn process_should_exit() -> bool {
    unsafe {
        match WaitForSingleObject(HANDLE, 0) {
            // Process has exited
            WAIT_OBJECT_0 => {
                info!("wait_object_0");
                true
            }
            // Reached timeout of 0, process has not exited
            WAIT_TIMEOUT => false,
            // Error checking process, winpty gave us a bad agent handle?
            _ => {
                info!("Bad exit: {}", ::std::io::Error::last_os_error());
                true
            }
        }
    }
}

/// Get the current value of errno
#[cfg(not(windows))]
fn errno() -> c_int {
    ::errno::errno().0
}

/// Get raw fds for master/slave ends of a new pty
#[cfg(target_os = "linux")]
fn openpty(rows: u8, cols: u8) -> (c_int, c_int) {
    let mut master: c_int = 0;
    let mut slave: c_int = 0;

    let win = winsize {
        ws_row: libc::c_ushort::from(rows),
        ws_col: libc::c_ushort::from(cols),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let res = unsafe { libc::openpty(&mut master, &mut slave, ptr::null_mut(), ptr::null(), &win) };

    if res < 0 {
        die!("openpty failed");
    }

    (master, slave)
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
fn openpty(rows: u8, cols: u8) -> (c_int, c_int) {
    let mut master: c_int = 0;
    let mut slave: c_int = 0;

    let mut win = winsize {
        ws_row: libc::c_ushort::from(rows),
        ws_col: libc::c_ushort::from(cols),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let res = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut win,
        )
    };

    if res < 0 {
        die!("openpty failed");
    }

    (master, slave)
}

/// Really only needed on BSD, but should be fine elsewhere
#[cfg(not(windows))]
fn set_controlling_terminal(fd: c_int) {
    let res = unsafe {
        // TIOSCTTY changes based on platform and the `ioctl` call is different
        // based on architecture (32/64). So a generic cast is used to make sure
        // there are no issues. To allow such a generic cast the clippy warning
        // is disabled.
        #[cfg_attr(feature = "clippy", allow(cast_lossless))]
        libc::ioctl(fd, TIOCSCTTY as _, 0)
    };

    if res < 0 {
        die!("ioctl TIOCSCTTY failed: {}", errno());
    }
}

#[cfg(not(windows))]
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
#[cfg(not(windows))]
fn get_pw_entry(buf: &mut [i8; 1024]) -> Passwd {
    // Create zeroed passwd struct
    let mut entry: libc::passwd = unsafe { ::std::mem::uninitialized() };

    let mut res: *mut libc::passwd = ptr::null_mut();

    // Try and read the pw file.
    let uid = unsafe { libc::getuid() };
    let status = unsafe {
        libc::getpwuid_r(
            uid,
            &mut entry,
            buf.as_mut_ptr() as *mut _,
            buf.len(),
            &mut res,
        )
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
///
/// On windows this starts the winpty agent to interact with the console
#[cfg(not(windows))]
pub fn new<T: ToWinsize>(
    config: &Config,
    options: &Options,
    size: T,
    window_id: Option<usize>,
) -> Pty {
    let win = size.to_winsize();
    let mut buf = [0; 1024];
    let pw = get_pw_entry(&mut buf);

    let (master, slave) = openpty(win.ws_row as _, win.ws_col as _);

    let default_shell = &Shell::new(pw.shell);
    let shell = config.shell().unwrap_or(default_shell);

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

    // Setup environment
    builder.env("LOGNAME", pw.name);
    builder.env("USER", pw.name);
    builder.env("SHELL", shell.program());
    builder.env("HOME", pw.dir);
    builder.env("TERM", "xterm-256color"); // default term until we can supply our own
    if let Some(window_id) = window_id {
        builder.env("WINDOWID", format!("{}", window_id));
    }
    for (key, value) in config.env().iter() {
        builder.env(key, value);
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

            let pty = Pty { fd: master };
            pty.resize(size);
            pty
        }
        Err(err) => {
            die!("Command::spawn() failed: {}", err);
        }
    }
}
#[cfg(windows)]
pub fn new<'a>(
    config: &Config,
    options: &Options,
    size: &SizeInfo,
    _window_id: Option<usize>,
) -> Pty<'a, NamedPipe, NamedPipe> {
    // Create config
    let mut wconfig = WinptyConfig::new(ConfigFlags::empty()).unwrap();

    wconfig.set_initial_size(size.cols().0 as i32, size.lines().0 as i32);
    wconfig.set_mouse_mode(MouseMode::Auto);
    wconfig.set_agent_timeout(AGENT_TIMEOUT);

    // Start agent
    let mut winpty = Winpty::open(&wconfig).unwrap();
    let (conin, conout) = (winpty.conin_name(), winpty.conout_name());

    // Get process commandline
    let default_shell = &Shell::new(env::var("COMSPEC").unwrap_or("cmd".into()));
    let shell = config.shell().unwrap_or(default_shell);
    let initial_command = options.command().unwrap_or(shell);
    let mut cmdline = initial_command.args().to_vec();
    cmdline.insert(0, initial_command.program().into());

    // Warning, here be borrow hell
    let cwd = options.working_dir.as_ref().map(|dir| canonicalize(dir).unwrap());
    let cwd = cwd.as_ref().map(|dir| dir.to_str()).unwrap();

    // Spawn process
    let spawnconfig = SpawnConfig::new(
        // This may be problematic if we can't tell immediately when the process shut down
        SpawnFlags::AUTO_SHUTDOWN | SpawnFlags::EXIT_AFTER_SHUTDOWN,
        None, // appname
        Some(&cmdline.join(" ")),
        cwd,
        None, // Env
    ).unwrap();

    let default_opts = &mut OpenOptions::new();
    default_opts
        .share_mode(0)
        .custom_flags(FILE_FLAG_OVERLAPPED);

    let (conout_pipe, conin_pipe);
    unsafe {
        conout_pipe = NamedPipe::from_raw_handle(
            default_opts
                .clone()
                .read(true)
                .open(conout)
                .unwrap()
                .into_raw_handle(),
        );
        conin_pipe = NamedPipe::from_raw_handle(
            default_opts
                .clone()
                .write(true)
                .open(conin)
                .unwrap()
                .into_raw_handle(),
        );
    };

    if let Some(err) = conout_pipe.connect().err() {
        if err.kind() != io::ErrorKind::WouldBlock {
            panic!(err);
        }
    }
    assert!(conout_pipe.take_error().unwrap().is_none());

    if let Some(err) = conin_pipe.connect().err() {
        if err.kind() != io::ErrorKind::WouldBlock {
            panic!(err);
        }
    }
    assert!(conin_pipe.take_error().unwrap().is_none());

    winpty.spawn(&spawnconfig, None, None).unwrap(); // Process handle, thread handle

    unsafe {
        HANDLE = winpty.raw_handle();
    }

    Pty {
        winpty: UnsafeCell::new(winpty),
        conout: conout_pipe,
        conin: conin_pipe,
        // Placeholder tokens that are overwritten
        read_token: 0.into(),
        write_token: 0.into(),
    }
}

#[cfg(not(windows))]
pub struct Pty {
    fd: c_int,
}
#[cfg(windows)]
pub struct Pty<'a, R: io::Read + Evented + Send, W: io::Write + Evented + Send> {
    // TODO: Provide methods for accessing this safely
    pub winpty: UnsafeCell<Winpty<'a>>,

    conout: R,
    conin: W,
    read_token: mio::Token,
    write_token: mio::Token,
}

#[cfg(not(windows))]
impl Pty {
    /// Get reader for the TTY
    ///
    /// XXX File is a bad abstraction here; it closes the fd on drop
    pub fn reader(&self) -> File {
        unsafe { File::from_raw_fd(self.fd) }
    }

    /// Resize the pty
    ///
    /// Tells the kernel that the window size changed with the new pixel
    /// dimensions and line/column counts.
    ///
    /// On windows only line/column counts are used.
    pub fn resize<T: ToWinsize>(&self, size: T) {
        let win = size.to_winsize();

        let res = unsafe { libc::ioctl(self.fd, libc::TIOCSWINSZ, &win as *const _) };

        if res < 0 {
            die!("ioctl TIOCSWINSZ failed: {}", errno());
        }
    }
}
#[cfg(windows)]
impl<'a> EventedRW<NamedPipe, NamedPipe> for Pty<'a, NamedPipe, NamedPipe> {
    fn register(
        &mut self,
        poll: &mio::Poll,
        token: &mut Iterator<Item = &usize>,
        interest: mio::Ready,
        poll_opts: mio::PollOpt,
    ) {
        self.read_token = (*token.next().unwrap()).into();
        self.write_token = (*token.next().unwrap()).into();
        if interest.is_readable() {
            poll.register(
                &self.conout,
                self.read_token,
                mio::Ready::readable(),
                poll_opts,
            ).unwrap();
        } else {
            poll.register(
                &self.conout,
                self.read_token,
                mio::Ready::empty(),
                poll_opts,
            ).unwrap();
        }
        if interest.is_writable() {
            poll.register(
                &self.conin,
                self.write_token,
                mio::Ready::writable(),
                poll_opts,
            ).unwrap();
        } else {
            poll.register(
                &self.conin,
                self.write_token,
                mio::Ready::empty(),
                poll_opts,
            ).unwrap();
        }
    }
    fn reregister(&mut self, poll: &mio::Poll, interest: mio::Ready, poll_opts: mio::PollOpt) {
        if interest.is_readable() {
            poll.reregister(
                &self.conout,
                self.read_token,
                mio::Ready::readable(),
                poll_opts,
            ).unwrap();
        } else {
            poll.reregister(
                &self.conout,
                self.write_token,
                mio::Ready::empty(),
                poll_opts,
            ).unwrap();
        }
        if interest.is_writable() {
            poll.reregister(
                &self.conin,
                self.write_token,
                mio::Ready::writable(),
                poll_opts,
            ).unwrap();
        } else {
            poll.reregister(
                &self.conin,
                self.write_token,
                mio::Ready::empty(),
                poll_opts,
            ).unwrap();
        }
    }
    fn deregister(&mut self, poll: &mio::Poll) {
        poll.deregister(&self.conout).unwrap();
        poll.deregister(&self.conin).unwrap();
    }

    fn reader(&mut self) -> &mut NamedPipe {
        &mut self.conout
    }
    fn read_token(&self) -> mio::Token {
        self.read_token
    }
    fn writer(&mut self) -> &mut NamedPipe {
        &mut self.conin
    }
    fn write_token(&self) -> mio::Token {
        self.write_token
    }
}
// TODO:
#[cfg(not(windows))]
impl PTY<RawFd, RawFd> for Pty {
    fn register(&mut self, poll: mio::Poll, interest: mio::Ready) {}
    fn reregister(&mut self, poll: mio::Poll, interest: mio::Ready) {}
    fn deregister(&mut self, poll: mio::Poll) {}

    fn reader(&mut self) -> (mio::Token, &mut RawFd) {}
    fn writer(&mut self) -> (mio::Token, &mut RawFd) {}
    fn resize(&self, sizeinfo: &SizeInfo) {}
}

/// This trait defines the behaviour needed to read and/or write to a stream.
/// It defines an abstraction over mio's interface in order to allow either one
/// read/write object or a seperate read and write object.
// TODO: Maybe return results here instead of panicing
// FIXME: There's probably a much more elegant way to do this
pub trait EventedRW<R: io::Read, W: io::Write> {
    fn register(&mut self, &mio::Poll, &mut Iterator<Item = &usize>, mio::Ready, mio::PollOpt);
    fn reregister(&mut self, &mio::Poll, mio::Ready, mio::PollOpt);
    fn deregister(&mut self, &mio::Poll);

    fn reader(&mut self) -> &mut R;
    fn read_token(&self) -> mio::Token;
    fn writer(&mut self) -> &mut W;
    fn write_token(&self) -> mio::Token;
}

/// Types that can produce a `libc::winsize`
#[cfg(not(windows))]
pub trait ToWinsize {
    /// Get a `libc::winsize`
    fn to_winsize(&self) -> winsize;
}

#[cfg(not(windows))]
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

#[cfg(windows)]
impl<'a> OnResize for Winpty<'a> {
    fn on_resize(&mut self, sizeinfo: &SizeInfo) {
        if sizeinfo.cols().0 > 0 && sizeinfo.lines().0 > 0 {
            self.set_size(sizeinfo.cols().0, sizeinfo.lines().0)
                .unwrap_or_else(|_| info!("Unable to set winpty size, did it die?"));
        }
    }
}
#[cfg(not(windows))]
impl OnResize for Pty {
    fn on_resize(&mut self, size: &SizeInfo) {
        self.resize(&size);
    }
}

#[cfg(not(windows))]
unsafe fn set_nonblocking(fd: c_int) {
    use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};

    let res = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL, 0) | O_NONBLOCK);
    assert_eq!(res, 0);
}

#[cfg(not(windows))]
#[test]
fn test_get_pw_entry() {
    let mut buf: [i8; 1024] = [0; 1024];
    let _pw = get_pw_entry(&mut buf);
}
