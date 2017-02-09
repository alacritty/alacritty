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
//! Compatibility layer for different font engines
//!
//! CoreText is used on Mac OS.
//! FreeType is used on everything that's not Mac OS.
//! Eventually, ClearType support will be available for windows
#[cfg(not(target_os = "macos"))]
extern crate fontconfig;
#[cfg(not(target_os = "macos"))]
extern crate freetype;

#[cfg(target_os = "macos")]
extern crate core_text;
#[cfg(target_os = "macos")]
extern crate core_foundation;
#[cfg(target_os = "macos")]
extern crate core_foundation_sys;
#[cfg(target_os = "macos")]
extern crate core_graphics;

extern crate euclid;
extern crate libc;

#[macro_use]
extern crate ffi_util;

#[macro_use]
extern crate log;

use std::hash::{Hash, Hasher};
use std::fmt;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

// If target isn't macos, reexport everything from ft
#[cfg(not(target_os = "macos"))]
mod ft;
#[cfg(not(target_os = "macos"))]
pub use ft::{FreeTypeRasterizer as Rasterizer, Error};

// If target is macos, reexport everything from darwin
#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "macos")]
pub use darwin::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontDesc {
    name: String,
    style: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slant {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    Normal,
    Bold
}

/// Style of font
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Style {
    Specific(String),
    Description { slant: Slant, weight: Weight }
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Style::Specific(ref s) => f.write_str(&s),
            Style::Description { slant, weight } => {
                write!(f, "slant={:?}, weight={:?}", slant, weight)
            },
        }
    }
}

impl FontDesc {
    pub fn new<S>(name: S, style: Style) -> FontDesc
        where S: Into<String>
    {
        FontDesc {
            name: name.into(),
            style: style
        }
    }
}

impl fmt::Display for FontDesc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "name '{}' and style '{}'", self.name, self.style)
    }
}

/// Identifier for a Font for use in maps/etc
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct FontKey {
    token: u16,
}

impl FontKey {
    /// Get next font key for given size
    ///
    /// The generated key will be globally unique
    pub fn next() -> FontKey {
        static TOKEN: AtomicUsize = ATOMIC_USIZE_INIT;

        FontKey {
            token: TOKEN.fetch_add(1, Ordering::SeqCst) as _,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GlyphKey {
    pub c: char,
    pub font_key: FontKey,
    pub size: Size,
}

impl Hash for GlyphKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe {
            // This transmute is fine:
            //
            // - If GlyphKey ever becomes a different size, this will fail to compile
            // - Result is being used for hashing and has no fields (it's a u64)
            ::std::mem::transmute::<GlyphKey, u64>(*self)
        }.hash(state);
    }
}

/// Font size stored as integer
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct Size(i16);

impl Size {
    /// Scale factor between font "Size" type and point size
    #[inline]
    pub fn factor() -> f32 {
        2.0
    }

    /// Create a new `Size` from a f32 size in points
    pub fn new(size: f32) -> Size {
        Size((size * Size::factor()) as i16)
    }

    /// Get the f32 size in points
    pub fn as_f32_pts(self) -> f32 {
        self.0 as f32 / Size::factor()
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

impl Default for RasterizedGlyph {
    fn default() -> RasterizedGlyph {
        RasterizedGlyph {
            c: ' ',
            width: 0,
            height: 0,
            top: 0,
            left: 0,
            buf: Vec::new(),
        }
    }
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

pub trait Rasterize {
    /// Errors occurring in Rasterize methods
    type Err: ::std::error::Error + Send + Sync + 'static;

    /// Create a new Rasterize
    fn new(dpi_x: f32, dpi_y: f32, device_pixel_ratio: f32, use_thin_strokes: bool) -> Result<Self, Self::Err>
        where Self: Sized;

    /// Get `Metrics` for the given `FontKey` and `Size`
    fn metrics(&self, FontKey, Size) -> Result<Metrics, Self::Err>;

    /// Load the font described by `FontDesc` and `Size`
    fn load_font(&mut self, &FontDesc, Size) -> Result<FontKey, Self::Err>;

    /// Rasterize the glyph described by `GlyphKey`.
    fn get_glyph(&mut self, &GlyphKey) -> Result<RasterizedGlyph, Self::Err>;
}
