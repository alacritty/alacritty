//! Contains traits with platform-specific methods in them.
//!
//! Contains the following modules:
//!
//!  - `android`
//!  - `ios`
//!  - `macos`
//!  - `unix`
//!  - `windows`
//!

pub mod android;
pub mod ios;
pub mod macos;
pub mod unix;
pub mod windows;

/// Platform-specific extensions for OpenGL contexts.
pub trait GlContextExt {
    /// Raw context handle.
    type Handle;

    /// Returns the raw context handle.
    unsafe fn raw_handle(&self) -> Self::Handle;
}
