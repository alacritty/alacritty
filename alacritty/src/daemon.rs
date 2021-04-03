use std::ffi::OsStr;
use std::fmt::Debug;
use std::io::{self, Write};
#[cfg(not(windows))]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};

use log::{debug, warn};

#[cfg(windows)]
use winapi::um::winbase::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};

/// Start the daemon and log error on failure.
pub fn start_daemon<I, S>(program: &str, args: I, input: Option<&str>)
where
    I: IntoIterator<Item = S> + Debug + Copy,
    S: AsRef<OsStr>,
{
    match spawn_daemon(program, args, input) {
        Ok(_) => debug!("Launched {} with args {:?}", program, args),
        Err(_) => warn!("Unable to launch {} with args {:?}", program, args),
    }
}

fn write_input(child: &mut Child, input: Option<&str>) -> io::Result<()> {
    if let Some(s) = input {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(s.as_bytes())?;
    }
    Ok(())
}

#[cfg(windows)]
fn spawn_daemon<I, S>(program: &str, args: I, input: Option<&str>) -> io::Result<()>
where
    I: IntoIterator<Item = S> + Copy,
    S: AsRef<OsStr>,
{
    // Setting all the I/O handles to null and setting the
    // CREATE_NEW_PROCESS_GROUP and CREATE_NO_WINDOW has the effect
    // that console applications will run without opening a new
    // console window.
    let mut child = Command::new(program)
        .args(args)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()?;
    write_input(&mut child, input)
}

#[cfg(not(windows))]
fn spawn_daemon<I, S>(program: &str, args: I, input: Option<&str>) -> io::Result<()>
where
    I: IntoIterator<Item = S> + Copy,
    S: AsRef<OsStr>,
{
    unsafe {
        let mut child = Command::new(program)
            .args(args)
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
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
            .spawn()?;
        write_input(&mut child, input)?;
        child.wait().map(|_| ())
    }
}
