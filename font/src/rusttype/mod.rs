extern crate font_loader;
use self::font_loader::system_fonts;

extern crate rusttype;
use self::rusttype::{Scale, FontCollection, Font, point};

use super::{FontDesc, RasterizedGlyph, Metrics, Size, FontKey, GlyphKey, Weight, Slant, Style};

pub struct RustTypeRasterizer {
    fonts: Vec<rusttype::Font<'static>>,
    dpi: f32
}

impl ::Rasterize for RustTypeRasterizer {
    type Err = Error;

    fn new(device_pixel_ratio: f32, _: bool) -> Result<RustTypeRasterizer, Error> {
        Ok(RustTypeRasterizer{
            fonts: Vec::new(),
            dpi: device_pixel_ratio
        })
    }

    fn metrics(&self, key: FontKey) -> Result<Metrics, Error> {
        // Change scale to respect dpi
        let metrics = self.fonts[key.token as usize].v_metrics(Scale::uniform(1.));
        Ok(Metrics{
            descent: metrics.descent,
            // TODO
            average_advance: 0.,
            line_height: 0.
        })
    }

    fn load_font(&mut self, desc: &FontDesc, size: Size) -> Result<FontKey, Error> {
        let fp = system_fonts::FontPropertyBuilder::new()
            .family(&desc.name)
            .monospace();

        let fp = match desc.style {
            Style::Specific(_) => unimplemented!(""),
            Style::Description{slant, weight} => {
                let fp = match slant {
                    Slant::Normal => fp,
                    Slant::Italic => fp.italic(),
                    // This is not supported by rust-font-loader
                    Slant::Oblique => return Err(Error::UnsupportedStyle)
                };
                match weight {
                    Weight::Bold => fp.bold(),
                    Weight::Normal => fp
                }
            }
        };
        self.fonts.push(FontCollection::from_bytes(
            // TODO Clone is maybe not the best way
            system_fonts::get(&fp.build()).ok_or(Error::MissingFont(desc.clone()))?.0
        ).into_font().ok_or(Error::UnsupportedFont)?);
        Ok(FontKey{token: (self.fonts.len() - 1) as u16})
    }

    fn get_glyph(&mut self, glyph_key: &GlyphKey) -> Result<RasterizedGlyph, Error> {
        let glyph = self.fonts[glyph_key.font_key.token as usize].glyph(glyph_key.c).ok_or(Error::MissingGlyph)?
            // Scaling from: http://docs.piston.rs/conrod/src/conrod/text.rs.html#76-78
            .scaled(Scale::uniform((glyph_key.size.as_f32_pts() * 4.) as f32 / 3.0))
            .positioned(point(0.,0.));

        let bb = glyph.pixel_bounding_box().unwrap(); // I'm not sure why this is failable
        let (width, height) = (bb.max.x - bb.min.x, bb.max.y - bb.min.y);
        let mut buf = Vec::with_capacity((height * width) as usize);

        glyph.draw(|x,y,v| {
            buf.push(((v * 255.0) + 0.5).floor().max(0.0).min(255.0) as u8);
        });
        Ok(RasterizedGlyph{
            c: glyph_key.c,
            width: width,
            height: height,
            top: bb.min.y,
            left: bb.min.x,
            buf: buf
        })
    }
}

#[derive(Debug)]
pub enum Error {
    MissingFont(FontDesc),
    UnsupportedFont,
    UnsupportedStyle,
    // This error is different from how the FreeType code handles it
    MissingGlyph
}

impl ::std::error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::MissingFont(ref _desc) => "couldn't find the requested font",
            Error::UnsupportedFont => "only TrueType fonts are supported",
            Error::UnsupportedStyle => "the selected style is not supported by rusttype",
            Error::MissingGlyph => "the selected font did not have requested glyph",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        // TODO Improve error messages
        match *self {
            Error::MissingFont(ref desc) => {
                write!(f, "Couldn't find a font with {}\
                       \n\tPlease check the font config in your alacritty.yml.", desc)
            },
            Error::UnsupportedFont => {
                write!(f, "Rusttype only supports TrueType fonts.\n\tPlease select a TrueType font instead.")
            },
            Error::UnsupportedStyle => {
                write!(f, "The selected font style is not supported by rusttype.")
            },
            Error::MissingGlyph => {
                write!(f, "The selected font did not have the requested glyph.")
            }
        }
    }
}