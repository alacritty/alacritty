#![cfg_attr(feature = "cargo-clippy", deny(clippy, if_not_else, enum_glob_use, wrong_pub_self_convention))]

#[macro_use]
#[cfg(windows)]
extern crate bitflags;
#[cfg(windows)]
extern crate widestring;
#[cfg(windows)]
extern crate winpty_sys;

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::*;
