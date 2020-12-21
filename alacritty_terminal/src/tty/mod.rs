//! TTY related functionality.

use std::{env, io};

use crate::config::Config;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use self::unix::*;

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows::*;

/// This trait defines the behaviour needed to read and/or write to a stream.
/// It defines an abstraction over mio's interface in order to allow either one
/// read/write object or a separate read and write object.
pub trait EventedReadWrite {
    type Reader: io::Read;
    type Writer: io::Write;

    fn register(
        &mut self,
        _: &mio::Poll,
        _: &mut dyn Iterator<Item = mio::Token>,
        _: mio::Ready,
        _: mio::PollOpt,
    ) -> io::Result<()>;
    fn reregister(&mut self, _: &mio::Poll, _: mio::Ready, _: mio::PollOpt) -> io::Result<()>;
    fn deregister(&mut self, _: &mio::Poll) -> io::Result<()>;

    fn reader(&mut self) -> &mut Self::Reader;
    fn read_token(&self) -> mio::Token;
    fn writer(&mut self) -> &mut Self::Writer;
    fn write_token(&self) -> mio::Token;
}

/// Events concerning TTY child processes.
#[derive(Debug, PartialEq)]
pub enum ChildEvent {
    /// Indicates the child has exited.
    Exited,
}

/// A pseudoterminal (or PTY).
///
/// This is a refinement of EventedReadWrite that also provides a channel through which we can be
/// notified if the PTY child process does something we care about (other than writing to the TTY).
/// In particular, this allows for race-free child exit notification on UNIX (cf. `SIGCHLD`).
pub trait EventedPty: EventedReadWrite {
    fn child_event_token(&self) -> mio::Token;

    /// Tries to retrieve an event.
    ///
    /// Returns `Some(event)` on success, or `None` if there are no events to retrieve.
    fn next_child_event(&mut self) -> Option<ChildEvent>;
}

/// Setup environment variables.
pub fn setup_env<C>(config: &Config<C>) {
    // Default to 'alacritty' terminfo if it is available, otherwise
    // default to 'xterm-256color'. May be overridden by user's config
    // below.
    env::set_var(
        "TERM",
        if is_alacritty_terminfo_available() { "alacritty" } else { "xterm-256color" },
    );

    // Advertise 24-bit color support.
    env::set_var("COLORTERM", "truecolor");

    // Prevent child processes from inheriting startup notification env.
    env::remove_var("DESKTOP_STARTUP_ID");

    // Set env vars from config.
    for (key, value) in config.env.iter() {
        env::set_var(key, value);
    }
}

fn is_alacritty_terminfo_available() -> bool {
    use std::fs;
    use std::path::{Path, PathBuf};

    let name = "alacritty";
    let first = 'a';
    let mut search: Vec<PathBuf> = Vec::new();
    if let Some(dir) = env::var_os("TERMINFO") {
        search.push(dir.into());
    } else if let Some(mut home) = dirs::home_dir() {
        home.push(".terminfo");
        search.push(home);
    }
    if let Ok(dirs) = env::var("TERMINFO_DIRS") {
        for dir in dirs.split(':') {
            search.push(dir.into());
        }
    }
    if let Ok(prefix) = env::var("PREFIX") {
        let path = Path::new(&prefix);
        search.push(path.join("etc/terminfo"));
        search.push(path.join("lib/terminfo"));
        search.push(path.join("share/terminfo"));
    }
    search.push("/etc/terminfo".into());
    search.push("/lib/terminfo".into());
    search.push("/usr/share/terminfo".into());
    search.push("/boot/system/data/terminfo".into());
    for path in search {
        if fs::metadata(&path).is_ok() {
            {
                let mut path = path.clone();
                path.push(first.to_string());
                path.push(name);

                if fs::metadata(&path).is_ok() {
                    return true;
                }
            }

            {
                let mut path = path.clone();
                path.push(format!("{:x}", first as usize));
                path.push(name);

                if fs::metadata(&path).is_ok() {
                    return true;
                }
            }
        }
    }
    false
}
