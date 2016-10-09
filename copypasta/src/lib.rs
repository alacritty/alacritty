//! A cross-platform clipboard library

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
}

/// Types that can set the system clipboard contents
///
/// Note that some platforms require the clipboard context to stay active in
/// order to load the contents from other applications.
pub trait Store : Load {
    /// Sets the primary clipboard contents
    fn store_primary(&mut self, contents: String) -> Result<(), Self::Err>;

    /// Sets the secondary clipboard contents
    fn store_selection(&mut self, contents: String) -> Result<(), Self::Err>;
}

#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub use x11::{Clipboard, Error};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{Clipboard, Error};

