use clipboard::ClipboardContext;
use clipboard::ClipboardProvider;

use super::{Load, Store};

pub struct Clipboard(ClipboardContext);

#[derive(Debug)]
pub enum Error {
    Clipboard(Box<::std::error::Error>),
}

impl ::std::error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Clipboard(..) => "Error opening clipboard",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::Clipboard(ref err) => err.fmt(f),
        }
    }
}

unsafe impl Send for Error {}
unsafe impl Sync for Error {}

impl Load for Clipboard {
    type Err = Error;

    fn new() -> Result<Self, Error> {
        ClipboardContext::new().map(Clipboard).map_err(Error::Clipboard)
    }

    fn load_primary(&self) -> Result<String, Self::Err> {
        let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
        ctx.get_contents().map_err(Error::Clipboard)
    }

    fn load_selection(&self) -> Result<String, Self::Err> {
        let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
        ctx.get_contents().map_err(Error::Clipboard)
    }
}

impl Store for Clipboard {
    /// Sets the primary clipboard contents
    #[inline]
    fn store_primary<S>(&mut self, contents: S) -> Result<(), Self::Err>
    where
        S: Into<String>,
    {
        self.0.set_contents(contents.into()).map_err(Error::Clipboard)
    }

    /// Sets the secondary clipboard contents
    #[inline]
    fn store_selection<S>(&mut self, contents: S) -> Result<(), Self::Err>
    where
        S: Into<String>,
    {
        self.0.set_contents(contents.into()).map_err(Error::Clipboard)
    }
}
