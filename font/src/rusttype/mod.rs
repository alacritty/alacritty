extern crate font_loader;
use self::font_loader::system_fonts;

extern crate rusttype;
use self::rusttype::{point, Codepoint, FontCollection, Scale};

use super::{FontDesc, FontKey, GlyphKey, Metrics, RasterizedGlyph, Size, Slant, Style, Weight};

pub struct RustTypeRasterizer {
    fonts: Vec<rusttype::Font<'static>>,
    dpi_ratio: f32,
}

impl crate::Rasterize for RustTypeRasterizer {
    type Err = Error;

    fn new(device_pixel_ratio: f32, _: bool) -> Result<RustTypeRasterizer, Error> {
        Ok(RustTypeRasterizer {
            fonts: Vec::new(),
            dpi_ratio: device_pixel_ratio,
        })
    }

    fn metrics(&self, key: FontKey, size: Size) -> Result<Metrics, Error> {
        let scale = Scale::uniform(size.as_f32_pts() * self.dpi_ratio * 96. / 72.);
        let vmetrics = self.fonts[key.token as usize].v_metrics(scale);
        let hmetrics = self.fonts[key.token as usize]
            .glyph(
                // If the font is monospaced all glyphs *should* have the same width
                // 33 '!' is the first displaying character
                Codepoint(33),
            )
            .ok_or(Error::MissingGlyph)?
            .scaled(scale)
            .h_metrics();

        let line_height = f64::from(vmetrics.ascent - vmetrics.descent + vmetrics.line_gap);
        let average_advance = f64::from(hmetrics.advance_width);
        let descent = vmetrics.descent;

        // Strikeout and underline metrics.
        // RustType doesn't support these, so we make up our own.
        let thickness = (descent / 5.).round();
        let underline_position = descent / 2.;
        let strikeout_position = line_height as f32 / 2. - descent;

        Ok(Metrics {
            descent,
            average_advance,
            line_height,
            underline_position,
            underline_thickness: thickness,
            strikeout_position,
            strikeout_thickness: thickness,
        })
    }

    fn load_font(&mut self, desc: &FontDesc, _size: Size) -> Result<FontKey, Error> {
        let fp = system_fonts::FontPropertyBuilder::new()
            .family(&desc.name)
            .monospace();

        let fp = match desc.style {
            Style::Specific(_) => unimplemented!(""),
            Style::Description { slant, weight } => {
                let fp = match slant {
                    Slant::Normal => fp,
                    Slant::Italic => fp.italic(),
                    // This style is not supported by rust-font-loader
                    Slant::Oblique => return Err(Error::UnsupportedStyle),
                };
                match weight {
                    Weight::Bold => fp.bold(),
                    Weight::Normal => fp,
                }
            }
        };
        self.fonts.push(FontCollection::from_bytes(
            system_fonts::get(&fp.build())
                .ok_or_else(|| Error::MissingFont(desc.clone()))?
                .0,
        ).into_font()
            .ok_or(Error::UnsupportedFont)?);
        Ok(FontKey {
            token: (self.fonts.len() - 1) as u16,
        })
    }

    fn get_glyph(&mut self, glyph_key: GlyphKey) -> Result<RasterizedGlyph, Error> {
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

        let scaled_glyph = self.fonts[glyph_key.font_key.token as usize]
            .glyph(glyph_key.c)
            .ok_or(Error::MissingGlyph)?
            .scaled(Scale::uniform(
                glyph_key.size.as_f32_pts() * self.dpi_ratio * 96. / 72.,
            ));

        let glyph = scaled_glyph.positioned(point(0.0, 0.0));

        // Pixel bounding box
        let bb = match glyph.pixel_bounding_box() {
            Some(bb) => bb,
            // Bounding box calculation fails for spaces so we provide a placeholder bounding box
            None => rusttype::Rect {
                min: point(0, 0),
                max: point(0, 0),
            },
        };

        let mut buf = Vec::with_capacity((bb.width() * bb.height()) as usize);

        glyph.draw(|_x, _y, v| {
            buf.push((v * 255.0) as u8);
            buf.push((v * 255.0) as u8);
            buf.push((v * 255.0) as u8);
        });
        Ok(RasterizedGlyph {
            c: glyph_key.c,
            width: bb.width(),
            height: bb.height(),
            top: -bb.min.y,
            left: bb.min.x,
            buf,
        })
    }

    fn update_dpr(&mut self, device_pixel_ratio: f32) {
        self.dpi_ratio = device_pixel_ratio;
    }
}

#[derive(Debug)]
pub enum Error {
    MissingFont(FontDesc),
    UnsupportedFont,
    UnsupportedStyle,
    // NOTE: This error is different from how the FreeType code handles it
    MissingGlyph,
}

impl ::std::error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::MissingFont(ref _desc) => "Couldn't find the requested font",
            Error::UnsupportedFont => "Only TrueType fonts are supported",
            Error::UnsupportedStyle => "The selected style is not supported by rusttype",
            Error::MissingGlyph => "The selected font did not have the requested glyph",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::MissingFont(ref desc) => write!(
                f,
                "Couldn't find a font with `{}`\
                 \n\tPlease check the font config in your alacritty.yml.",
                desc
            ),
            Error::UnsupportedFont => write!(
                f,
                "Rusttype only supports TrueType fonts.\n\tPlease select a TrueType font instead."
            ),
            Error::UnsupportedStyle => {
                write!(f, "The selected font style is not supported by rusttype.")
            }
            Error::MissingGlyph => write!(f, "The selected font did not have the requested glyph."),
        }
    }
}
