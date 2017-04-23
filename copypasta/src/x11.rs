//! X11 Clipboard implementation
//!
//! Note that the x11 implementation is really crap right now - we just depend
//! on xclip being on the user's path. If x11 pasting doesn't work, it's
//! probably because xclip is unavailable. There's currently no non-GPL x11
//! clipboard library for Rust. Until then, we have this hack.
//!
//! FIXME: Implement actual X11 clipboard API using the ICCCM reference
//!        https://tronche.com/gui/x/icccm/
use std::string::FromUtf8Error;
use std::time::Duration;
use x11_clipboard::Clipboard as X11Clipboard;
use super::{Load, Store};

/// The x11 clipboard
pub struct Clipboard(X11Clipboard);

#[derive(Debug)]
pub enum Error {
    Clipboard(String),
    Utf8(FromUtf8Error),
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::Utf8(ref err) => Some(err),
            _ => None
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Clipboard(..) => "clipboard error",
            Error::Utf8(..) => "clipboard contents not utf8",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::Clipboard(ref err) => write!(f, "clipboard error: {}", err),
            Error::Utf8(ref err) => write!(f, "error parsing xclip output: {}", err),
        }
    }
}

impl From<FromUtf8Error> for Error {
    fn from(val: FromUtf8Error) -> Error {
        Error::Utf8(val)
    }
}

impl From<::x11_clipboard::error::Error> for Error {
    fn from(val: ::x11_clipboard::error::Error) -> Error {
        Error::Clipboard(format!("{}", val))
    }
}

impl Load for Clipboard {
    type Err = Error;

    fn new() -> Result<Self, Error> {
        X11Clipboard::new()
            .map(Clipboard)
            .map_err(Into::into)
    }

    fn load_primary(&self) -> Result<String, Self::Err> {
        let atom_clipboard = self.0.getter.atoms.clipboard;
        let atom_utf8string = self.0.getter.atoms.utf8_string;
        let atom_property = self.0.getter.atoms.property;

        self.0.load(atom_clipboard, atom_utf8string, atom_property, Duration::from_secs(3))
            .map_err(Into::into)
            .and_then(|vec| String::from_utf8(vec).map_err(Into::into))
    }

    fn load_selection(&self) -> Result<String, Self::Err> {
        let atom_primary = self.0.getter.atoms.primary;
        let atom_utf8string = self.0.getter.atoms.utf8_string;
        let atom_property = self.0.getter.atoms.property;

        self.0.load(atom_primary, atom_utf8string, atom_property, Duration::from_secs(3))
            .map_err(Into::into)
            .and_then(|vec| String::from_utf8(vec).map_err(Into::into))
    }
}

impl Store for Clipboard {
    /// Sets the primary clipboard contents
    #[inline]
    fn store_primary<S>(&mut self, contents: S) -> Result<(), Self::Err>
        where S: Into<String>
    {

        let atom_clipboard = self.0.setter.atoms.clipboard;
        let atom_utf8string = self.0.setter.atoms.utf8_string;

        self.0.store(atom_clipboard, atom_utf8string, contents.into())
            .map_err(Into::into)
    }

    /// Sets the secondary clipboard contents
    #[inline]
    fn store_selection<S>(&mut self, contents: S) -> Result<(), Self::Err>
        where S: Into<String>
    {
        let atom_primary = self.0.setter.atoms.primary;
        let atom_utf8string = self.0.setter.atoms.utf8_string;

        self.0.store(atom_primary, atom_utf8string, contents.into())
            .map_err(Into::into)
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
