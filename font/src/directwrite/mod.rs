// Copyright 2019 Joe Wilm, The Alacritty Project Contributors
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
//! Rasterization powered by DirectWrite
use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::os::windows::ffi::OsStringExt;

use dwrote::{
    FontCollection, FontFace, FontFallback, FontStretch, FontStyle, FontWeight, GlyphOffset,
    GlyphRunAnalysis, TextAnalysisSource, TextAnalysisSourceMethods, DWRITE_GLYPH_RUN,
};

use winapi::shared::ntdef::{HRESULT, LOCALE_NAME_MAX_LENGTH};
use winapi::um::dwrite;
use winapi::um::winnls::GetUserDefaultLocaleName;

use super::{
    BitmapBuffer, FontDesc, FontKey, GlyphKey, Metrics, RasterizedGlyph, Size, Slant, Style, Weight,
};

/// Cached DirectWrite font.
struct Font {
    face: FontFace,
    family_name: String,
    weight: FontWeight,
    style: FontStyle,
    stretch: FontStretch,
}

pub struct DirectWriteRasterizer {
    fonts: HashMap<FontKey, Font>,
    keys: HashMap<FontDesc, FontKey>,
    device_pixel_ratio: f32,
    available_fonts: FontCollection,
    fallback_sequence: Option<FontFallback>,
}

impl DirectWriteRasterizer {
    fn rasterize_glyph(
        &self,
        face: &FontFace,
        size: Size,
        c: char,
    ) -> Result<RasterizedGlyph, Error> {
        let glyph_index = self.get_glyph_index(face, c)?;

        let em_size = em_size(size);

        let glyph_run = DWRITE_GLYPH_RUN {
            fontFace: unsafe { face.as_ptr() },
            fontEmSize: em_size,
            glyphCount: 1,
            glyphIndices: &glyph_index,
            glyphAdvances: &0.0,
            glyphOffsets: &GlyphOffset::default(),
            isSideways: 0,
            bidiLevel: 0,
        };

        let rendering_mode = face.get_recommended_rendering_mode_default_params(
            em_size,
            self.device_pixel_ratio,
            dwrote::DWRITE_MEASURING_MODE_NATURAL,
        );

        let glyph_analysis = GlyphRunAnalysis::create(
            &glyph_run,
            self.device_pixel_ratio,
            None,
            rendering_mode,
            dwrote::DWRITE_MEASURING_MODE_NATURAL,
            0.0,
            0.0,
        )
        .map_err(Error::DirectWriteError)?;

        let bounds = glyph_analysis
            .get_alpha_texture_bounds(dwrote::DWRITE_TEXTURE_CLEARTYPE_3x1)
            .map_err(Error::DirectWriteError)?;

        let buf = glyph_analysis
            .create_alpha_texture(dwrote::DWRITE_TEXTURE_CLEARTYPE_3x1, bounds)
            .map_err(Error::DirectWriteError)?;

        Ok(RasterizedGlyph {
            c,
            width: (bounds.right - bounds.left) as i32,
            height: (bounds.bottom - bounds.top) as i32,
            top: -bounds.top,
            left: bounds.left,
            buf: BitmapBuffer::RGB(buf),
        })
    }

    fn get_loaded_font(&self, font_key: FontKey) -> Result<&Font, Error> {
        self.fonts.get(&font_key).ok_or(Error::FontNotLoaded)
    }

    fn get_glyph_index(&self, face: &FontFace, c: char) -> Result<u16, Error> {
        let idx = *face
            .get_glyph_indices(&[c as u32])
            .first()
            // DirectWrite returns 0 if the glyph does not exist in the font
            .filter(|glyph_index| **glyph_index != 0)
            .ok_or_else(|| Error::MissingGlyph(c))?;

        Ok(idx)
    }

    fn get_fallback_font(&self, loaded_font: &Font, c: char) -> Option<dwrote::Font> {
        let fallback = self.fallback_sequence.as_ref()?;

        let mut buf = [0u16; 2];
        c.encode_utf16(&mut buf);

        let length = c.len_utf16() as u32;
        let utf16_codepoints = &buf[..length as usize];

        let locale = get_current_locale();

        let text_analysis_source_data = TextAnalysisSourceData { locale: &locale, length };
        let text_analysis_source = TextAnalysisSource::from_text(
            Box::new(text_analysis_source_data),
            Cow::Borrowed(utf16_codepoints),
        );

        let fallback_result = fallback.map_characters(
            &text_analysis_source,
            0,
            length,
            &self.available_fonts,
            Some(&loaded_font.family_name),
            loaded_font.weight,
            loaded_font.style,
            loaded_font.stretch,
        );

        fallback_result.mapped_font
    }
}

impl crate::Rasterize for DirectWriteRasterizer {
    type Err = Error;

    fn new(device_pixel_ratio: f32, _: bool) -> Result<DirectWriteRasterizer, Error> {
        Ok(DirectWriteRasterizer {
            fonts: HashMap::new(),
            keys: HashMap::new(),
            device_pixel_ratio,
            available_fonts: FontCollection::system(),
            fallback_sequence: FontFallback::get_system_fallback(),
        })
    }

    fn metrics(&self, key: FontKey, size: Size) -> Result<Metrics, Error> {
        let face = &self.get_loaded_font(key)?.face;
        let vmetrics = face.metrics().metrics0();

        let scale = em_size(size) * self.device_pixel_ratio / f32::from(vmetrics.designUnitsPerEm);

        let underline_position = f32::from(vmetrics.underlinePosition) * scale;
        let underline_thickness = f32::from(vmetrics.underlineThickness) * scale;

        let strikeout_position = f32::from(vmetrics.strikethroughPosition) * scale;
        let strikeout_thickness = f32::from(vmetrics.strikethroughThickness) * scale;

        let ascent = f32::from(vmetrics.ascent) * scale;
        let descent = -f32::from(vmetrics.descent) * scale;
        let line_gap = f32::from(vmetrics.lineGap) * scale;

        let line_height = f64::from(ascent - descent + line_gap);

        // Since all monospace characters have the same width, we use `!` for horizontal metrics
        let c = '!';
        let glyph_index = self.get_glyph_index(face, c)?;

        let glyph_metrics = face.get_design_glyph_metrics(&[glyph_index], false);
        let hmetrics = glyph_metrics.first().ok_or_else(|| Error::MissingGlyph(c))?;

        let average_advance = f64::from(hmetrics.advanceWidth) * f64::from(scale);

        Ok(Metrics {
            descent,
            average_advance,
            line_height,
            underline_position,
            underline_thickness,
            strikeout_position,
            strikeout_thickness,
        })
    }

    fn load_font(&mut self, desc: &FontDesc, _size: Size) -> Result<FontKey, Error> {
        // Fast path if face is already loaded
        if let Some(key) = self.keys.get(desc) {
            return Ok(*key);
        }

        let family = self
            .available_fonts
            .get_font_family_by_name(&desc.name)
            .ok_or_else(|| Error::MissingFont(desc.clone()))?;

        let font = match desc.style {
            Style::Description { weight, slant } => {
                // This searches for the "best" font - should mean we don't have to worry about
                // fallbacks if our exact desired weight/style isn't available
                Ok(family.get_first_matching_font(weight.into(), FontStretch::Normal, slant.into()))
            }
            Style::Specific(ref style) => {
                let mut idx = 0;
                let count = family.get_font_count();

                loop {
                    if idx == count {
                        break Err(Error::MissingFont(desc.clone()));
                    }

                    let font = family.get_font(idx);

                    if font.face_name() == *style {
                        break Ok(font);
                    }

                    idx += 1;
                }
            }
        }?;

        let key = FontKey::next();
        self.keys.insert(desc.clone(), key);
        self.fonts.insert(key, font.into());

        Ok(key)
    }

    fn get_glyph(&mut self, glyph: GlyphKey) -> Result<RasterizedGlyph, Error> {
        let loaded_font = self.get_loaded_font(glyph.font_key)?;

        match self.rasterize_glyph(&loaded_font.face, glyph.size, glyph.c) {
            Err(err @ Error::MissingGlyph(_)) => {
                let fallback_font = self.get_fallback_font(&loaded_font, glyph.c).ok_or(err)?;
                self.rasterize_glyph(&fallback_font.create_font_face(), glyph.size, glyph.c)
            },
            result => result,
        }
    }

    fn update_dpr(&mut self, device_pixel_ratio: f32) {
        self.device_pixel_ratio = device_pixel_ratio;
    }
}

#[derive(Debug)]
pub enum Error {
    MissingFont(FontDesc),
    MissingGlyph(char),
    FontNotLoaded,
    DirectWriteError(HRESULT),
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Error::MissingGlyph(c) => write!(f, "Glyph not found for char {:?}", c),
            Error::MissingFont(desc) => write!(
                f,
                "Couldn't find a font with {}\n\tPlease check the font config in your \
                 alacritty.yml.",
                desc
            ),
            Error::FontNotLoaded => f.write_str("Tried to use a font that hasn't been loaded"),
            Error::DirectWriteError(hresult) => {
                write!(f, "A DirectWrite rendering error occurred: {:#X}", hresult)
            }
        }
    }
}

fn em_size(size: Size) -> f32 {
    size.as_f32_pts() * (96.0 / 72.0)
}

impl From<dwrote::Font> for Font {
    fn from(font: dwrote::Font) -> Font {
        Font {
            face: font.create_font_face(),
            family_name: font.family_name(),
            weight: font.weight(),
            style: font.style(),
            stretch: font.stretch(),
        }
    }
}

impl From<Weight> for FontWeight {
    fn from(weight: Weight) -> FontWeight {
        match weight {
            Weight::Bold => FontWeight::Bold,
            Weight::Normal => FontWeight::Regular,
        }
    }
}

impl From<Slant> for FontStyle {
    fn from(slant: Slant) -> FontStyle {
        match slant {
            Slant::Oblique => FontStyle::Oblique,
            Slant::Italic => FontStyle::Italic,
            Slant::Normal => FontStyle::Normal,
        }
    }
}

fn get_current_locale() -> String {
    let mut buf = vec![0u16; LOCALE_NAME_MAX_LENGTH];
    let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) as usize };

    // `len` includes null byte, which we don't need in Rust
    OsString::from_wide(&buf[..len - 1]).into_string().expect("Locale not valid unicode")
}

/// Font fallback information for dwrote's TextAnalysisSource.
struct TextAnalysisSourceData<'a> {
    locale: &'a str,
    length: u32,
}

impl TextAnalysisSourceMethods for TextAnalysisSourceData<'_> {
    fn get_locale_name(&self, _text_position: u32) -> (Cow<str>, u32) {
        (Cow::Borrowed(self.locale), self.length)
    }

    fn get_paragraph_reading_direction(&self) -> dwrite::DWRITE_READING_DIRECTION {
        dwrite::DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
    }
}
