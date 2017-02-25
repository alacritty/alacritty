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
//! Rasterization powered by FreeType and FontConfig
use std::collections::HashMap;

use freetype::{self, Library, Face};


mod list_fonts;

use self::list_fonts::fc;
use super::{FontDesc, RasterizedGlyph, Metrics, Size, FontKey, GlyphKey, Weight, Slant, Style};

/// Rasterizes glyphs for a single font face.
pub struct FreeTypeRasterizer {
    faces: HashMap<FontKey, Face<'static>>,
    library: Library,
    keys: HashMap<FontDesc, FontKey>,
    dpi_x: u32,
    dpi_y: u32,
    dpr: f32,
}

#[inline]
fn to_freetype_26_6(f: f32) -> isize {
    ((1i32 << 6) as f32 * f) as isize
}

impl ::Rasterize for FreeTypeRasterizer {
    type Err = Error;

    fn new(dpi_x: f32, dpi_y: f32, device_pixel_ratio: f32, _: bool) -> Result<FreeTypeRasterizer, Error> {
        let library = Library::init()?;

        Ok(FreeTypeRasterizer {
            faces: HashMap::new(),
            keys: HashMap::new(),
            library: library,
            dpi_x: dpi_x as u32,
            dpi_y: dpi_y as u32,
            dpr: device_pixel_ratio,
        })
    }

    fn metrics(&self, key: FontKey, size: Size) -> Result<Metrics, Error> {
        let face = self.faces
            .get(&key)
            .ok_or(Error::FontNotLoaded)?;

        let scale_size = self.dpr as f64 * size.as_f32_pts() as f64;

        let em_size = face.em_size() as f64;
        let w = face.max_advance_width() as f64;
        let h = (face.ascender() - face.descender() + face.height()) as f64;

        let w_scale = w * scale_size / em_size;
        let h_scale = h * scale_size / em_size;

        Ok(Metrics {
            average_advance: w_scale,
            line_height: h_scale,
        })
    }

    fn load_font(&mut self, desc: &FontDesc, _size: Size) -> Result<FontKey, Error> {
        self.keys
            .get(&desc.to_owned())
            .map(|k| Ok(*k))
            .unwrap_or_else(|| {
                let face = self.get_face(desc)?;
                let key = FontKey::next();
                self.faces.insert(key, face);
                Ok(key)
            })
    }

    fn get_glyph(&mut self, glyph_key: &GlyphKey) -> Result<RasterizedGlyph, Error> {
        let face = self.faces
            .get(&glyph_key.font_key)
            .ok_or(Error::FontNotLoaded)?;

        let size = glyph_key.size.as_f32_pts() * self.dpr;
        let c = glyph_key.c;

        face.set_char_size(to_freetype_26_6(size), 0, self.dpi_x, self.dpi_y)?;
        let index = face.get_char_index(c as usize);

        // Test fallback case
        if index == 0 {
            self.load_font(
                &FontDesc::new("fallback", Style::ContainsGlyph(glyph_key.c)),
                glyph_key.size
            );
            return self.get_glyph(&glyph_key);
        }

        face.load_glyph(index as u32, freetype::face::TARGET_LIGHT)?;
        let glyph = face.glyph();
        glyph.render_glyph(freetype::render_mode::RenderMode::Lcd)?;

        unsafe {
            let ft_lib = self.library.raw();
            freetype::ffi::FT_Library_SetLcdFilter(
                ft_lib,
                freetype::ffi::FT_LCD_FILTER_DEFAULT
            );
        }

        let bitmap = glyph.bitmap();
        let buf = bitmap.buffer();
        let pitch = bitmap.pitch() as usize;

        let mut packed = Vec::with_capacity((bitmap.rows() * bitmap.width()) as usize);
        for i in 0..bitmap.rows() {
            let start = (i as usize) * pitch;
            let stop = start + bitmap.width() as usize;
            packed.extend_from_slice(&buf[start..stop]);
        }

        Ok(RasterizedGlyph {
            c: c,
            top: glyph.bitmap_top(),
            left: glyph.bitmap_left(),
            width: glyph.bitmap().width() / 3,
            height: glyph.bitmap().rows(),
            buf: packed,
        })
    }
}

pub trait IntoFontconfigType {
    type FcType;
    fn into_fontconfig_type(&self) -> Self::FcType;
}

impl IntoFontconfigType for Slant {
    type FcType = fc::Slant;
    fn into_fontconfig_type(&self) -> Self::FcType {
        match *self {
            Slant::Normal => fc::Slant::Roman,
            Slant::Italic => fc::Slant::Italic,
            Slant::Oblique => fc::Slant::Oblique,
        }
    }
}

impl IntoFontconfigType for Weight {
    type FcType = fc::Weight;

    fn into_fontconfig_type(&self) -> Self::FcType {
        match *self {
            Weight::Normal => fc::Weight::Regular,
            Weight::Bold => fc::Weight::Bold,
        }
    }
}

impl FreeTypeRasterizer {
    /// Load a font face accoring to `FontDesc`
    fn get_face(&mut self, desc: &FontDesc) -> Result<Face<'static>, Error> {
        match desc.style {
            Style::ContainsGlyph(glyph) => {
                self.get_face_with_glyph(&desc, glyph)
            }
            Style::Description { slant, weight } => {
                // Match nearest font
                self.get_matching_face(&desc, slant, weight)
            }
            Style::Specific(ref style) => {
                // If a name was specified, try and load specifically that font.
                self.get_specific_face(&desc, &style)
            }
        }
    }

    fn get_face_with_glyph(&mut self, desc: &FontDesc, glyph: char) -> Result<Face<'static>, Error> {
        let mut pattern = fc::Pattern::new();
        pattern.add_glyph(glyph);

        let fonts = fc::font_sort(fc::Config::get_current(), &mut pattern)
            .ok_or_else(|| Error::MissingFont(desc.to_owned()))?;

        for font in &fonts {
            if let (Some(path), Some(index)) = (font.file(0), font.index(0)) {
                return Ok(self.library.new_face(path, index)?);
            }
        }

        Err(Error::MissingFont(desc.to_owned()))
    }

    fn get_matching_face(
        &mut self,
        desc: &FontDesc,
        slant: Slant,
        weight: Weight
    ) -> Result<Face<'static>, Error> {
        let mut pattern = fc::Pattern::new();
        pattern.add_family(&desc.name);
        pattern.set_weight(weight.into_fontconfig_type());
        pattern.set_slant(slant.into_fontconfig_type());

        let fonts = fc::font_sort(fc::Config::get_current(), &mut pattern)
            .ok_or_else(|| Error::MissingFont(desc.to_owned()))?;

        // Take first font that has a path
        for font in &fonts {
            if let (Some(path), Some(index)) = (font.file(0), font.index(0)) {
                return Ok(self.library.new_face(path, index)?);
            }
        }

        Err(Error::MissingFont(desc.to_owned()))
    }

    fn get_specific_face(
        &mut self,
        desc: &FontDesc,
        style: &str
    ) -> Result<Face<'static>, Error> {
        let mut pattern = fc::Pattern::new();
        pattern.add_family(&desc.name);
        pattern.add_style(style);

        let font = fc::font_match(fc::Config::get_current(), &mut pattern)
            .ok_or_else(|| Error::MissingFont(desc.to_owned()))?;
        if let (Some(path), Some(index)) = (font.file(0), font.index(0)) {
            println!("got font path={:?}", path);
            return Ok(self.library.new_face(path, index)?);
        } else {
            Err(Error::MissingFont(desc.to_owned()))
        }
    }
}

/// Errors occurring when using the freetype rasterizer
#[derive(Debug)]
pub enum Error {
    /// Error occurred within the FreeType library
    FreeType(freetype::Error),

    /// Couldn't find font matching description
    MissingFont(FontDesc),

    /// Requested an operation with a FontKey that isn't known to the rasterizer
    FontNotLoaded,
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::FreeType(ref err) => Some(err),
            _ => None,
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::FreeType(ref err) => err.description(),
            Error::MissingFont(ref _desc) => "couldn't find the requested font",
            Error::FontNotLoaded => "tried to operate on font that hasn't been loaded",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::FreeType(ref err) => {
                err.fmt(f)
            },
            Error::MissingFont(ref desc) => {
                write!(f, "Couldn't find a font with {}\
                       \n\tPlease check the font config in your alacritty.yml.", desc)
            },
            Error::FontNotLoaded => {
                f.write_str("Tried to use a font that hasn't been loaded")
            }
        }
    }
}

impl From<freetype::Error> for Error {
    fn from(val: freetype::Error) -> Error {
        Error::FreeType(val)
    }
}

unsafe impl Send for FreeTypeRasterizer {}
