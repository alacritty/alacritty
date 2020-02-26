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
//! Rasterization powered by FreeType and Fontconfig.
use std::cmp::{min, Ordering};
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::rc::Rc;

use freetype::tt_os2::TrueTypeOS2Table;
use freetype::{self, Library};
use freetype::{freetype_sys, Face as FTFace};
use libc::c_uint;
use log::{debug, trace};

pub mod fc;

use fc::{CharSet, Pattern, PatternHash, PatternRef};

use super::{
    BitmapBuffer, FontDesc, FontKey, GlyphKey, Metrics, Rasterize, RasterizedGlyph, Size, Slant,
    Style, Weight,
};

struct FallbackFont {
    pattern: Pattern,
    key: FontKey,
}

impl FallbackFont {
    fn new(pattern: Pattern, key: FontKey) -> FallbackFont {
        Self { pattern, key }
    }
}

impl FontKey {
    fn from_pattern_hashes(lhs: PatternHash, rhs: PatternHash) -> Self {
        // XOR two hashes to get a font ID
        Self { token: lhs.0.rotate_left(1) ^ rhs.0 }
    }
}

#[derive(Default)]
struct FallbackList {
    list: Vec<FallbackFont>,
    coverage: CharSet,
}

struct FaceLoadingProperties {
    load_flags: freetype::face::LoadFlag,
    render_mode: freetype::RenderMode,
    lcd_filter: c_uint,
    pixelsize: f64,
    colored: bool,
    pixelsize_fixup_factor: Option<f64>,
    ft_face: Rc<FTFace>,
}

impl fmt::Debug for FaceLoadingProperties {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Face")
            .field("ft_face", &self.ft_face)
            .field("load_flags", &self.load_flags)
            .field("render_mode", &match self.render_mode {
                freetype::RenderMode::Normal => "Normal",
                freetype::RenderMode::Light => "Light",
                freetype::RenderMode::Mono => "Mono",
                freetype::RenderMode::Lcd => "Lcd",
                freetype::RenderMode::LcdV => "LcdV",
                freetype::RenderMode::Max => "Max",
            })
            .field("lcd_filter", &self.lcd_filter)
            .finish()
    }
}

/// Rasterizes glyphs for a single font face.
pub struct FreeTypeRasterizer {
    library: Library,
    faces: HashMap<FontKey, FaceLoadingProperties>,
    ft_faces: HashMap<PathBuf, Rc<FTFace>>,
    fallback_lists: HashMap<FontKey, FallbackList>,
    device_pixel_ratio: f32,
}

#[inline]
fn to_freetype_26_6(f: f32) -> isize {
    ((1i32 << 6) as f32 * f) as isize
}

impl Rasterize for FreeTypeRasterizer {
    type Err = Error;

    fn new(device_pixel_ratio: f32, _: bool) -> Result<FreeTypeRasterizer, Error> {
        let library = Library::init()?;

        Ok(FreeTypeRasterizer {
            faces: HashMap::new(),
            ft_faces: HashMap::new(),
            fallback_lists: HashMap::new(),
            library,
            device_pixel_ratio,
        })
    }

    fn metrics(&self, key: FontKey, _size: Size) -> Result<Metrics, Error> {
        let face = &mut self.faces.get(&key).ok_or(Error::FontNotLoaded)?;
        let full = self.full_metrics(&face)?;

        let height = (full.size_metrics.height / 64) as f64;
        let descent = (full.size_metrics.descender / 64) as f32;

        // Get underline position and thickness in device pixels
        let x_scale = full.size_metrics.x_scale as f32 / 65536.0;
        let mut underline_position = f32::from(face.ft_face.underline_position()) * x_scale / 64.;
        let mut underline_thickness = f32::from(face.ft_face.underline_thickness()) * x_scale / 64.;

        // Fallback for bitmap fonts which do not provide underline metrics
        if underline_position == 0. {
            underline_thickness = (descent.abs() / 5.).round();
            underline_position = descent / 2.;
        }

        // Get strikeout position and thickness in device pixels
        let (strikeout_position, strikeout_thickness) =
            match TrueTypeOS2Table::from_face(&mut (*face.ft_face).clone()) {
                Some(os2) => {
                    let strikeout_position = f32::from(os2.y_strikeout_position()) * x_scale / 64.;
                    let strikeout_thickness = f32::from(os2.y_strikeout_size()) * x_scale / 64.;
                    (strikeout_position, strikeout_thickness)
                },
                _ => {
                    // Fallback if font doesn't provide info about strikeout
                    trace!("Using fallback strikeout metrics");
                    let strikeout_position = height as f32 / 2. + descent;
                    (strikeout_position, underline_thickness)
                },
            };

        Ok(Metrics {
            average_advance: full.cell_width,
            line_height: height,
            descent,
            underline_position,
            underline_thickness,
            strikeout_position,
            strikeout_thickness,
        })
    }

    fn load_font(&mut self, desc: &FontDesc, size: Size) -> Result<FontKey, Error> {
        self.get_face(desc, size)
    }

    fn get_glyph(&mut self, glyph_key: GlyphKey) -> Result<RasterizedGlyph, Error> {
        self.get_rendered_glyph(glyph_key)
    }

    fn update_dpr(&mut self, device_pixel_ratio: f32) {
        self.device_pixel_ratio = device_pixel_ratio;
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

struct FullMetrics {
    size_metrics: freetype::ffi::FT_Size_Metrics,
    cell_width: f64,
}

impl FreeTypeRasterizer {
    /// Load a font face according to `FontDesc`
    fn get_face(&mut self, desc: &FontDesc, size: Size) -> Result<FontKey, Error> {
        // Adjust for DPI
        let size = f64::from(size.as_f32_pts() * self.device_pixel_ratio * 96. / 72.);

        let config = fc::Config::get_current();
        let mut pattern = Pattern::new();
        pattern.add_family(&desc.name);
        pattern.add_pixelsize(size);
        let hash = pattern.hash();

        // Add style to a pattern
        match desc.style {
            Style::Description { slant, weight } => {
                // Match nearest font
                pattern.set_weight(weight.into_fontconfig_type());
                pattern.set_slant(slant.into_fontconfig_type());
            },
            Style::Specific(ref style) => {
                // If a name was specified, try and load specifically that font
                pattern.add_style(style);
            },
        }

        pattern.config_substitute(config, fc::MatchKind::Pattern);
        pattern.default_substitute();

        // Get font list using pattern. First font is the primary one while the rest are fallbacks
        let matched_fonts =
            fc::font_sort(&config, &pattern).ok_or_else(|| Error::MissingFont(desc.to_owned()))?;
        let mut matched_fonts = matched_fonts.into_iter();

        let primary_font =
            matched_fonts.next().ok_or_else(|| Error::MissingFont(desc.to_owned()))?;

        // We should render patterns to get values like `pixelsizefixupfactor`
        let primary_font = pattern.render_prepare(config, primary_font);

        // Hash pattern together with request pattern to include requested font size in the hash
        let primary_font_key = FontKey::from_pattern_hashes(hash, primary_font.hash());

        // Return if we already have the same primary font
        if self.fallback_lists.contains_key(&primary_font_key) {
            return Ok(primary_font_key);
        }

        // Load font if we haven't loaded it yet
        if !self.faces.contains_key(&primary_font_key) {
            self.face_from_pattern(&primary_font, primary_font_key)
                .and_then(|pattern| pattern.ok_or_else(|| Error::MissingFont(desc.to_owned())))?;
        }

        // Coverage for fallback fonts
        let coverage = CharSet::new();
        let empty_charset = CharSet::new();

        let list: Vec<FallbackFont> = matched_fonts
            .map(|fallback_font| {
                let charset = fallback_font.get_charset().unwrap_or(&empty_charset);

                // Use original pattern to preserve loading flags
                let fallback_font = pattern.render_prepare(config, fallback_font);
                let fallback_font_key = FontKey::from_pattern_hashes(hash, fallback_font.hash());

                let _ = coverage.merge(&charset);

                FallbackFont::new(fallback_font, fallback_font_key)
            })
            .collect();

        self.fallback_lists.insert(primary_font_key, FallbackList { list, coverage });

        Ok(primary_font_key)
    }

    fn full_metrics(&self, face_load_props: &FaceLoadingProperties) -> Result<FullMetrics, Error> {
        let ft_face = &face_load_props.ft_face;
        let size_metrics = ft_face.size_metrics().ok_or(Error::MissingSizeMetrics)?;

        let width = match ft_face.load_char('0' as usize, face_load_props.load_flags) {
            Ok(_) => ft_face.glyph().metrics().horiAdvance / 64,
            Err(_) => size_metrics.max_advance / 64,
        } as f64;

        Ok(FullMetrics { size_metrics, cell_width: width })
    }

    fn load_ft_face(&mut self, path: PathBuf, index: isize) -> Result<Rc<FTFace>, Error> {
        let mut ft_face = self.library.new_face(&path, index)?;
        if ft_face.has_color() {
            unsafe {
                // Select the colored bitmap size to use from the array of available sizes
                freetype_sys::FT_Select_Size(ft_face.raw_mut(), 0);
            }
        }

        let ft_face = Rc::new(ft_face);
        self.ft_faces.insert(path, Rc::clone(&ft_face));

        Ok(ft_face)
    }

    fn face_from_pattern(
        &mut self,
        pattern: &PatternRef,
        font_key: FontKey,
    ) -> Result<Option<FontKey>, Error> {
        if let (Some(path), Some(index)) = (pattern.file(0), pattern.index().next()) {
            if self.faces.get(&font_key).is_some() {
                return Ok(Some(font_key));
            }

            trace!("Got font path={:?}", path);

            let ft_face = match self.ft_faces.get(&path) {
                Some(ft_face) => Rc::clone(ft_face),
                None => self.load_ft_face(path, index)?,
            };

            // Get available pixel sizes if font isn't scalable.
            let pixelsize =
                pattern.pixelsize().next().expect("Font is missing pixelsize information.");

            let pixelsize_fixup_factor = pattern.pixelsizefixupfactor().next();

            let face = FaceLoadingProperties {
                load_flags: Self::ft_load_flags(pattern),
                render_mode: Self::ft_render_mode(pattern),
                lcd_filter: Self::ft_lcd_filter(pattern),
                pixelsize,
                colored: ft_face.has_color(),
                pixelsize_fixup_factor,
                ft_face,
            };

            debug!("Loaded Face {:?}", face);

            self.faces.insert(font_key, face);

            Ok(Some(font_key))
        } else {
            Ok(None)
        }
    }

    fn face_for_glyph(&mut self, glyph_key: GlyphKey) -> Result<FontKey, Error> {
        if let Some(face) = self.faces.get(&glyph_key.font_key) {
            let index = face.ft_face.get_char_index(glyph_key.c as usize);

            if index != 0 {
                return Ok(glyph_key.font_key);
            }
        }

        Ok(self.load_face_with_glyph(glyph_key).unwrap_or(glyph_key.font_key))
    }

    fn load_face_with_glyph(&mut self, glyph: GlyphKey) -> Result<FontKey, Error> {
        let fallback_list = self.fallback_lists.get(&glyph.font_key).unwrap();

        // Check whether glyph is presented in any fallback font
        if !fallback_list.coverage.has_char(glyph.c) {
            return Ok(glyph.font_key);
        }

        for fallback_font in &fallback_list.list {
            let font_key = fallback_font.key;
            let font_pattern = &fallback_font.pattern;
            match self.faces.get(&font_key) {
                Some(face) => {
                    let index = face.ft_face.get_char_index(glyph.c as usize);

                    // We found something in a current face, so let's use it
                    if index != 0 {
                        return Ok(font_key);
                    }
                },
                None => {
                    if font_pattern.get_charset().map(|cs| cs.has_char(glyph.c)) != Some(true) {
                        continue;
                    }

                    let pattern = font_pattern.clone();
                    let key = self.face_from_pattern(&pattern, font_key)?.unwrap();

                    return Ok(key);
                },
            }
        }

        // You can hit this return, if you're failing to get charset from a pattern
        Ok(glyph.font_key)
    }

    fn get_rendered_glyph(&mut self, glyph_key: GlyphKey) -> Result<RasterizedGlyph, Error> {
        // Render a normal character if it's not a cursor
        let font_key = self.face_for_glyph(glyph_key)?;
        let face = &self.faces[&font_key];
        let index = face.ft_face.get_char_index(glyph_key.c as usize);

        if !face.colored {
            face.ft_face.set_char_size(to_freetype_26_6(face.pixelsize as f32), 0, 0, 0)?;
        }

        unsafe {
            let ft_lib = self.library.raw();
            freetype::ffi::FT_Library_SetLcdFilter(ft_lib, face.lcd_filter);
        }

        face.ft_face.load_glyph(index as u32, face.load_flags)?;

        let glyph = face.ft_face.glyph();
        glyph.render_glyph(face.render_mode)?;

        let (pixel_height, pixel_width, buf) = Self::normalize_buffer(&glyph.bitmap())?;

        let rasterized_glyph = RasterizedGlyph {
            c: glyph_key.c,
            top: glyph.bitmap_top(),
            left: glyph.bitmap_left(),
            width: pixel_width,
            height: pixel_height,
            buf,
        };

        if face.colored {
            let fixup_factor = if let Some(pixelsize_fixup_factor) = face.pixelsize_fixup_factor {
                pixelsize_fixup_factor
            } else {
                // Fallback if user has bitmap scaling disabled
                let metrics = face.ft_face.size_metrics().ok_or(Error::MissingSizeMetrics)?;
                face.pixelsize / metrics.y_ppem as f64
            };
            Ok(downsample_bitmap(rasterized_glyph, fixup_factor))
        } else {
            Ok(rasterized_glyph)
        }
    }

    fn ft_load_flags(pattern: &PatternRef) -> freetype::face::LoadFlag {
        let antialias = pattern.antialias().next().unwrap_or(true);
        let hinting = pattern.hintstyle().next().unwrap_or(fc::HintStyle::Slight);
        let rgba = pattern.rgba().next().unwrap_or(fc::Rgba::Unknown);
        let embedded_bitmaps = pattern.embeddedbitmap().next().unwrap_or(true);
        let scalable = pattern.scalable().next().unwrap_or(true);
        let color = pattern.color().next().unwrap_or(false);

        use freetype::face::LoadFlag;
        let mut flags = match (antialias, hinting, rgba) {
            (false, fc::HintStyle::None, _) => LoadFlag::NO_HINTING | LoadFlag::MONOCHROME,
            (false, ..) => LoadFlag::TARGET_MONO | LoadFlag::MONOCHROME,
            (true, fc::HintStyle::None, _) => LoadFlag::NO_HINTING | LoadFlag::TARGET_NORMAL,
            // hintslight does *not* use LCD hinting even when a subpixel mode
            // is selected.
            //
            // According to the FreeType docs,
            //
            // > You can use a hinting algorithm that doesn't correspond to the
            // > same rendering mode.  As an example, it is possible to use the
            // > ‘light’ hinting algorithm and have the results rendered in
            // > horizontal LCD pixel mode.
            //
            // In practice, this means we can have `FT_LOAD_TARGET_LIGHT` with
            // subpixel render modes like `FT_RENDER_MODE_LCD`. Libraries like
            // cairo take the same approach and consider `hintslight` to always
            // prefer `FT_LOAD_TARGET_LIGHT`
            (true, fc::HintStyle::Slight, _) => LoadFlag::TARGET_LIGHT,
            // If LCD hinting is to be used, must select hintmedium or hintfull,
            // have AA enabled, and select a subpixel mode.
            (true, _, fc::Rgba::Rgb) | (true, _, fc::Rgba::Bgr) => LoadFlag::TARGET_LCD,
            (true, _, fc::Rgba::Vrgb) | (true, _, fc::Rgba::Vbgr) => LoadFlag::TARGET_LCD_V,
            // For non-rgba modes with either Medium or Full hinting, just use
            // the default hinting algorithm.
            //
            // TODO should Medium/Full control whether to use the auto hinter?
            (true, _, fc::Rgba::Unknown) => LoadFlag::TARGET_NORMAL,
            (true, _, fc::Rgba::None) => LoadFlag::TARGET_NORMAL,
        };

        // Non scalable fonts only have bitmaps, so disabling them entirely is likely not a
        // desirable thing. Colored fonts aren't scalable, but also only have bitmaps.
        if !embedded_bitmaps && scalable && !color {
            flags |= LoadFlag::NO_BITMAP;
        }

        if color {
            flags |= LoadFlag::COLOR;
        }

        flags
    }

    fn ft_render_mode(pat: &PatternRef) -> freetype::RenderMode {
        let antialias = pat.antialias().next().unwrap_or(true);
        let rgba = pat.rgba().next().unwrap_or(fc::Rgba::Unknown);

        match (antialias, rgba) {
            (false, _) => freetype::RenderMode::Mono,
            (_, fc::Rgba::Rgb) | (_, fc::Rgba::Bgr) => freetype::RenderMode::Lcd,
            (_, fc::Rgba::Vrgb) | (_, fc::Rgba::Vbgr) => freetype::RenderMode::LcdV,
            (true, _) => freetype::RenderMode::Normal,
        }
    }

    fn ft_lcd_filter(pat: &PatternRef) -> c_uint {
        match pat.lcdfilter().next().unwrap_or(fc::LcdFilter::Default) {
            fc::LcdFilter::None => freetype::ffi::FT_LCD_FILTER_NONE,
            fc::LcdFilter::Default => freetype::ffi::FT_LCD_FILTER_DEFAULT,
            fc::LcdFilter::Light => freetype::ffi::FT_LCD_FILTER_LIGHT,
            fc::LcdFilter::Legacy => freetype::ffi::FT_LCD_FILTER_LEGACY,
        }
    }

    /// Given a FreeType `Bitmap`, returns packed buffer with 1 byte per LCD channel.
    ///
    /// The i32 value in the return type is the number of pixels per row.
    fn normalize_buffer(
        bitmap: &freetype::bitmap::Bitmap,
    ) -> freetype::FtResult<(i32, i32, BitmapBuffer)> {
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
                Ok((bitmap.rows(), bitmap.width() / 3, BitmapBuffer::RGB(packed)))
            },
            PixelMode::LcdV => {
                for i in 0..bitmap.rows() / 3 {
                    for j in 0..bitmap.width() {
                        for k in 0..3 {
                            let offset = ((i as usize) * 3 + k) * pitch + (j as usize);
                            packed.push(buf[offset]);
                        }
                    }
                }
                Ok((bitmap.rows() / 3, bitmap.width(), BitmapBuffer::RGB(packed)))
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
                Ok((bitmap.rows(), bitmap.width(), BitmapBuffer::RGB(packed)))
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
                Ok((bitmap.rows(), bitmap.width(), BitmapBuffer::RGB(packed)))
            },
            PixelMode::Bgra => {
                let buf_size = (bitmap.rows() * bitmap.width() * 4) as usize;
                let mut i = 0;
                while i < buf_size {
                    packed.push(buf[i + 2]);
                    packed.push(buf[i + 1]);
                    packed.push(buf[i]);
                    packed.push(buf[i + 3]);
                    i += 4;
                }
                Ok((bitmap.rows(), bitmap.width(), BitmapBuffer::RGBA(packed)))
            },
            mode => panic!("unhandled pixel mode: {:?}", mode),
        }
    }
}

/// Downscale a bitmap by a fixed factor.
///
/// This will take the `bitmap_glyph` as input and return the glyph's content downscaled by
/// `fixup_factor`.
fn downsample_bitmap(mut bitmap_glyph: RasterizedGlyph, fixup_factor: f64) -> RasterizedGlyph {
    // Only scale colored buffers which are bigger than required
    let bitmap_buffer = match (&bitmap_glyph.buf, fixup_factor.partial_cmp(&1.0)) {
        (BitmapBuffer::RGBA(buffer), Some(Ordering::Less)) => buffer,
        _ => return bitmap_glyph,
    };

    let bitmap_width = bitmap_glyph.width as usize;
    let bitmap_height = bitmap_glyph.height as usize;

    let target_width = (bitmap_width as f64 * fixup_factor) as usize;
    let target_height = (bitmap_height as f64 * fixup_factor) as usize;

    // Number of pixels in the input buffer, per pixel in the output buffer
    let downsampling_step = 1.0 / fixup_factor;

    let mut downsampled_buffer = Vec::<u8>::with_capacity(target_width * target_height * 4);

    for line_index in 0..target_height {
        // Get the first and last line which will be consolidated in the current output pixel
        let line_index = line_index as f64;
        let source_line_start = (line_index * downsampling_step).round() as usize;
        let source_line_end = ((line_index + 1.) * downsampling_step).round() as usize;

        for column_index in 0..target_width {
            // Get the first and last column which will be consolidated in the current output pixel
            let column_index = column_index as f64;
            let source_column_start = (column_index * downsampling_step).round() as usize;
            let source_column_end = ((column_index + 1.) * downsampling_step).round() as usize;

            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            let mut pixels_picked: u32 = 0;

            // Consolidate all pixels within the source rectangle into a single averaged pixel
            for source_line in source_line_start..source_line_end {
                let source_pixel_index = source_line * bitmap_width;

                for source_column in source_column_start..source_column_end {
                    let offset = (source_pixel_index + source_column) * 4;
                    r += bitmap_buffer[offset] as u32;
                    g += bitmap_buffer[offset + 1] as u32;
                    b += bitmap_buffer[offset + 2] as u32;
                    a += bitmap_buffer[offset + 3] as u32;
                    pixels_picked += 1;
                }
            }

            // Add a single pixel to the output buffer for the downscaled source rectangle
            downsampled_buffer.push((r / pixels_picked) as u8);
            downsampled_buffer.push((g / pixels_picked) as u8);
            downsampled_buffer.push((b / pixels_picked) as u8);
            downsampled_buffer.push((a / pixels_picked) as u8);
        }
    }

    bitmap_glyph.buf = BitmapBuffer::RGBA(downsampled_buffer);

    // Downscale the metrics
    bitmap_glyph.top = (bitmap_glyph.top as f64 * fixup_factor) as i32;
    bitmap_glyph.left = (bitmap_glyph.left as f64 * fixup_factor) as i32;
    bitmap_glyph.width = target_width as i32;
    bitmap_glyph.height = target_height as i32;

    bitmap_glyph
}

/// Errors occurring when using the freetype rasterizer
#[derive(Debug)]
pub enum Error {
    /// Error occurred within the FreeType library
    FreeType(freetype::Error),

    /// Couldn't find font matching description
    MissingFont(FontDesc),

    /// Tried to get size metrics from a Face that didn't have a size
    MissingSizeMetrics,

    /// Requested an operation with a FontKey that isn't known to the rasterizer
    FontNotLoaded,
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::FreeType(err) => err.source(),
            _ => None,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Error::FreeType(err) => err.fmt(f),
            Error::MissingFont(err) => write!(
                f,
                "Couldn't find a font with {}\n\tPlease check the font config in your \
                 alacritty.yml.",
                err
            ),
            Error::FontNotLoaded => f.write_str("Tried to use a font that hasn't been loaded"),
            Error::MissingSizeMetrics => {
                f.write_str("Tried to get size metrics from a face without a size")
            },
        }
    }
}

impl From<freetype::Error> for Error {
    fn from(val: freetype::Error) -> Error {
        Error::FreeType(val)
    }
}

unsafe impl Send for FreeTypeRasterizer {}
