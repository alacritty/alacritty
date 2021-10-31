use std::ffi::OsStr;
use std::fmt::Debug;
use std::io;
#[cfg(not(windows))]
use {
    alacritty_terminal::tty, std::error::Error, std::os::unix::process::CommandExt,
    std::path::PathBuf,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

use log::{debug, warn};
#[cfg(windows)]
use winapi::um::winbase::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};

#[cfg(target_os = "macos")]
use crate::macos;

/// Start the daemon and log error on failure.
pub fn start_daemon<I, S>(program: &str, args: I)
where
    I: IntoIterator<Item = S> + Debug + Copy,
    S: AsRef<OsStr>,
{
    match spawn_daemon(program, args) {
        Ok(_) => debug!("Launched {} with args {:?}", program, args),
        Err(_) => warn!("Unable to launch {} with args {:?}", program, args),
    }
}

#[cfg(not(windows))]
pub fn foreground_process_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut pid = unsafe { libc::tcgetpgrp(tty::master_fd()) };
    if pid < 0 {
        pid = tty::child_pid();
    }

    #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
    let link_path = format!("/proc/{}/cwd", pid);
    #[cfg(target_os = "freebsd")]
    let link_path = format!("/compat/linux/proc/{}/cwd", pid);

    #[cfg(not(target_os = "macos"))]
    let cwd = std::fs::read_link(link_path)?;

    #[cfg(target_os = "macos")]
    let cwd = macos::proc::cwd(pid)?;

    Ok(cwd)
}

#[cfg(windows)]
fn spawn_daemon<I, S>(program: &str, args: I) -> io::Result<()>
where
    I: IntoIterator<Item = S> + Copy,
    S: AsRef<OsStr>,
{
    // Setting all the I/O handles to null and setting the
    // CREATE_NEW_PROCESS_GROUP and CREATE_NO_WINDOW has the effect
    // that console applications will run without opening a new
    // console window.
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
}

#[cfg(not(windows))]
fn spawn_daemon<I, S>(program: &str, args: I) -> io::Result<()>
where
    I: IntoIterator<Item = S> + Copy,
    S: AsRef<OsStr>,
{
    unsafe {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .pre_exec(|| {
                match libc::fork() {
                    -1 => return Err(io::Error::last_os_error()),
                    0 => (),
                    _ => libc::_exit(0),
                }

                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }

                Ok(())
            })
            .spawn()?
            .wait()
            .map(|_| ())
    }
}
