//! Alacritty - The GPU Enhanced Terminal.

#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use, clippy::wrong_pub_self_convention)]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]
#![cfg_attr(all(test, feature = "bench"), feature(test))]

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

pub mod ansi;
pub mod config;
pub mod event;
pub mod event_loop;
pub mod grid;
pub mod index;
#[cfg(target_os = "macos")]
pub mod locale;
pub mod message_bar;
pub mod meter;
pub mod panic;
pub mod selection;
pub mod sync;
pub mod term;
pub mod tty;
pub mod util;
pub mod vi_mode;

pub use crate::grid::Grid;
pub use crate::term::Term;
