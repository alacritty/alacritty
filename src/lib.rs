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
#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use, clippy::wrong_pub_self_convention)]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]
#![cfg_attr(all(test, feature = "bench"), feature(test))]

#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

#[macro_use]
pub mod macros;
pub mod ansi;
pub mod cli;
pub mod config;
pub mod display;
pub mod event;
pub mod event_loop;
pub mod grid;
pub mod index;
pub mod input;
pub mod locale;
pub mod logging;
pub mod meter;
pub mod panic;
pub mod renderer;
pub mod selection;
pub mod sync;
pub mod term;
pub mod tty;
pub mod util;
pub mod window;
pub mod message_bar;
mod url;

use std::ops::Mul;

pub use crate::grid::Grid;
pub use crate::term::Term;

const RED: Rgb = Rgb { r: 0xff, g: 0x0, b: 0x0 };
const YELLOW: Rgb = Rgb { r: 0xff, g: 0xff, b: 0x0 };

/// Facade around [winit's `MouseCursor`](glutin::MouseCursor)
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum MouseCursor {
    Arrow,
    Text,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

// a multiply function for Rgb, as the default dim is just *2/3
impl Mul<f32> for Rgb {
    type Output = Rgb;

    fn mul(self, rhs: f32) -> Rgb {
        let result = Rgb {
            r: (f32::from(self.r) * rhs).max(0.0).min(255.0) as u8,
            g: (f32::from(self.g) * rhs).max(0.0).min(255.0) as u8,
            b: (f32::from(self.b) * rhs).max(0.0).min(255.0) as u8
        };

        trace!("Scaling RGB by {} from {:?} to {:?}", rhs, self, result);

        result
    }
}


pub mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}
