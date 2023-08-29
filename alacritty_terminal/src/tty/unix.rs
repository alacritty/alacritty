//! TTY related functionality.

use std::ffi::CStr;
use std::fs::File;
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::mem::MaybeUninit;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::{env, ptr};

use bitflags::bitflags;
use libc::{self, c_int, winsize, TIOCSCTTY};
use log::{error, warn};
use mio::unix::EventedFd;
use nix::pty::openpty;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use nix::sys::termios::{self, InputFlags, SetArg};
use signal_hook::consts as sigconsts;
use signal_hook_mio::v0_6::Signals;

use crate::config::PtyConfig;
use crate::event::{OnResize, WindowSize};
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite};

/// Get raw fds for master/slave ends of a new PTY.
fn make_pty(size: winsize) -> Result<(RawFd, RawFd)> {
    let mut window_size = size;
    window_size.ws_xpixel = 0;
    window_size.ws_ypixel = 0;

    let ends = openpty(Some(&window_size), None)?;

    Ok((ends.master, ends.slave))
}

/// Really only needed on BSD, but should be fine elsewhere.
fn set_controlling_terminal(fd: c_int) -> Result<()> {
    let res = unsafe {
        // TIOSCTTY changes based on platform and the `ioctl` call is different
        // based on architecture (32/64). So a generic cast is used to make sure
        // there are no issues. To allow such a generic cast the clippy warning
        // is disabled.
        #[allow(clippy::cast_lossless)]
        libc::ioctl(fd, TIOCSCTTY as _, 0)
    };

    if res < 0 {
        Err(Error::last_os_error())
    } else {
        Ok(())
    }
}

#[derive(Debug)]
struct Passwd<'a> {
    name: &'a str,
    dir: &'a str,
    shell: &'a str,
}

/// Return a Passwd struct with pointers into the provided buf.
///
/// # Unsafety
///
/// If `buf` is changed while `Passwd` is alive, bad thing will almost certainly happen.
fn get_pw_entry(buf: &mut [i8; 1024]) -> Result<Passwd<'_>> {
    // Create zeroed passwd struct.
    let mut entry: MaybeUninit<libc::passwd> = MaybeUninit::uninit();

    let mut res: *mut libc::passwd = ptr::null_mut();

    // Try and read the pw file.
    let uid = unsafe { libc::getuid() };
    let status = unsafe {
        libc::getpwuid_r(uid, entry.as_mut_ptr(), buf.as_mut_ptr() as *mut _, buf.len(), &mut res)
    };
    let entry = unsafe { entry.assume_init() };

    if status < 0 {
        return Err(Error::new(ErrorKind::Other, "getpwuid_r failed"));
    }

    if res.is_null() {
        return Err(Error::new(ErrorKind::Other, "pw not found"));
    }

    // Sanity check.
    assert_eq!(entry.pw_uid, uid);

    // Build a borrowed Passwd struct.
    Ok(Passwd {
        name: unsafe { CStr::from_ptr(entry.pw_name).to_str().unwrap() },
        dir: unsafe { CStr::from_ptr(entry.pw_dir).to_str().unwrap() },
        shell: unsafe { CStr::from_ptr(entry.pw_shell).to_str().unwrap() },
    })
}

pub struct Pty {
    child: Child,
    file: File,
    token: mio::Token,
    signals: Signals,
    signals_token: mio::Token,
}

impl Pty {
    pub fn child(&self) -> &Child {
        &self.child
    }

    pub fn file(&self) -> &File {
        &self.file
    }
}

/// User information that is required for a new shell session.
struct ShellUser {
    user: String,
    home: String,
    shell: String,
}

impl ShellUser {
    /// look for shell, username, longname, and home dir in the respective environment variables
    /// before falling back on looking in to `passwd`.
    fn from_env() -> Result<Self> {
        let mut buf = [0; 1024];
        let pw = get_pw_entry(&mut buf);

        let user = match env::var("USER") {
            Ok(user) => user,
            Err(_) => match pw {
                Ok(ref pw) => pw.name.to_owned(),
                Err(err) => return Err(err),
            },
        };

        let home = match env::var("HOME") {
            Ok(home) => home,
            Err(_) => match pw {
                Ok(ref pw) => pw.dir.to_owned(),
                Err(err) => return Err(err),
            },
        };

        let shell = match env::var("SHELL") {
            Ok(shell) => shell,
            Err(_) => match pw {
                Ok(ref pw) => pw.shell.to_owned(),
                Err(err) => return Err(err),
            },
        };

        Ok(Self { user, home, shell })
    }
}

#[cfg(not(target_os = "macos"))]
fn default_shell_command(shell: &str, _user: &str) -> Command {
    Command::new(shell)
}

#[cfg(target_os = "macos")]
fn default_shell_command(shell: &str, user: &str) -> Command {
    let shell_name = shell.rsplit('/').next().unwrap();

    // On macOS, use the `login` command so the shell will appear as a tty session.
    let mut login_command = Command::new("/usr/bin/login");

    // Exec the shell with argv[0] prepended by '-' so it becomes a login shell.
    // `login` normally does this itself, but `-l` disables this.
    let exec = format!("exec -a -{} {}", shell_name, shell);

    // -f: Bypasses authentication for the already-logged-in user.
    // -l: Skips changing directory to $HOME and prepending '-' to argv[0].
    // -p: Preserves the environment.
    //
    // XXX: we use zsh here over sh due to `exec -a`.
    login_command.args(["-flp", user, "/bin/zsh", "-c", &exec]);
    login_command
}

bitflags! {
    /// Status of the child process reported by pre_exec()
    pub struct ChildStatus: u8 {
        // The status is sent over PTY. Make sure not to send any control characters.
        const BASE                = 0b0100_0000; // Base pty-safe value - no errors
        const SETSID_ERROR        = 0b0000_0001; // Cannot set session ID
        const TIOCSCTTY_ERROR     = 0b0000_0010; // Cannot set controlling terminal
        const CHDIR_ERROR         = 0b0000_0100; // Cannot change working directory
    }
}

/// Read status from the child process, log warnings if any.
fn check_child_status(file: &mut File) -> Result<()> {
    // Read status byte from the child process
    let mut status_buffer = [0u8; 1];
    file.read_exact(&mut status_buffer)?;
    let pre_exec_status = ChildStatus::from_bits(status_buffer[0]).unwrap_or_else(|| {
        warn!("Cannot decode child process status {:#2x}", status_buffer[0]);
        ChildStatus::BASE
    });

    if pre_exec_status.contains(ChildStatus::SETSID_ERROR) {
        warn!("Child process failed to set session ID");
    }

    if pre_exec_status.contains(ChildStatus::TIOCSCTTY_ERROR) {
        warn!("Child process failed to set controlling terminal");
    }

    if pre_exec_status.contains(ChildStatus::CHDIR_ERROR) {
        warn!("Child process failed to change directory");
    }

    Ok(())
}

/// Create a new TTY and return a handle to interact with it.
pub fn new(config: &PtyConfig, window_size: WindowSize, window_id: u64) -> Result<Pty> {
    let (master, slave) = make_pty(window_size.to_winsize())?;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if let Ok(mut termios) = termios::tcgetattr(master) {
        // Set character encoding to UTF-8.
        termios.input_flags.set(InputFlags::IUTF8, true);
        let _ = termios::tcsetattr(master, SetArg::TCSANOW, &termios);
    }

    let user = ShellUser::from_env()?;

    let mut builder = if let Some(shell) = config.shell.as_ref() {
        let mut cmd = Command::new(shell.program());
        cmd.args(shell.args());
        cmd
    } else {
        default_shell_command(&user.shell, &user.user)
    };

    // Setup child stdin/stdout/stderr as slave fd of PTY.
    // Ownership of fd is transferred to the Stdio structs and will be closed by them at the end of
    // this scope. (It is not an issue that the fd is closed three times since File::drop ignores
    // error on libc::close.).
    builder.stdin(unsafe { Stdio::from_raw_fd(slave) });
    builder.stderr(unsafe { Stdio::from_raw_fd(slave) });
    builder.stdout(unsafe { Stdio::from_raw_fd(slave) });

    // Setup shell environment.
    let window_id = window_id.to_string();
    builder.env("ALACRITTY_WINDOW_ID", &window_id);
    builder.env("USER", user.user);
    builder.env("HOME", user.home);

    // Set Window ID for clients relying on X11 hacks.
    builder.env("WINDOWID", window_id);

    let working_directory = config.working_directory.clone();

    unsafe {
        builder.pre_exec(move || {
            let mut status_flag = ChildStatus::BASE;

            // Create a new process group.
            let err = libc::setsid();
            if err == -1 {
                status_flag |= ChildStatus::SETSID_ERROR;
            }

            if set_controlling_terminal(slave).is_err() {
                status_flag |= ChildStatus::TIOCSCTTY_ERROR;
            }

            // Handle set working directory option.
            if let Some(dir) = &working_directory {
                env::set_current_dir(dir)
                    .unwrap_or_else(|_| status_flag |= ChildStatus::CHDIR_ERROR);
            }

            let status_buffer = [status_flag.bits()];
            let _ = File::from_raw_fd(slave).write_all(&status_buffer);

            // No longer need slave/master fds.
            libc::close(slave);
            libc::close(master);

            libc::signal(libc::SIGCHLD, libc::SIG_DFL);
            libc::signal(libc::SIGHUP, libc::SIG_DFL);
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            libc::signal(libc::SIGQUIT, libc::SIG_DFL);
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGALRM, libc::SIG_DFL);

            Ok(())
        });
    }

    // Prepare signal handling before spawning child.
    let signals = Signals::new([sigconsts::SIGCHLD]).expect("error preparing signal handling");

    match builder.spawn() {
        Ok(child) => {
            unsafe {
                // Maybe this should be done outside of this function so nonblocking
                // isn't forced upon consumers. Although maybe it should be?
                set_nonblocking(master);
            }

            let mut file = unsafe { File::from_raw_fd(master) };
            check_child_status(&mut file)?;

            let mut pty = Pty {
                child,
                file,
                token: mio::Token::from(0),
                signals,
                signals_token: mio::Token::from(0),
            };
            pty.on_resize(window_size);
            Ok(pty)
        },
        Err(err) => Err(Error::new(
            err.kind(),
            format!(
                "Failed to spawn command '{}': {}",
                builder.get_program().to_string_lossy(),
                err
            ),
        )),
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Make sure the PTY is terminated properly.
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGHUP);
        }
        let _ = self.child.wait();
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
    ) -> Result<()> {
        self.token = token.next().unwrap();
        poll.register(&EventedFd(&self.file.as_raw_fd()), self.token, interest, poll_opts)?;

        self.signals_token = token.next().unwrap();
        poll.register(
            &self.signals,
            self.signals_token,
            mio::Ready::readable(),
            mio::PollOpt::level(),
        )
    }

    #[inline]
    fn reregister(
        &mut self,
        poll: &mio::Poll,
        interest: mio::Ready,
        poll_opts: mio::PollOpt,
    ) -> Result<()> {
        poll.reregister(&EventedFd(&self.file.as_raw_fd()), self.token, interest, poll_opts)?;

        poll.reregister(
            &self.signals,
            self.signals_token,
            mio::Ready::readable(),
            mio::PollOpt::level(),
        )
    }

    #[inline]
    fn deregister(&mut self, poll: &mio::Poll) -> Result<()> {
        poll.deregister(&EventedFd(&self.file.as_raw_fd()))?;
        poll.deregister(&self.signals)
    }

    #[inline]
    fn reader(&mut self) -> &mut File {
        &mut self.file
    }

    #[inline]
    fn read_token(&self) -> mio::Token {
        self.token
    }

    #[inline]
    fn writer(&mut self) -> &mut File {
        &mut self.file
    }

    #[inline]
    fn write_token(&self) -> mio::Token {
        self.token
    }
}

impl EventedPty for Pty {
    #[inline]
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.signals.pending().next().and_then(|signal| {
            if signal != sigconsts::SIGCHLD {
                return None;
            }

            match self.child.try_wait() {
                Err(e) => {
                    error!("Error checking child process termination: {}", e);
                    None
                },
                Ok(None) => None,
                Ok(_) => Some(ChildEvent::Exited),
            }
        })
    }

    #[inline]
    fn child_event_token(&self) -> mio::Token {
        self.signals_token
    }
}

impl OnResize for Pty {
    /// Resize the PTY.
    ///
    /// Tells the kernel that the window size changed with the new pixel
    /// dimensions and line/column counts.
    fn on_resize(&mut self, window_size: WindowSize) {
        let win = window_size.to_winsize();

        let res = unsafe { libc::ioctl(self.file.as_raw_fd(), libc::TIOCSWINSZ, &win as *const _) };

        if res < 0 {
            warn!("ioctl TIOCSWINSZ failed: {}", Error::last_os_error());
        }
    }
}

/// Types that can produce a `libc::winsize`.
pub trait ToWinsize {
    /// Get a `libc::winsize`.
    fn to_winsize(self) -> winsize;
}

impl ToWinsize for WindowSize {
    fn to_winsize(self) -> winsize {
        let ws_row = self.num_lines as libc::c_ushort;
        let ws_col = self.num_cols as libc::c_ushort;

        let ws_xpixel = ws_col * self.cell_width as libc::c_ushort;
        let ws_ypixel = ws_row * self.cell_height as libc::c_ushort;
        winsize { ws_row, ws_col, ws_xpixel, ws_ypixel }
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
    let _pw = get_pw_entry(&mut buf).unwrap();
}
