use std::ffi::OsStr;
#[cfg(not(any(target_os = "macos", windows)))]
use std::fs;
use std::io;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

#[rustfmt::skip]
#[cfg(not(windows))]
use {
    std::error::Error,
    std::os::unix::process::CommandExt,
    std::os::unix::io::RawFd,
    std::path::PathBuf,
};

#[cfg(not(windows))]
use libc::pid_t;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};

#[cfg(target_os = "macos")]
use crate::macos;

/// Start a new process in the background.
#[cfg(windows)]
pub fn spawn_daemon<I, S>(program: &str, args: I) -> io::Result<()>
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

/// Start a new process in the background.
#[cfg(not(windows))]
pub fn spawn_daemon<I, S>(
    program: &str,
    args: I,
    master_fd: RawFd,
    shell_pid: u32,
) -> io::Result<()>
where
    I: IntoIterator<Item = S> + Copy,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Ok(cwd) = foreground_process_path(master_fd, shell_pid) {
        command.current_dir(cwd);
    }
    unsafe {
        command
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

/// Get working directory of controlling process.
#[cfg(not(windows))]
pub fn foreground_process_path(
    master_fd: RawFd,
    shell_pid: u32,
) -> Result<PathBuf, Box<dyn Error>> {
    let mut pid = unsafe { libc::tcgetpgrp(master_fd) };
    if pid < 0 {
        pid = shell_pid as pid_t;
    }

    #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
    let link_path = format!("/proc/{pid}/cwd");
    #[cfg(target_os = "freebsd")]
    let link_path = format!("/compat/linux/proc/{}/cwd", pid);

    #[cfg(not(target_os = "macos"))]
    let cwd = fs::read_link(link_path)?;

    #[cfg(target_os = "macos")]
    let cwd = macos::proc::cwd(pid)?;

    Ok(cwd)
}
