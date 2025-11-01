#[cfg(target_os = "openbsd")]
use std::ffi::CStr;
use std::ffi::OsStr;
#[cfg(not(any(target_os = "macos", target_os = "openbsd", windows)))]
use std::fs;
use std::io;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
#[cfg(target_os = "openbsd")]
use std::ptr;

#[rustfmt::skip]
#[cfg(not(windows))]
use {
    std::env,
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

    let working_directory = foreground_process_path(master_fd, shell_pid).ok();
    unsafe {
        command
            .pre_exec(move || {
                match libc::fork() {
                    -1 => return Err(io::Error::last_os_error()),
                    0 => (),
                    _ => libc::_exit(0),
                }

                // Copy foreground process' working directory, ignoring invalid paths.
                if let Some(working_directory) = working_directory.as_ref() {
                    let _ = env::set_current_dir(working_directory);
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
#[cfg(not(any(windows, target_os = "openbsd")))]
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

#[cfg(target_os = "openbsd")]
pub fn foreground_process_path(
    master_fd: RawFd,
    shell_pid: u32,
) -> Result<PathBuf, Box<dyn Error>> {
    let mut pid = unsafe { libc::tcgetpgrp(master_fd) };
    if pid < 0 {
        pid = shell_pid as pid_t;
    }
    let name = [libc::CTL_KERN, libc::KERN_PROC_CWD, pid];
    let mut buf = [0u8; libc::PATH_MAX as usize];
    let result = unsafe {
        libc::sysctl(
            name.as_ptr(),
            name.len().try_into().unwrap(),
            buf.as_mut_ptr() as *mut _,
            &mut buf.len() as *mut _,
            ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        Err(io::Error::last_os_error().into())
    } else {
        let foreground_path = unsafe { CStr::from_ptr(buf.as_ptr().cast()) }.to_str()?;
        Ok(PathBuf::from(foreground_path))
    }
}
