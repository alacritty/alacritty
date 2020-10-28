//! Alacritty - The GPU Enhanced Terminal.

#![warn(rust_2018_idioms, future_incompatible)]
#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use, clippy::wrong_pub_self_convention)]
#![cfg_attr(feature = "cargo-clippy", deny(warnings))]
#![cfg_attr(all(test, feature = "bench"), feature(test))]

pub mod ansi;
pub mod config;
pub mod event;
pub mod event_loop;
pub mod grid;
pub mod index;
pub mod selection;
pub mod sync;
pub mod term;
pub mod thread;
pub mod tty;
pub mod vi_mode;

pub use crate::grid::Grid;
pub use crate::term::Term;
