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

#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use, clippy::wrong_pub_self_convention)]

#[cfg(not(any(target_os = "macos", windows)))]
extern crate fontconfig;
#[cfg(not(any(target_os = "macos", windows)))]
extern crate freetype;

#[cfg(target_os = "macos")]
extern crate core_foundation;
#[cfg(target_os = "macos")]
extern crate core_foundation_sys;
#[cfg(target_os = "macos")]
extern crate core_graphics;
#[cfg(target_os = "macos")]
extern crate core_text;
#[cfg(target_os = "macos")]
extern crate euclid;

extern crate libc;

#[cfg(not(any(target_os = "macos", windows)))]
#[macro_use]
extern crate foreign_types;

#[cfg_attr(not(windows), macro_use)]
extern crate log;

use std::hash::{Hash, Hasher};
use std::{fmt, cmp};
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

// If target isn't macos or windows, reexport everything from ft
#[cfg(not(any(target_os = "macos", windows)))]
pub mod ft;
#[cfg(not(any(target_os = "macos", windows)))]
pub use ft::{Error, FreeTypeRasterizer as Rasterizer};

#[cfg(windows)]
pub mod rusttype;
#[cfg(windows)]
pub use crate::rusttype::{Error, RustTypeRasterizer as Rasterizer};

// If target is macos, reexport everything from darwin
#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "macos")]
pub use darwin::*;

/// Width/Height of the cursor relative to the font width
pub const CURSOR_WIDTH_PERCENTAGE: i32 = 15;

/// Character used for the underline cursor
// This is part of the private use area and should not conflict with any font
pub const UNDERLINE_CURSOR_CHAR: char = '\u{10a3e2}';

/// Character used for the beam cursor
// This is part of the private use area and should not conflict with any font
pub const BEAM_CURSOR_CHAR: char = '\u{10a3e3}';

/// Character used for the empty box cursor
// This is part of the private use area and should not conflict with any font
pub const BOX_CURSOR_CHAR: char = '\u{10a3e4}';

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
    Bold,
}

/// Style of font
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Style {
    Specific(String),
    Description { slant: Slant, weight: Weight },
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Style::Specific(ref s) => f.write_str(&s),
            Style::Description { slant, weight } => {
                write!(f, "slant={:?}, weight={:?}", slant, weight)
            }
        }
    }
}

impl FontDesc {
    pub fn new<S>(name: S, style: Style) -> FontDesc
    where
        S: Into<String>,
    {
        FontDesc {
            name: name.into(),
            style,
        }
    }
}

impl fmt::Display for FontDesc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "name {} and style {}", self.name, self.style)
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

#[derive(Debug, Copy, Clone, Eq)]
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

impl PartialEq for GlyphKey {
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            // This transmute is fine:
            //
            // - If GlyphKey ever becomes a different size, this will fail to compile
            // - Result is being used for equality checking and has no fields (it's a u64)
            let other = ::std::mem::transmute::<GlyphKey, u64>(*other);
            ::std::mem::transmute::<GlyphKey, u64>(*self).eq(&other)
        }
    }
}

/// Font size stored as integer
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
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
        f32::from(self.0) / Size::factor()
    }
}

impl ::std::ops::Add for Size {
    type Output = Size;

    fn add(self, other: Size) -> Size {
        Size(self.0.saturating_add(other.0))
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

// Returns a custom underline cursor character
pub fn get_underline_cursor_glyph(descent: i32, width: i32) -> Result<RasterizedGlyph, Error> {
    // Create a new rectangle, the height is relative to the font width
    let height = cmp::max(width * CURSOR_WIDTH_PERCENTAGE / 100, 1);
    let buf = vec![255u8; (width * height * 3) as usize];

    // Create a custom glyph with the rectangle data attached to it
    Ok(RasterizedGlyph {
        c: UNDERLINE_CURSOR_CHAR,
        top: descent + height,
        left: 0,
        height,
        width,
        buf,
    })
}

// Returns a custom beam cursor character
pub fn get_beam_cursor_glyph(
    ascent: i32,
    height: i32,
    width: i32,
) -> Result<RasterizedGlyph, Error> {
    // Create a new rectangle that is at least one pixel wide
    let beam_width = cmp::max(width * CURSOR_WIDTH_PERCENTAGE / 100, 1);
    let buf = vec![255u8; (beam_width * height * 3) as usize];

    // Create a custom glyph with the rectangle data attached to it
    Ok(RasterizedGlyph {
        c: BEAM_CURSOR_CHAR,
        top: ascent,
        left: 0,
        height,
        width: beam_width,
        buf,
    })
}

// Returns a custom box cursor character
pub fn get_box_cursor_glyph(
    ascent: i32,
    height: i32,
    width: i32,
) -> Result<RasterizedGlyph, Error> {
    // Create a new box outline rectangle
    let border_width = cmp::max(width * CURSOR_WIDTH_PERCENTAGE / 100, 1);
    let mut buf = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            if y < border_width || y >= height - border_width ||
               x < border_width || x >= width - border_width {
                buf.append(&mut vec![255u8; 3]);
            } else {
                buf.append(&mut vec![0u8; 3]);
            }
        }
    }

    // Create a custom glyph with the rectangle data attached to it
    Ok(RasterizedGlyph {
        c: BOX_CURSOR_CHAR,
        top: ascent,
        left: 0,
        height,
        width,
        buf,
    })
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
    pub descent: f32,
    pub underline_position: f32,
    pub underline_thickness: f32,
    pub strikeout_position: f32,
    pub strikeout_thickness: f32,
}

pub trait Rasterize {
    /// Errors occurring in Rasterize methods
    type Err: ::std::error::Error + Send + Sync + 'static;

    /// Create a new Rasterizer
    fn new(device_pixel_ratio: f32, use_thin_strokes: bool) -> Result<Self, Self::Err>
    where
        Self: Sized;

    /// Get `Metrics` for the given `FontKey`
    fn metrics(&self, _: FontKey, _: Size) -> Result<Metrics, Self::Err>;

    /// Load the font described by `FontDesc` and `Size`
    fn load_font(&mut self, _: &FontDesc, _: Size) -> Result<FontKey, Self::Err>;

    /// Rasterize the glyph described by `GlyphKey`.
    fn get_glyph(&mut self, _: GlyphKey) -> Result<RasterizedGlyph, Self::Err>;

    /// Update the Rasterizer's DPI factor
    fn update_dpr(&mut self, device_pixel_ratio: f32);
}
