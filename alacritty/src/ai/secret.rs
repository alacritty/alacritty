//! API-key storage backed by the OS secret store (keyring).
//!
//! The key is never written to the Alacritty config file, logs, the chat transcript, or
//! the PTY child environment. It is fetched on demand and held only in memory for the
//! duration of a request.

use std::io::{self, BufRead, Write};

use keyring::{Entry, Error as KeyringError};

/// Outcome of looking up the API key.
pub enum KeyLookup {
    /// The key was found in the OS keyring.
    Found(String),
    /// No key has been stored for this service/user.
    Missing,
    /// The keyring backend could not be reached (e.g. no Secret Service running).
    Unavailable(String),
}

/// Build a keyring entry for the given service/user.
fn entry(service: &str, user: &str) -> Result<Entry, KeyringError> {
    Entry::new(service, user)
}

/// Retrieve the API key from the OS keyring.
pub fn get_api_key(service: &str, user: &str) -> KeyLookup {
    let entry = match entry(service, user) {
        Ok(entry) => entry,
        Err(err) => return KeyLookup::Unavailable(err.to_string()),
    };

    match entry.get_password() {
        Ok(key) => KeyLookup::Found(key),
        Err(KeyringError::NoEntry) => KeyLookup::Missing,
        Err(err) => KeyLookup::Unavailable(err.to_string()),
    }
}

/// Store the API key in the OS keyring, replacing any existing value.
pub fn set_api_key(service: &str, user: &str, key: &str) -> Result<(), KeyringError> {
    entry(service, user)?.set_password(key)
}

/// Remove the API key from the OS keyring. Succeeds even if no entry exists.
pub fn delete_api_key(service: &str, user: &str) -> Result<(), KeyringError> {
    match entry(service, user)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(err) => Err(err),
    }
}

/// Prompt on stderr and read a secret from stdin without echoing it to the terminal.
///
/// When stdin is not a TTY (e.g. piped input) the value is read directly. The returned
/// string has trailing newline characters stripped.
pub fn prompt_secret(prompt: &str) -> io::Result<String> {
    eprint!("{prompt}");
    io::stderr().flush()?;

    let _guard = EchoGuard::disable();

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;

    // The disabled echo also swallowed the user's newline; emit one so the next output
    // starts on a fresh line.
    if _guard.is_some() {
        eprintln!();
    }

    Ok(line.trim_end_matches(['\n', '\r']).to_owned())
}

/// RAII guard that disables terminal echo on stdin and restores it on drop.
#[cfg(unix)]
struct EchoGuard {
    fd: std::os::unix::io::RawFd,
    original: libc::termios,
}

#[cfg(unix)]
impl EchoGuard {
    /// Disable echo on stdin, returning a guard if stdin is a TTY.
    fn disable() -> Option<Self> {
        use std::os::unix::io::AsRawFd;

        let fd = io::stdin().as_raw_fd();
        // SAFETY: `termios` is fully initialized by `tcgetattr` before use.
        unsafe {
            let mut termios: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut termios) != 0 {
                // Not a TTY (e.g. piped input); nothing to restore.
                return None;
            }
            let original = termios;
            termios.c_lflag &= !libc::ECHO;
            if libc::tcsetattr(fd, libc::TCSANOW, &termios) != 0 {
                return None;
            }
            Some(Self { fd, original })
        }
    }
}

#[cfg(unix)]
impl Drop for EchoGuard {
    fn drop(&mut self) {
        // SAFETY: restoring the previously captured, valid termios state.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.original);
        }
    }
}

/// No-op echo guard on platforms without a termios implementation here.
#[cfg(not(unix))]
struct EchoGuard;

#[cfg(not(unix))]
impl EchoGuard {
    fn disable() -> Option<Self> {
        None
    }
}
