// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//! Alacritty - The GPU Enhanced Terminal
#![feature(question_mark)]
#![feature(range_contains)]
#![feature(inclusive_range_syntax)]
#![feature(drop_types_in_const)]
#![feature(unicode)]
#![feature(step_trait)]
#![cfg_attr(test, feature(test))]
#![feature(core_intrinsics)]
#![allow(stable_features)] // lying about question_mark because 1.14.0 isn't released!

#![feature(proc_macro)]

#[macro_use]
extern crate serde_derive;

extern crate cgmath;
extern crate copypasta;
extern crate errno;
extern crate font;
extern crate glutin;
extern crate libc;
extern crate mio;
extern crate notify;
extern crate parking_lot;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate vte;

#[macro_use]
extern crate bitflags;

#[macro_use]
pub mod macros;

pub mod event;
pub mod event_loop;
pub mod index;
pub mod input;
pub mod meter;
pub mod renderer;
pub mod sync;
pub mod term;
pub mod tty;
pub mod util;
pub mod ansi;
pub mod config;
pub mod grid;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use grid::Grid;
pub use term::Term;

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub mod gl {
    #![allow(non_upper_case_globals)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}

#[derive(Clone)]
pub struct Flag(pub Arc<AtomicBool>);
impl Flag {
    pub fn new(initial_value: bool) -> Flag {
        Flag(Arc::new(AtomicBool::new(initial_value)))
    }

    #[inline]
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    #[inline]
    pub fn set(&self, value: bool) {
        self.0.store(value, Ordering::Release)
    }
}
