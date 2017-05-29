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
use std::cmp::min;
use std::path::PathBuf;

use freetype::{self, Library, Face};

mod list_fonts;

use self::list_fonts::fc;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Font {
    face: Face<'static>,
}

#[inline]
fn to_freetype_26_6(f: f32) -> isize {
    ((1i32 << 6) as f32 * f) as isize
}

unsafe impl Send for Font {}

impl Font {

    fn metrics(&self) -> Result<Metrics, Error> {

        let size_metrics = self.face.size_metrics()
            .ok_or(Error::MissingSizeMetrics)?;

        let width = (size_metrics.max_advance / 64) as f64;
        let height = (size_metrics.height / 64) as f64;
        let descent = (size_metrics.descender / 64) as f32;

        Ok(Metrics {
            average_advance: width,
            line_height: height,
            descent: descent,
        })
    }

}



/// Exposed implementation of `RasterizeImpl`.
pub struct RasterizerImpl {
    /// Loaded font paths.
    paths: HashMap<PathBuf, Font>,
    library: Library,
}

impl RasterizerImpl {

    /// Load a font face accoring to `FontDesc`
    fn get_face(&self, desc: &FontDesc) -> Result<Face<'static>, Error> {
        match desc.style {
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

    fn get_matching_face(
        &self,
        desc: &FontDesc,
        slant: Slant,
        weight: Weight
    ) -> Result<Face<'static>, Error> {
        let mut pattern = fc::Pattern::new();
        pattern.add_family(&desc.name);
        pattern.set_weight(weight.into_fontconfig_type());
        pattern.set_slant(slant.into_fontconfig_type());

        let font = fc::font_match(fc::Config::get_current(), &mut pattern)
            .ok_or_else(|| Error::MissingFont(desc.to_owned()))?;

        if let (Some(path), Some(index)) = (font.file(0), font.index(0)) {
            return Ok(self.library.new_face(path, index)?);
        }

        Err(Error::MissingFont(desc.to_owned()))
    }

    fn get_specific_face(
        &self,
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
            Ok(self.library.new_face(path, index)?)
        }
        else {
            Err(Error::MissingFont(desc.to_owned()))
        }
    }

    fn get_rendered_glyph(
        &self,
        c: char,
        font: &Font,
        size: Size,
        dpi_x: u32,
        dpi_y: u32,
        device_pixel_ratio: f32,
    ) -> Result<RasterizedGlyph, Error> {

        let ref face = font.face;

        let fsize = size.as_f32_pts() * device_pixel_ratio;

        face.set_char_size(to_freetype_26_6(fsize), 0, dpi_x, dpi_y)?;
        let index = face.get_char_index(c as usize);
        if index == 0 {
            return Err(Error::MissingGlyph(c));
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

        let (pixel_width, buf) = Self::normalize_buffer(&glyph.bitmap())?;

        Ok(RasterizedGlyph {
            c,
            top: glyph.bitmap_top(),
            left: glyph.bitmap_left(),
            width: pixel_width,
            height: glyph.bitmap().rows(),
            buf: buf,
        })
    }


    /// Given a FreeType `Bitmap`, returns packed buffer with 1 byte per LCD channel.
    ///
    /// The i32 value in the return type is the number of pixels per row.
    fn normalize_buffer(bitmap: &freetype::bitmap::Bitmap) -> freetype::FtResult<(i32, Vec<u8>)> {
        use freetype::bitmap::PixelMode;

        let buf = bitmap.buffer();
        let mut packed = Vec::with_capacity((bitmap.rows() * bitmap.width()) as usize);
        let pitch = bitmap.pitch().abs() as usize;
        match bitmap.pixel_mode()? {
            PixelMode::Lcd => {
                for i in 0..bitmap.rows() {
                    let start = (i as usize) * pitch;
                    let stop = start + bitmap.width() as usize;
                    packed.extend_from_slice(&buf[start..stop]);
                }
                Ok((bitmap.width() / 3, packed))
            },
            // Mono data is stored in a packed format using 1 bit per pixel.
            PixelMode::Mono => {
                fn unpack_byte(res: &mut Vec<u8>, byte: u8, mut count: u8) {
                    // Mono stores MSBit at top of byte
                    let mut bit = 7;
                    while count != 0 {
                        let value = ((byte >> bit) & 1) * 255;
                        // Push value 3x since result buffer should be 1 byte
                        // per channel
                        res.push(value);
                        res.push(value);
                        res.push(value);
                        count -= 1;
                        bit -= 1;
                    }
                };

                for i in 0..(bitmap.rows() as usize) {
                    let mut columns = bitmap.width();
                    let mut byte = 0;
                    let offset = i * bitmap.pitch().abs() as usize;
                    while columns != 0 {
                        let bits = min(8, columns);
                        unpack_byte(&mut packed, buf[offset + byte], bits as u8);

                        columns -= bits;
                        byte += 1;
                    }
                }
                Ok((bitmap.width(), packed))
            },
            // Gray data is stored as a value between 0 and 255 using 1 byte per pixel.
            PixelMode::Gray => {
                for i in 0..bitmap.rows() {
                    let start = (i as usize) * pitch;
                    let stop = start + bitmap.width() as usize;
                    for byte in &buf[start..stop] {
                        packed.push(*byte);
                        packed.push(*byte);
                        packed.push(*byte);
                    }
                }
                Ok((bitmap.width(), packed))
            },
            mode @ _ => panic!("unhandled pixel mode: {:?}", mode)
        }
    }

    fn load_face_with_glyph(
        &mut self,
        glyph: char,
    ) -> Result<PathBuf, Error> {
        let mut charset = fc::CharSet::new();
        charset.add(glyph);
        let mut pattern = fc::Pattern::new();
        pattern.add_charset(&charset);

        let config = fc::Config::get_current();
        match fc::font_match(config, &mut pattern) {
            Some(font) => {
                if let (Some(path), Some(index)) = (font.file(0), font.index(0)) {
                    match self.paths.get(&path) {
                        // We've previously loaded this font, so don't
                        // load it again.
                        Some(_) => {
                            debug!("Hit for font {:?}", path);
                            Ok(path)
                        },

                        None => {
                            debug!("Miss for font {:?}", path);
                            let face = self.library.new_face(&path, index)?;
                            let font = Font {face};
                            self.paths.insert(path.clone(), font);
                            Ok(path)
                        }
                    }
                }
                else {
                Err(Error::MissingFont(
                    FontDesc::new("fallback-without-path", Style::Specific(glyph.to_string()))))
                }
            },
            None => {
                Err(Error::MissingFont(
                    FontDesc::new("no-fallback-for", Style::Specific(glyph.to_string()))
                ))
            }
        }
    }

}

#[allow(unused_variables)]
impl RasterizeImpl for RasterizerImpl {

    type Err = Error;

    fn new() -> Result<RasterizerImpl,Self::Err> {

        // init the freetype library
        let library = Library::init()?;

        Ok(RasterizerImpl {
            paths: HashMap::new(),
            library,
        })
    }

    /// Load the fonts described by `FontDesc` and `Size`, this doesn't
    /// necessarily mean the implementation must load the font
    /// before calls to metrics/get_glyph.
    fn load_font(
        &self,
        details: &Details,
        desc: &FontDesc,
        size: Size,
    ) -> Result<Font, Self::Err> {

        // at this point only get a face that is used
        // to load an actual font according to fontconfig
        // later.
        let face = self.get_face(desc)?;

        Ok(Font {face})
    }

    /// Get the fallback list of `Font` for given `FontDesc` `Font` and `Size`.
    /// This is not used by freetype/fontconfig.
    fn get_fallback_fonts(
        &self,
        details: &Details,
        desc: &FontDesc,
        font: &Font,
        size: Size,
        loaded_names: &Vec<String>,
    ) -> Result<Vec<Font>, Self::Err> {
        Ok(vec![])
    }

    /// Get `Metrics` for the given `Font`
    fn metrics(
        &self,
        details: &Details,
        font: &Font,
    ) -> Result<Metrics, Self::Err> {
        font.metrics()
    }

    /// Rasterize the glyph described by `char` `Font` and `Size`.
    fn get_glyph(
        &self,
        details: &Details,
        c: char,
        encoded: &[u16], // utf16 encoded char
        font: &Font,
        size: Size,
    ) -> Result<RasterizedGlyph, Self::Err> {

        let dpi_x = details.dpi_x as u32;
        let dpi_y = details.dpi_y as u32;

        self.get_rendered_glyph(
            c,
            font,
            size,
            dpi_x,
            dpi_y,
            details.device_pixel_ratio,
        )
    }

    fn get_glyph_fallback(
        &mut self,
        details: &Details,
        c: char,
        encoded: &[u16], // utf16 encoded char
        size: Size,
    ) -> Result<RasterizedGlyph, Self::Err> {

        // this attempts to use fontconfig to do a fallback lookup
        // for the glyph we are rendering
        let font_path = self.load_face_with_glyph(c)?;
        let fallback_font = self.paths.get(&font_path).unwrap();

        // it succeeded, we now got another font to do the rendering with
        self.get_glyph(
            details,
            c,
            encoded,
            fallback_font,
            size,
        )
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



/// Errors occurring when using the freetype rasterizer
#[derive(Debug)]
pub enum Error {
    /// Error occurred within the FreeType library
    FreeType(freetype::Error),

    /// Tried to rasterize a glyph but it was not available
    MissingGlyph(char),

    /// Couldn't find font matching description
    MissingFont(FontDesc),

    /// Tried to get size metrics from a font that didn't have a size
    MissingSizeMetrics,

    /// Requested an operation with a FontKey that isn't known to the rasterizer
    FontNotLoaded,

    /// Loading a `FontDescList` that resulted in no loaded fonts.
    NoFontsForList,

    /// Internal carrier when doing font fallback.
    FallbackFont(PathBuf),
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
            Error::MissingGlyph(_) => "couldn't find the requested glyph",
            Error::MissingFont(_) => "couldn't find the requested font",
            Error::FontNotLoaded => "tried to operate on font that hasn't been loaded",
            Error::MissingSizeMetrics => "tried to get size metrics from a face without a size",
            Error::NoFontsForList => "provided font list didn't result in any loaded font",
            Error::FallbackFont(_) => "internal fallback error, should never be seen",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::FreeType(ref err) => {
                err.fmt(f)
            },
            Error::MissingGlyph(ref c) => {
                write!(f, "Glyph not found for char {:?}", c)
            },
            Error::MissingFont(ref desc) => {
                write!(f, "Couldn't find a font with {}\
                       \n\tPlease check the font config in your alacritty.yml.", desc)
            },
            Error::FontNotLoaded => {
                f.write_str("Tried to use a font that hasn't been loaded")
            },
            Error::MissingSizeMetrics => {
                f.write_str("Tried to get size metrics from a face without a size")
            }
            Error::NoFontsForList => {
                f.write_str("Provided font list didn't result in any loaded font")
            }
            Error::FallbackFont(_) => {
                f.write_str("Internal fallback error, should never be seen")
            }
        }
    }
}

impl From<freetype::Error> for Error {
    fn from(val: freetype::Error) -> Error {
        Error::FreeType(val)
    }
}
