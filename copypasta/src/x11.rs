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

use super::Load;

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
            Error::Io(ref err) => write!(f, "error calling xclip: {}", err),
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
        let output = try!(Command::new("xclip")
            .args(&["-o", "-selection", "clipboard"])
            .output());

        Clipboard::process_xclip_output(output)
    }

    fn load_selection(&self) -> Result<String, Self::Err> {
        let output = try!(Command::new("xclip")
            .args(&["-o"])
            .output());

        Clipboard::process_xclip_output(output)
    }
}

impl Clipboard {
    fn process_xclip_output(output: Output) -> Result<String, Error> {
        if output.status.success() {
            Ok(try!(String::from_utf8(output.stdout)))
        } else {
            Ok(try!(String::from_utf8(output.stderr)))
        }
    }
}
