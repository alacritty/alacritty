//! X11 Clipboard implementation
//!
//! Note that the x11 implementation is really crap right now - we just depend
//! on xclip being on the user's path. If x11 pasting doesn't work, it's
//! probably because xclip is unavailable. There's currently no non-GPL x11
//! clipboard library for Rust. Until then, we have this hack.
//!
//! FIXME: Implement actual X11 clipboard API using the ICCCM reference
//!        https://tronche.com/gui/x/icccm/
use std::io;
use std::process::{Output, Command};
use std::string::FromUtf8Error;
use std::ffi::OsStr;

use super::{Load, Store};

/// The x11 clipboard
pub struct Clipboard;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Xclip(String),
    Utf8(FromUtf8Error),
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::Io(ref err) => Some(err),
            Error::Utf8(ref err) => Some(err),
            _ => None,
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Io(..) => "error calling xclip",
            Error::Xclip(..) => "error reported by xclip",
            Error::Utf8(..) => "clipboard contents not utf8",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::Io(ref err) => {
                match err.kind() {
                    io::ErrorKind::NotFound => {
                        write!(f, "Please install `xclip` to enable clipboard support")
                    },
                    _ => write!(f, "error calling xclip: {}", err),
                }
            },
            Error::Xclip(ref s) => write!(f, "error from xclip: {}", s),
            Error::Utf8(ref err) => write!(f, "error parsing xclip output: {}", err),
        }
    }
}

impl From<io::Error> for Error {
    fn from(val: io::Error) -> Error {
        Error::Io(val)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(val: FromUtf8Error) -> Error {
        Error::Utf8(val)
    }
}

impl Load for Clipboard {
    type Err = Error;

    fn new() -> Result<Self, Error> {
        Ok(Clipboard)
    }

    fn load_primary(&self) -> Result<String, Self::Err> {
        let output = Command::new("xclip")
            .args(&["-o", "-selection", "clipboard"])
            .output()?;

        Clipboard::process_xclip_output(output)
    }

    fn load_selection(&self) -> Result<String, Self::Err> {
        let output = Command::new("xclip")
            .args(&["-o"])
            .output()?;

        Clipboard::process_xclip_output(output)
    }
}

impl Store for Clipboard {
    /// Sets the primary clipboard contents
    #[inline]
    fn store_primary<S>(&mut self, contents: S) -> Result<(), Self::Err>
        where S: Into<String>
    {
        self.store(contents, &["-i", "-selection", "clipboard"])
    }

    /// Sets the secondary clipboard contents
    #[inline]
    fn store_selection<S>(&mut self, contents: S) -> Result<(), Self::Err>
        where S: Into<String>
    {
        self.store(contents, &["-i"])
    }
}

impl Clipboard {
    fn process_xclip_output(output: Output) -> Result<String, Error> {
        if output.status.success() {
            String::from_utf8(output.stdout)
                .map_err(::std::convert::From::from)
        } else {
            String::from_utf8(output.stderr)
                .map_err(::std::convert::From::from)
        }
    }

    fn store<C, S>(&mut self, contents: C, args: &[S]) -> Result<(), Error>
        where C: Into<String>,
              S: AsRef<OsStr>,
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let contents = contents.into();
        let mut child = Command::new("xclip")
            .args(args)
            .stdin(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.as_mut() {
            stdin.write_all(contents.as_bytes())?;
        }

        // Return error if didn't exit cleanly
        let exit_status = child.wait()?;
        if exit_status.success() {
            Ok(())
        } else {
            Err(Error::Xclip("xclip returned non-zero exit code".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Clipboard;
    use ::{Load, Store};

    #[test]
    fn clipboard_works() {
        let mut clipboard = Clipboard::new().expect("create clipboard");
        let arst = "arst";
        let oien = "oien";
        clipboard.store_primary(arst).expect("store selection");
        clipboard.store_selection(oien).expect("store selection");

        let selection = clipboard.load_selection().expect("load selection");
        let primary = clipboard.load_primary().expect("load selection");

        assert_eq!(arst, primary);
        assert_eq!(oien, selection);
    }
}
