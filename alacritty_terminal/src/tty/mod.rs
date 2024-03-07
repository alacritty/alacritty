//! TTY related functionality.

use std::path::PathBuf;
use std::sync::Arc;
use std::{env, io};

use polling::{Event, PollMode, Poller};

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use self::unix::*;

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows::*;

/// Configuration for the `Pty` interface.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Options {
    /// Shell options.
    ///
    /// [`None`] will use the default shell.
    pub shell: Option<Shell>,

    /// Shell startup directory.
    pub working_directory: Option<PathBuf>,

    /// Remain open after child process exits.
    pub hold: bool,
}

/// Shell options.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Shell {
    /// Path to a shell program to run on startup.
    pub(crate) program: String,
    /// Arguments passed to shell.
    pub(crate) args: Vec<String>,
}

impl Shell {
    pub fn new(program: String, args: Vec<String>) -> Self {
        Self { program, args }
    }
}

/// This trait defines the behaviour needed to read and/or write to a stream.
/// It defines an abstraction over polling's interface in order to allow either one
/// read/write object or a separate read and write object.
pub trait EventedReadWrite {
    type Reader: io::Read;
    type Writer: io::Write;

    /// # Safety
    ///
    /// The underlying sources must outlive their registration in the `Poller`.
    unsafe fn register(&mut self, _: &Arc<Poller>, _: Event, _: PollMode) -> io::Result<()>;
    fn reregister(&mut self, _: &Arc<Poller>, _: Event, _: PollMode) -> io::Result<()>;
    fn deregister(&mut self, _: &Arc<Poller>) -> io::Result<()>;

    fn reader(&mut self) -> &mut Self::Reader;
    fn writer(&mut self) -> &mut Self::Writer;
}

/// Events concerning TTY child processes.
#[derive(Debug, PartialEq, Eq)]
pub enum ChildEvent {
    /// Indicates the child has exited, with an error code if available.
    Exited(Option<i32>),
}

/// A pseudoterminal (or PTY).
///
/// This is a refinement of EventedReadWrite that also provides a channel through which we can be
/// notified if the PTY child process does something we care about (other than writing to the TTY).
/// In particular, this allows for race-free child exit notification on UNIX (cf. `SIGCHLD`).
pub trait EventedPty: EventedReadWrite {
    /// Tries to retrieve an event.
    ///
    /// Returns `Some(event)` on success, or `None` if there are no events to retrieve.
    fn next_child_event(&mut self) -> Option<ChildEvent>;
}

/// Setup environment variables.
pub fn setup_env() {
    // Default to 'alacritty' terminfo if it is available, otherwise
    // default to 'xterm-256color'. May be overridden by user's config
    // below.
    let terminfo = if terminfo_exists("alacritty") { "alacritty" } else { "xterm-256color" };
    env::set_var("TERM", terminfo);

    // Advertise 24-bit color support.
    env::set_var("COLORTERM", "truecolor");

    // Prevent child processes from inheriting startup notification env.
    env::remove_var("DESKTOP_STARTUP_ID");
    env::remove_var("XDG_ACTIVATION_TOKEN");
}

/// Check if a terminfo entry exists on the system.
fn terminfo_exists(terminfo: &str) -> bool {
    // Get first terminfo character for the parent directory.
    let first = terminfo.get(..1).unwrap_or_default();
    let first_hex = format!("{:x}", first.chars().next().unwrap_or_default() as usize);

    // Return true if the terminfo file exists at the specified location.
    macro_rules! check_path {
        ($path:expr) => {
            if $path.join(first).join(terminfo).exists()
                || $path.join(&first_hex).join(terminfo).exists()
            {
                return true;
            }
        };
    }

    if let Some(dir) = env::var_os("TERMINFO") {
        check_path!(PathBuf::from(&dir));
    } else if let Some(home) = home::home_dir() {
        check_path!(home.join(".terminfo"));
    }

    if let Ok(dirs) = env::var("TERMINFO_DIRS") {
        for dir in dirs.split(':') {
            check_path!(PathBuf::from(dir));
        }
    }

    if let Ok(prefix) = env::var("PREFIX") {
        let path = PathBuf::from(prefix);
        check_path!(path.join("etc/terminfo"));
        check_path!(path.join("lib/terminfo"));
        check_path!(path.join("share/terminfo"));
    }

    check_path!(PathBuf::from("/etc/terminfo"));
    check_path!(PathBuf::from("/lib/terminfo"));
    check_path!(PathBuf::from("/usr/share/terminfo"));
    check_path!(PathBuf::from("/boot/system/data/terminfo"));

    // No valid terminfo path has been found.
    false
}
