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
//! Rasterization powered by CoreText

#![allow(improper_ctypes)]

mod cg_color;
mod byte_order;
mod cg_ext;

use std::{ptr};

use core_foundation::string::{CFString};
use core_foundation::base::TCFType;
use core_foundation::array::{CFIndex, CFArray};
use core_graphics::context::{CGContext};
use core_graphics::color_space::CGColorSpace;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_graphics::font::{CGGlyph};
use core_graphics::base::kCGImageAlphaPremultipliedFirst;
use core_text::font::{CTFont, new_from_descriptor as ct_new_from_descriptor, cascade_list_for_languages as ct_cascade_list_for_languages};
use core_text::font_descriptor::{kCTFontDefaultOrientation, kCTFontHorizontalOrientation, kCTFontVerticalOrientation, CTFontOrientation, CTFontDescriptor, CTFontDescriptorRef, SymbolicTraitAccessors};
use core_text::font_collection::{create_for_family};

use darwin::byte_order::kCGBitmapByteOrder32Host;
use darwin::byte_order::extract_rgb;

use euclid::point::Point2D;
use euclid::rect::Rect;
use euclid::size::Size2D;

use self::cg_ext::*;

use super::*;



/// Specifies the intended rendering orientation of the font for obtaining glyph metrics
#[derive(Debug)]
pub enum FontOrientation {
    Default = kCTFontDefaultOrientation as isize,
    Horizontal = kCTFontHorizontalOrientation as isize,
    Vertical = kCTFontVerticalOrientation as isize,
}

impl Default for FontOrientation {
    fn default() -> FontOrientation {
        FontOrientation::Default
    }
}


#[derive(Debug, Clone)]
pub struct Font {
    ct_font: CTFont,
}

unsafe impl Send for Font {}

impl Font {
    /// The the bounding rect of a glyph
    pub fn bounding_rect_for_glyph(
        &self,
        orientation: FontOrientation,
        index: u32
    ) -> Rect<f64> {
        let cg_rect = self.ct_font.get_bounding_rects_for_glyphs(
            orientation as CTFontOrientation,
            &[index as CGGlyph]
        );

        Rect::new(
            Point2D::new(cg_rect.origin.x, cg_rect.origin.y),
            Size2D::new(cg_rect.size.width, cg_rect.size.height),
        )
    }

    pub fn metrics(&self) -> Metrics {
        let average_advance = self.glyph_advance('0');

        let ascent = self.ct_font.ascent() as f64;
        let descent = self.ct_font.descent() as f64;
        let leading = self.ct_font.leading() as f64;
        let line_height = (ascent + descent + leading + 0.5).floor();

        Metrics {
            average_advance: average_advance,
            line_height: line_height,
            descent: -(self.ct_font.descent() as f32),
        }
    }

    pub fn is_bold(&self) -> bool {
        self.ct_font.symbolic_traits().is_bold()
    }

    pub fn is_italic(&self) -> bool {
        self.ct_font.symbolic_traits().is_italic()
    }

    fn glyph_advance(&self, character: char) -> f64 {
        let index = self.glyph_index(character).unwrap();

        let indices = [index as CGGlyph];

        self.ct_font.get_advances_for_glyphs(
            FontOrientation::Default as _,
            &indices[0],
            ptr::null_mut(),
            1
        )
    }

    pub fn get_glyph(
        &self, character:char,
        encoded:&[u16],
        _size: f64,
        use_thin_strokes: bool
    ) -> Result<RasterizedGlyph, Error> {
        let glyph_index = self.glyph_index_utf16(encoded)
            .ok_or(Error::MissingGlyph(character))?;

        let bounds = self.bounding_rect_for_glyph(Default::default(), glyph_index);

        let rasterized_left = bounds.origin.x.floor() as i32;
        let rasterized_width =
            (bounds.origin.x - (rasterized_left as f64) + bounds.size.width).ceil() as u32;
        let rasterized_descent = (-bounds.origin.y).ceil() as i32;
        let rasterized_ascent = (bounds.size.height + bounds.origin.y).ceil() as i32;
        let rasterized_height = (rasterized_descent + rasterized_ascent) as u32;

        if rasterized_width == 0 || rasterized_height == 0 {
            return Ok(RasterizedGlyph {
                c: ' ',
                width: 0,
                height: 0,
                top: 0,
                left: 0,
                buf: Vec::new()
            });
        }

        let mut cg_context = CGContext::create_bitmap_context(
            rasterized_width as usize,
            rasterized_height as usize,
            8, // bits per component
            rasterized_width as usize * 4,
            &CGColorSpace::create_device_rgb(),
            kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Host
        );

        // Give the context an opaque, black background
        cg_context.set_rgb_fill_color(0.0, 0.0, 0.0, 1.0);
        let context_rect = CGRect::new(
            &CGPoint::new(0.0, 0.0),
            &CGSize::new(
                rasterized_width as f64,
                rasterized_height as f64
            )
        );

        cg_context.fill_rect(context_rect);

        if use_thin_strokes {
            cg_context.set_font_smoothing_style(16);
        }

        cg_context.set_allows_font_smoothing(true);
        cg_context.set_should_smooth_fonts(true);
        cg_context.set_allows_font_subpixel_quantization(true);
        cg_context.set_should_subpixel_quantize_fonts(true);
        cg_context.set_allows_font_subpixel_positioning(true);
        cg_context.set_should_subpixel_position_fonts(true);
        cg_context.set_allows_antialiasing(true);
        cg_context.set_should_antialias(true);

        // Set fill color to white for drawing the glyph
        cg_context.set_rgb_fill_color(1.0, 1.0, 1.0, 1.0);
        let rasterization_origin = CGPoint {
            x: -rasterized_left as f64,
            y: rasterized_descent as f64,
        };

        self.ct_font.draw_glyphs(&[glyph_index as CGGlyph],
                                 &[rasterization_origin],
                                 cg_context.clone());

        let rasterized_pixels = cg_context.data().to_vec();

        let buf = extract_rgb(rasterized_pixels);

        Ok(RasterizedGlyph {
            c: character,
            left: rasterized_left,
            top: (bounds.size.height + bounds.origin.y).ceil() as i32,
            width: rasterized_width as i32,
            height: rasterized_height as i32,
            buf: buf,
        })
    }

    fn glyph_index(&self, character: char) -> Option<u32> {
        // encode this char as utf-16
        let mut buf = [0; 2];
        let encoded:&[u16] = character.encode_utf16(&mut buf);
        // and use the utf-16 buffer to get the index
        self.glyph_index_utf16(encoded)
    }
    fn glyph_index_utf16(&self, encoded: &[u16]) -> Option<u32> {

        // output buffer for the glyph. for non-BMP glyphs, like
        // emojis, this will be filled with two chars the second
        // always being a 0.
        let mut glyphs:[CGGlyph; 2] = [0; 2];

        let res = self.ct_font.get_glyphs_for_characters(
            encoded.as_ptr(),
            glyphs.as_mut_ptr(),
            encoded.len() as CFIndex
        );

        if res {
            Some(glyphs[0] as u32)
        } else {
            None
        }
    }
}


/// Exposed implementation of `RasterizeImpl`.
pub struct RasterizerImpl {
}

impl RasterizerImpl {

    fn get_specific_face(
        &self,
        desc: &FontDesc,
        style: &str,
        size: Size,
        device_pixel_ratio: f32,
    ) -> Result<Font, Error> {
        let descriptors = descriptors_for_family(&desc.name[..]);
        for descriptor in descriptors {
            if descriptor.style_name == style {
                // Found the font we want
                let scaled_size = size.as_f32_pts() as f64 * device_pixel_ratio as f64;
                let font = descriptor.to_font(scaled_size);
                return Ok(font);
            }
        }

        Err(Error::MissingFont(desc.to_owned()))
    }

    fn get_matching_face(
        &self,
        desc: &FontDesc,
        slant: Slant,
        weight: Weight,
        size: Size,
        device_pixel_ratio: f32,
    ) -> Result<Font, Error> {
        let bold = match weight {
            Weight::Bold => true,
            _ => false
        };
        let italic = match slant {
            Slant::Normal => false,
            _ => true,
        };
        let scaled_size = size.as_f32_pts() as f64 * device_pixel_ratio as f64;

        let descriptors = descriptors_for_family(&desc.name[..]);
        for descriptor in descriptors {
            let font = descriptor.to_font(scaled_size);
            if font.is_bold() == bold && font.is_italic() == italic {
                // Found the font we want
                return Ok(font);
            }
        }

        Err(Error::MissingFont(desc.to_owned()))
    }

}


#[allow(unused_variables)]
impl RasterizeImpl for RasterizerImpl {

    type Err = Error;

    fn new() -> Result<RasterizerImpl,Self::Err> {
        Ok(RasterizerImpl {})
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
        match desc.style {
            Style::Specific(ref style) =>
                self.get_specific_face(desc, style, size, details.device_pixel_ratio),
            Style::Description { slant, weight } =>
                self.get_matching_face(desc, slant, weight, size, details.device_pixel_ratio),
        }
    }

    /// Get the fallback list of `Font` for given `FontDesc` `Font` and `Size`.
    /// This is used on macOS.
    fn get_fallback_fonts(
        &self,
        details: &Details,
        desc: &FontDesc,
        font: &Font,
        size: Size,
        loaded_names: &Vec<String>,
    ) -> Result<Vec<Font>, Self::Err> {
        Ok({
            // XXX FIXME, hardcoded language
            let lang = vec!["en".to_owned()];

            let scaled_size = size.as_f32_pts() as f64 * details.device_pixel_ratio as f64;

            // the system lists contains (at least) two strange fonts:
            // .Apple Symbol Fallback
            // .Noto Sans Universal
            // both have a .-prefix (to indicate they are internal?)
            // neither work very well. the latter even breaks things because
            // it defines code points with just [?] glyphs.
            cascade_list_for_languages(font, &lang).into_iter()
                .filter(|x| x.font_path != "")
                .filter(|x| !loaded_names.contains(&x.family_name))
                .map(|x| x.to_font(scaled_size))
                .collect()
        })
    }

    /// Get `Metrics` for the given `Font`
    fn metrics(
        &self,
        details: &Details,
        font: &Font,
    ) -> Result<Metrics, Self::Err> {
        Ok(font.metrics())
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
        let scaled_size = details.device_pixel_ratio * size.as_f32_pts();
        font.get_glyph(c, encoded, scaled_size as f64, details.use_thin_strokes)
    }

    fn get_glyph_fallback(
        &mut self,
        details: &Details,
        c: char,
        encoded: &[u16], // utf16 encoded char
        size: Size,
    ) -> Result<RasterizedGlyph, Self::Err> {
        // this is not used for macOS since the fallback chain has already
        // been added on to the fonts tested in get_glyph.
        Err(Error::MissingGlyph(c))
    }

}


/// Font descriptor
///
/// The descriptor provides data about a font and supports creating a font.
#[derive(Debug)]
pub struct Descriptor {
    family_name: String,
    font_name: String,
    style_name: String,
    display_name: String,
    font_path: String,

    ct_descriptor: CTFontDescriptor
}


impl Descriptor {
    /// Create a Descriptor from a CTFontDescriptor
    pub fn new(desc: CTFontDescriptor) -> Descriptor {
        Descriptor {
            family_name: desc.family_name(),
            font_name: desc.font_name(),
            style_name: desc.style_name(),
            display_name: desc.display_name(),
            font_path: desc.font_path().unwrap_or_else(|| "".to_owned()),
            ct_descriptor: desc,
        }
    }

    /// Create a Font from this descriptor
    pub fn to_font(&self, size: f64) -> Font {
        let ct_font = ct_new_from_descriptor(&self.ct_descriptor, size);
        Font {
            ct_font: ct_font,
        }
    }
}


/// Return fallback descriptors for font/language list
fn cascade_list_for_languages(
    font: &Font,
    languages: &Vec<String>
) -> Vec<Descriptor> {

    let lang:Vec<CFString> = languages.clone().into_iter()
        .map(|x| CFString::new(&x))
        .collect();
    let langarr = CFArray::from_CFTypes(&lang);

    let list = ct_cascade_list_for_languages(&font.ct_font, &langarr);

    // CFArray of CTFontDescriptorRef (again)
    list.into_iter()
        .map(|x| {
            let desc: CTFontDescriptor = unsafe {
                TCFType::wrap_under_get_rule(x as CTFontDescriptorRef)
            };
            Descriptor::new(desc)
        })
        .collect()
}


/// Get descriptors for family name
pub fn descriptors_for_family(family: &str) -> Vec<Descriptor> {

    let ct_collection = match create_for_family(family) {
        Some(c) => c,
        None => return vec![],
    };

    // CFArray of CTFontDescriptorRef (i think)
    let descriptors = ct_collection.get_descriptors();
    descriptors.into_iter()
        .map(|x| {
            let desc: CTFontDescriptor = unsafe {
                TCFType::wrap_under_get_rule(x as CTFontDescriptorRef)
            };
            Descriptor::new(desc)
        })
        .collect()
}


/// Errors occurring when using the core text rasterizer
#[derive(Debug)]
pub enum Error {
    /// Tried to rasterize a glyph but it was not available
    MissingGlyph(char),

    /// Couldn't find font matching description
    MissingFont(FontDesc),

    /// Requested an operation with a FontKey that isn't known to the rasterizer
    FontNotLoaded,

    /// Loading a `FontDescList` that resulted in no loaded fonts.
    NoFontsForList,
}

impl ::std::error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::MissingGlyph(ref _c) => "couldn't find the requested glyph",
            Error::MissingFont(ref _desc) => "couldn't find the requested font",
            Error::FontNotLoaded => "tried to operate on font that hasn't been loaded",
            Error::NoFontsForList => "provided font list didn't result in any loaded font",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::MissingGlyph(ref c) => {
                write!(f, "Glyph not found for char {:?}", c)
            },
            Error::MissingFont(ref desc) => {
                write!(f, "Couldn't find a font with {}\
                       \n\tPlease check the font config in your alacritty.yml.", desc)
            },
            Error::FontNotLoaded => {
                f.write_str("Tried to use a font that hasn't been loaded")
            }
            Error::NoFontsForList => {
                f.write_str("Provided font list didn't result in any loaded font")
            }
        }
    }
}
