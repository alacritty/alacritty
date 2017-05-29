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

#[cfg(not(target_os = "macos"))]
#[macro_use]
extern crate ffi_util;

#[macro_use]
extern crate log;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::fmt;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

// If target isn't macos, reexport everything from ft
#[cfg(not(target_os = "macos"))]
mod ft;
#[cfg(not(target_os = "macos"))]
pub use ft::{Font, RasterizerImpl, Error};

// If target is macos, reexport everything from darwin
#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "macos")]
pub use darwin::{Font, RasterizerImpl, Error};

#[derive(Debug, Clone)]
pub struct FontList {
    fonts: Vec<Font>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontDesc {
    name: String,
    style: Style,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontDescList {
    pub descs: Vec<FontDesc>,
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
    pub descent: f32,
}

//
// We support two types of font fallbacks depending on platform.
// 1. For macOS, we load the user provided fallback lists (from alacritty.yml)
//    and add on what we get from get_fallback_fonts(), which are lists
//    provided by the system.
// 2. For freetype/fontconfig, we try the user fallbacks, and failing that
//    fall back on fontconfig where an appropriate font is found using get_glyph_fallback()
//    depending on glyph.
//
pub trait RasterizeImpl {
    /// Errors occurring in Rasterize methods
    type Err: ::std::error::Error + Send + Sync + 'static;

    /// Create a new RasterizeImpl
    fn new() -> Result<Self,Self::Err>
        where Self: Sized;

    /// Load the fonts described by `FontDesc` and `Size`, this doesn't
    /// necessarily mean the implementation must load the font
    /// before calls to metrics/get_glyph.
    fn load_font(
        &self,
        &Details,
        &FontDesc,
        Size,
    ) -> Result<Font, Self::Err>;

    /// Get the fallback list of `Font` for given `FontDesc` `Font` and `Size`.
    /// This is used on macOS.
    fn get_fallback_fonts(
        &self,
        &Details,
        &FontDesc,
        &Font,
        Size,
        &Vec<String>,
    ) -> Result<Vec<Font>, Self::Err>;

    /// Get `Metrics` for the given `Font`
    fn metrics(
        &self,
        &Details,
        &Font,
    ) -> Result<Metrics, Self::Err>;

    /// Rasterize the glyph described by `char` `Font` and `Size`.
    fn get_glyph(
        &self,
        &Details,
        char,
        &[u16], // utf16 encoded char for macOS
        &Font,
        Size,
    ) -> Result<RasterizedGlyph, Self::Err>;

    /// Try to rasterize the glyph described by `char` `Font` and `Size` using
    /// fallback mechanisms. This is relevant for fontconfig.
    fn get_glyph_fallback(
        &mut self,
        &Details,
        char,
        &[u16], // utf16 encoded char for macOS
        Size,
    ) -> Result<RasterizedGlyph, Self::Err>;
}

pub struct Details {
    pub dpi_x: f32,
    pub dpi_y: f32,
    pub device_pixel_ratio: f32,
    pub use_thin_strokes: bool,
}

/// Exposed Rasterizer delegating to actual RasterizeImpl depending on platform
pub struct Rasterizer {
    fonts: HashMap<FontKey, FontList>,
    keys: HashMap<(FontDescList, Size), FontKey>,
    r_impl: RasterizerImpl,
    details: Details,
}

impl Rasterizer {

    pub fn new(
        dpi_x: f32,
        dpi_y: f32,
        device_pixel_ratio: f32,
        use_thin_strokes: bool,
    ) -> Result<Self,Error> {
        Ok(Rasterizer {
            fonts: HashMap::new(),
            keys: HashMap::new(),
            r_impl: RasterizerImpl::new()?,
            details: Details {dpi_x, dpi_y, device_pixel_ratio, use_thin_strokes},
        })
    }

    /// Declare the font list described by `FontDescList` and `Size` to get
    /// the corresponding `FontKey`
    pub fn declare_font_list(
        &mut self,
        lists: &FontDescList,
        size: Size,
    ) -> Result<FontKey, Error> {

        self.keys
            .get(&(lists.clone(), size))
            .map(|k| Ok(*k))
            .unwrap_or_else(|| {

                // these are the fonts configured by the user in alacritty.yml
                let user_fonts = lists.descs.clone().into_iter()
                    .filter_map(|x| {
                        // delegate font loading to platform implementation
                        match self.r_impl.load_font(&self.details, &x, size) {
                            Ok(font) => Some(Ok((x, font))),

                            // fonts that fail to load for some reason
                            Err(e) =>
                                if let Error::MissingFont(ref d) = e {
                                    // missing is not an error we abort on,
                                    // we log it and then drop the entry.
                                    info!("Failed to load font {}", d);
                                    None
                                } else {
                                    // any other error results in aborting
                                    // the iterator.
                                    Some(Err(e))
                                }
                        }
                    })
                    .collect::<Result<Vec<(FontDesc,Font)>, Error>>()?;

                // we don't accept loading no fonts at all.
                if user_fonts.len() == 0 {
                    return Err(Error::NoFontsForList)
                }

                // get system fallback fonts to add on to the user configured ones.
                let mut fallback_fonts = {
                    // names of fonts that were already loaded by the user config.
                    let loaded_names:Vec<String> = user_fonts.iter()
                        .map(|x| x.0.name.to_owned()).collect();
                    // we consider the first font to be the "primary" to
                    // base system fallback fonts on.
                    let ref first = user_fonts[0].clone();

                    // delegate getting fallback to platform implementation.
                    self.r_impl.get_fallback_fonts(
                        &self.details, &first.0, &first.1, size, &loaded_names)?
                };

                // build final list of fonts adding user_fonts + fallback_fonts.
                let mut fonts:Vec<Font> = user_fonts.into_iter().map(|x| x.1).collect();
                fonts.append(&mut fallback_fonts);

                let key = FontKey::next();

                self.fonts.insert(key, FontList {fonts});
                self.keys.insert((lists.clone(), size), key);

                Ok(key)
            })

    }

    /// Get `Metrics` for the given `FontKey`
    pub fn metrics(
        &self,
        key: &FontKey,
    ) -> Result<Metrics, Error> {

        let list = self.fonts
            .get(&key)
            .ok_or(Error::FontNotLoaded)?;

        // we are guaranteed to always have at least one
        // font in the list.
        let ref font = list.fonts[0];

        self.r_impl.metrics(&self.details, font)

    }

    /// Rasterize the glyph described by `GlyphKey`.
    pub fn get_glyph(
        &mut self,
        glyph: &GlyphKey,
    ) -> Result<RasterizedGlyph, Error> {

        let list = self.fonts
            .get(&glyph.font_key)
            .ok_or(Error::FontNotLoaded)?;

        // to avoid utf16 encoding the char for every fallback
        // lookup, we encoded it once. this is required for macOS.
        let mut buf = [0; 2];
        let encoded:&[u16] = glyph.c.encode_utf16(&mut buf);

        for font in &list.fonts {
            match self.r_impl.get_glyph(
                &self.details, glyph.c, encoded, font, glyph.size) {
                // early return, first glyph that renders
                Ok(glyph) => return Ok(glyph),

                Err(error) => {
                    match error {
                        // ignore the missing glyph and continue to try the next
                        // font in the font list.
                        Error::MissingGlyph(_) => continue,
                        // any other error aborts the attempt
                        _ => return Err(error)
                    }
                }

            }
        }

        // before giving up we try doing a fallback font.
        // for macOS this isn't used, since the fallback fonts are padded
        // onto the list.fonts already at startup and are thus handled
        // by the above for-loop.
        self.r_impl.get_glyph_fallback(&self.details, glyph.c, encoded, glyph.size)
    }

}
