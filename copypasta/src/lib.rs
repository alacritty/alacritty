//! A cross-platform clipboard library

// This has to be here due to macro_use
#[cfg(target_os = "macos")]
#[macro_use] extern crate objc;

/// An enumeration describing available clipboard buffers
pub enum Buffer {
    Primary,
    Selection
}

/// Types that can get the system clipboard contents
pub trait Load : Sized {
    /// Errors encountered when working with a clipboard. Each implementation is
    /// allowed to define its own error type, but it must conform to std error.
    type Err: ::std::error::Error + Send + Sync + 'static;

    /// Create a clipboard
    fn new() -> Result<Self, Self::Err>;

    /// Get the primary clipboard contents.
    fn load_primary(&self) -> Result<String, Self::Err>;

    /// Get the clipboard selection contents.
    ///
    /// On most platforms, this doesn't mean anything. A default implementation
    /// is provided which uses the primary clipboard.
    #[inline]
    fn load_selection(&self) -> Result<String, Self::Err> {
        self.load_primary()
    }

    fn load(&self, buffer: Buffer) -> Result<String, Self::Err> {
        match buffer {
            Buffer::Selection => self.load_selection(),
            Buffer::Primary => self.load_primary(),
        }
    }
}

/// Types that can set the system clipboard contents
///
/// Note that some platforms require the clipboard context to stay active in
/// order to load the contents from other applications.
pub trait Store : Load {
    /// Sets the primary clipboard contents
    fn store_primary<S>(&mut self, contents: S) -> Result<(), Self::Err>
        where S: Into<String>;

    /// Sets the secondary clipboard contents
    fn store_selection<S>(&mut self, contents: S) -> Result<(), Self::Err>
        where S: Into<String>;

    /// Store into the specified `buffer`.
    fn store<S>(&mut self, contents: S, buffer: Buffer) -> Result<(), Self::Err>
        where S: Into<String>
    {
        match buffer {
            Buffer::Selection => self.store_selection(contents),
            Buffer::Primary => self.store_primary(contents),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod x11;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub use x11::{Clipboard, Error};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{Clipboard, Error};
