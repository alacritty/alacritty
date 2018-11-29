use std::error;
use std::fmt::{self, Display, Formatter};

use log::trace;

use super::{FontDesc, FontKey, GlyphKey, Metrics, RasterizedGlyph, Size, Slant, Style, Weight};

// TODO: Move into parent crate
extern crate font_kit;

use self::font_kit::source::SystemSource;
use self::font_kit::properties::{self, Properties};
use self::font_kit::family_name::FamilyName;
use self::font_kit::font::Font;
use self::font_kit::canvas::{Canvas, RasterizationOptions, Format};
use self::font_kit::hinting::HintingOptions;

#[cfg(windows)]
use self::font_kit::sources::directwrite::DirectWriteSource;

#[derive(Debug)]
pub enum Error {
    SelectionError(font_kit::error::SelectionError)
}

impl Into<Error> for font_kit::error::SelectionError {
    fn into(self) -> Error {
        Error::SelectionError(self)
    }
}

impl error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        unimplemented!();
    }
}

// Unfortunate specialisation. Is there a way around this?
pub struct FontKitRasterizer {
    // FIXME: This should be generic
    source: DirectWriteSource,
    dpi: f32,
    fonts: Vec<Font>
}

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

impl ::Rasterize for FontKitRasterizer {
    type Err = Error;

    #[cfg(windows)]
    fn new(device_pixel_ratio: f32, _use_thin_strokes: bool) -> Result<Self, Self::Err> {
        Ok(
            Self{
                source: SystemSource::new(),
                dpi: device_pixel_ratio,
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

        let raw_advance = font.advance(33).unwrap().x;
        let line_height = (Into::<f32>::into(metrics.line_gap - metrics.descent + metrics.ascent)) as f64;

        trace!("FONTKIT {:#?}", metrics);

        let metrics = Metrics {
            // If the font is monospaced all glyphs *should* have the same width
            // 33 '!' is the first displaying character
            average_advance: (raw_advance * scale) as f64,
            line_height: (line_height * scale as f64),
            descent: metrics.descent * scale
        };
        trace!("FINAL {:#?}", metrics);
        Ok(metrics)
    }

    fn load_font(&mut self, desc: &FontDesc, _size: Size) -> Result<FontKey, Self::Err> {
        let mut p = Properties::new();
        self.fonts.push(self.source.select_best_match(
            &[FamilyName::Title(desc.name.clone())],
            match desc.style {
                Style::Specific(_) => unimplemented!(),
                Style::Description{slant, weight} => p.weight(weight.into()).style(slant.into())
            }
        // FIXME: Error handling
        ).unwrap().load().unwrap());

        Ok(FontKey{token: (self.fonts.len() - 1) as u16})
    }

    fn get_glyph(&mut self, glyph_key: GlyphKey) -> Result<RasterizedGlyph, Self::Err> {
        let font = &self.fonts[glyph_key.font_key.token as usize];
        // TODO: Error/fallback handling
        let glyph = font.glyph_for_char(glyph_key.c).unwrap();
        let metrics = font.metrics();
        let scale = glyph_key.size.as_f32_pts() * self.dpi * 96. / 72. / metrics.units_per_em as f32;

        let mut origin = font.origin(glyph).unwrap();

        let bounds = font.raster_bounds(
            glyph,
            glyph_key.size.as_f32_pts(),
            &origin, 
            HintingOptions::None, // TODO:
            RasterizationOptions::GrayscaleAa // TODO:
        ).unwrap();

        // move alloc out of get_glyph function?
        let mut canvas = Canvas::new(&bounds.size.ceil().cast::<u32>(), Format::Rgb24);

        if !glyph_key.c.is_whitespace() {
            font.rasterize_glyph(
                &mut canvas,
                glyph,
                glyph_key.size.as_f32_pts(),
                &origin,
                HintingOptions::None, // TODO:
                RasterizationOptions::GrayscaleAa // TODO:
            ).unwrap();
            // FIXME: Error handling
        }
        let tbounds = font.typographic_bounds(glyph).unwrap();

        let glyph = RasterizedGlyph {
            c: glyph_key.c,
            width: bounds.size.width,
            height: bounds.size.height,
            top: -(tbounds.origin.y * scale).round() as i32, // FIXME: TARGET VALUE
            left: bounds.origin.x,
            buf: canvas.pixels,
        };
        println!("{:#?}", glyph);
        Ok(glyph)
    }
}