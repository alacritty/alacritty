use std::error;
use std::fmt::{self, Display, Formatter};

use super::{FontDesc, FontKey, GlyphKey, Metrics, RasterizedGlyph, Size, Slant, Style, Weight};

// TODO: Move into parent crate
extern crate font_kit;

use self::font_kit::source::{SystemSource, Source};
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
            Normal => properties::Weight::NORMAL,
            Bold => properties::Weight::BOLD,
        }
    }
}

impl Into<properties::Style> for Slant {
    fn into(self) -> properties::Style {
        match self {
            Normal => properties::Style::Normal,
            Italic => properties::Style::Italic,
            Oblique => properties::Style::Oblique
        }
    }
}

impl ::Rasterize for FontKitRasterizer {
    type Err = Error;

    #[cfg(windows)]
    fn new(device_pixel_ratio: f32, use_thin_strokes: bool) -> Result<Self, Self::Err> {
        Ok(
            Self{
                source: SystemSource::new(),
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

        let metrics = Metrics{
            // If the font is monospaced all glyphs *should* have the same width
            // 33 '!' is the first displaying character
            // FIXME: Error handling
            average_advance: (Into::<f32>::into(font.advance(33).unwrap().x) / size.as_f32_pts()) as f64,
            line_height: (Into::<f32>::into(metrics.line_gap + metrics.descent + metrics.ascent) / size.as_f32_pts()) as f64,
            descent: metrics.descent / size.as_f32_pts()
        };
        println!("{:#?}", metrics);
        Ok(metrics)
    }

    fn load_font(&mut self, desc: &FontDesc, size: Size) -> Result<FontKey, Self::Err> {
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

        // TODO: Ideally this should be provided by the renderer
        let origin = font.origin(glyph_key.c as u32).unwrap();

        let bounds = font.raster_bounds(
            glyph_key.c as u32, 
            glyph_key.size.as_f32_pts(),
            &origin, 
            HintingOptions::None, 
            RasterizationOptions::GrayscaleAa
        ).unwrap();

        // TODO: Investigate subpixel rendering (coloured)
        // move alloc out of get_glyph function?
        let mut canvas = Canvas::new(&bounds.size.ceil().cast::<u32>(), Format::A8);

        font.rasterize_glyph(
            &mut canvas,
            glyph_key.c as u32,
            glyph_key.size.as_f32_pts(),
            &origin,
            HintingOptions::None, // TODO:
            RasterizationOptions::GrayscaleAa
        ).unwrap();
        // FIXME: Error handling

        let glyph = RasterizedGlyph {
            c: glyph_key.c,
            width: bounds.size.width,
            height: bounds.size.height,
            top: bounds.origin.y,
            left: bounds.origin.x,
            buf: canvas.pixels,
        };
        println!("{:#?}", glyph);
        Ok(glyph)
    }
}