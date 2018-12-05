use std::error;

use super::*;

use font_kit::source::SystemSource;
use font_kit::properties::{self, Properties};
use font_kit::family_name::FamilyName;
use font_kit::font::Font;
use font_kit::canvas::{Canvas, RasterizationOptions, Format};

#[derive(Debug, From, Display)]
pub enum Error {
    FontLoadingError(font_kit::error::FontLoadingError),
    SelectionError(font_kit::error::SelectionError),
    GlyphLoadingError(font_kit::error::GlyphLoadingError),
}

impl error::Error for Error {}

impl Into<properties::Weight> for Weight {
    fn into(self) -> properties::Weight {
        match self {
            Weight::Normal => properties::Weight::NORMAL,
            Weight::Bold => properties::Weight::BOLD,
        }
    }
}

impl Into<properties::Style> for Slant {
    fn into(self) -> properties::Style {
        match self {
            Slant::Normal => properties::Style::Normal,
            Slant::Italic => properties::Style::Italic,
            Slant::Oblique => properties::Style::Oblique
        }
    }
}

/// Rasterizer using font-kit.
/// Uses DirectWrite on Windows, FreeType on Linux, CoreText on OSX
pub struct FontKitRasterizer {
    source: SystemSource,
    dpi: f32,
    options: Options,
    fonts: Vec<Font>
}

impl Into<RasterizationOptions> for RasterizationMethod {
    fn into(self) -> RasterizationOptions {
        match self {
            RasterizationMethod::SubpixelAa => RasterizationOptions::SubpixelAa,
            RasterizationMethod::GrayScaleAa => RasterizationOptions::GrayscaleAa
        }
    }
}

impl ::Rasterize for FontKitRasterizer {
    type Err = Error;

    fn new(device_pixel_ratio: f32, options: &Options) -> Result<Self, Self::Err> {
        Ok(
            Self{
                source: SystemSource::new(),
                dpi: device_pixel_ratio,
                options: *options,
                fonts: Vec::new()
            }
        )
    }

    fn update_dpr(&mut self, device_pixel_ratio: f32) {
        self.dpi = device_pixel_ratio;
    }

    fn metrics(&self, key: FontKey, size: Size) -> Result<Metrics, Self::Err> {
        let font = &self.fonts[key.token as usize];
        let metrics = font.metrics();

        let scale = size.as_f32_pts() * self.dpi * 96. / 72. / metrics.units_per_em as f32;

        let line_height = (Into::<f32>::into(metrics.line_gap - metrics.descent + metrics.ascent)) as f64;

        Ok(Metrics {
            // If the font is monospaced all glyphs *should* have the same width
            // 33 '!' is the first displaying character
            average_advance: (font.advance(33)?.x * scale) as f64,
            line_height: (line_height * scale as f64),
            descent: metrics.descent * scale
        })
    }

    fn load_font(&mut self, desc: &FontDesc, _size: Size) -> Result<FontKey, Self::Err> {
        let mut p = Properties::new();
        self.fonts.push(self.source.select_best_match(
            &[FamilyName::Title(desc.name.clone())],
            match desc.style {
                Style::Specific(_) => unimplemented!(),
                Style::Description{slant, weight} => p.weight(weight.into()).style(slant.into())
            }
        )?.load()?);

        Ok(FontKey{token: (self.fonts.len() - 1) as u16})
    }

    fn get_glyph(&mut self, glyph_key: GlyphKey) -> Result<RasterizedGlyph, Self::Err> {
        match glyph_key.c {
            super::UNDERLINE_CURSOR_CHAR => {
                let metrics = self.metrics(glyph_key.font_key, glyph_key.size)?;
                return super::get_underline_cursor_glyph(metrics.descent as i32, metrics.average_advance as i32);
            }
            super::BEAM_CURSOR_CHAR => {
                let metrics = self.metrics(glyph_key.font_key, glyph_key.size)?;

                return super::get_beam_cursor_glyph(
                    (metrics.line_height + f64::from(metrics.descent)).round() as i32,
                    metrics.line_height.round() as i32,
                    metrics.average_advance.round() as i32
                );
            }
            super::BOX_CURSOR_CHAR => {
                let metrics = self.metrics(glyph_key.font_key, glyph_key.size)?;

                return super::get_box_cursor_glyph(
                    (metrics.line_height + f64::from(metrics.descent)).round() as i32,
                    metrics.line_height.round() as i32,
                    metrics.average_advance.round() as i32
                );
            }
            _ => ()
        }

        let font = &self.fonts[glyph_key.font_key.token as usize];
        let glyph = font.glyph_for_char(glyph_key.c).unwrap();
        let metrics = font.metrics();
        let scale = glyph_key.size.as_f32_pts() * self.dpi  / metrics.units_per_em as f32;
        // TODO: This is too small currently
        let baseline = (glyph_key.size.as_f32_pts() / (metrics.ascent + metrics.descent)) * metrics.ascent;
        let line_height = (Into::<f32>::into(metrics.line_gap - metrics.descent + metrics.ascent)) as f64;

        let origin = font.origin(glyph)? * scale;

        let bounds = font.raster_bounds(
            glyph,
            glyph_key.size.as_f32_pts(),
            &origin, 
            self.options.hinting.scale(glyph_key.size.as_f32_pts()),
            self.options.rasterization_method.into()
        )?;

        let mut canvas = Canvas::new(&bounds.size.cast::<u32>(), Format::Rgb24);

        // https://github.com/pcwalton/font-kit/issues/7
        if bounds.size.width != 0 && bounds.size.height != 0 {
            font.rasterize_glyph(
                &mut canvas,
                glyph,
                glyph_key.size.as_f32_pts(),
                &origin,
                self.options.hinting.scale(glyph_key.size.as_f32_pts()),
                self.options.rasterization_method.into()
            )?;
        }

        Ok(RasterizedGlyph {
            c: glyph_key.c,
            width: bounds.size.width,
            height: bounds.size.height,
            top: bounds.size.height + bounds.origin.y - (baseline.round() / 2.0) as i32,
            left: bounds.origin.x,
            buf: canvas.pixels.clone(),
        })
    }
}