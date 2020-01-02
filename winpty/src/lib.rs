#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use, clippy::wrong_pub_self_convention)]

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use crate::windows::*;
