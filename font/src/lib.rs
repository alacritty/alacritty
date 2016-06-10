//! Compatibility layer for different font engines
//!
//! This module is developed as part of Alacritty; Alacritty does not include Windows support
//! as a goal at this time, and neither does this module.
//!
//! CoreText is used on Mac OS.
//! FreeType is used on everything that's not Mac OS.

#[cfg(not(target_os = "macos"))]
extern crate fontconfig;
#[cfg(not(target_os = "macos"))]
extern crate freetype;
#[cfg(not(target_os = "macos"))]
extern crate libc;

#[cfg(target_os = "macos")]
extern crate core_text;
#[cfg(target_os = "macos")]
extern crate core_foundation;
#[cfg(target_os = "macos")]
extern crate core_foundation_sys;
#[cfg(target_os = "macos")]
extern crate core_graphics;

extern crate euclid;

use std::fmt;

// If target isn't macos, reexport everything from ft
#[cfg(not(target_os = "macos"))]
mod ft;
#[cfg(not(target_os = "macos"))]
pub use ft::*;

// If target is macos, reexport everything from darwin
#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "macos")]
pub use darwin::*;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FontDesc {
    name: String,
    style: String,
}

impl FontDesc {
    pub fn new<S>(name: S, style: S) -> FontDesc
        where S: Into<String>
    {
        FontDesc {
            name: name.into(),
            style: style.into()
        }
    }
}

pub struct RasterizedGlyph {
    pub c: char,
    pub width: i32,
    pub height: i32,
    pub top: i32,
    pub left: i32,
    pub buf: Vec<u8>,
}

struct BufDebugger<'a>(&'a [u8]);

impl<'a> fmt::Debug for BufDebugger<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("GlyphBuffer")
            .field("len", &self.0.len())
            .field("bytes", &self.0)
            .finish()
    }
}

impl fmt::Debug for RasterizedGlyph {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RasterizedGlyph")
            .field("c", &self.c)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("top", &self.top)
            .field("left", &self.left)
            .field("buf", &BufDebugger(&self.buf[..]))
            .finish()
    }
}

pub struct Metrics {
    pub average_advance: f64,
    pub line_height: f64,
}
