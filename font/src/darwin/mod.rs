//! Font rendering based on CoreText
//!
//! TODO error handling... just search for unwrap.
use std::collections::HashMap;
use std::ops::Deref;
use std::ptr;

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::array::CFIndex;
use core_foundation_sys::string::UniChar;
use core_graphics::base::kCGImageAlphaNoneSkipFirst;
use core_graphics::base::kCGImageAlphaPremultipliedLast;
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::{CGContext, CGContextRef};
use core_graphics::font::CGGlyph;
use core_graphics::geometry::CGPoint;
use core_text::font::{CTFont, new_from_descriptor as ct_new_from_descriptor};
use core_text::font_collection::create_for_family;
use core_text::font_collection::get_family_names as ct_get_family_names;
use core_text::font_descriptor::kCTFontDefaultOrientation;
use core_text::font_descriptor::kCTFontHorizontalOrientation;
use core_text::font_descriptor::kCTFontVerticalOrientation;
use core_text::font_descriptor::{CTFontDescriptor, CTFontDescriptorRef, CTFontOrientation};

use euclid::point::Point2D;
use euclid::rect::Rect;
use euclid::size::Size2D;

use super::{FontDesc, RasterizedGlyph, Metrics};

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

/// Rasterizer, the main type exported by this package
///
/// Given a fontdesc, can rasterize fonts.
pub struct Rasterizer {
    fonts: HashMap<FontDesc, Font>,
    device_pixel_ratio: f32,
}

impl Rasterizer {
    pub fn new(dpi_x: f32, dpi_y: f32, device_pixel_ratio: f32) -> Rasterizer {
        println!("device_pixel_ratio: {}", device_pixel_ratio);
        Rasterizer {
            fonts: HashMap::new(),
            device_pixel_ratio: device_pixel_ratio,
        }
    }

    pub fn metrics(&mut self, desc: &FontDesc, size: f32) -> Metrics {
        let scaled_size = self.device_pixel_ratio * size;
        self.get_font(desc, scaled_size).unwrap().metrics()
    }

    fn get_font(&mut self, desc: &FontDesc, size: f32) -> Option<Font> {
        if let Some(font) = self.fonts.get(desc) {
            return Some(font.clone());
        }

        let descriptors = descriptors_for_family(&desc.name[..]);
        for descriptor in descriptors {
            if descriptor.style_name == desc.style {
                // Found the font we want
                let font = descriptor.to_font(size as _);
                self.fonts.insert(desc.to_owned(), font.clone());
                return Some(font);
            }
        }

        None
    }

    pub fn get_glyph(&mut self, desc: &FontDesc, size: f32, c: char) -> RasterizedGlyph {
        let scaled_size = self.device_pixel_ratio * size;
        let glyph = self.get_font(desc, scaled_size).unwrap().get_glyph(c, scaled_size as _);

        glyph
    }
}

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

/// A font
#[derive(Debug, Clone)]
pub struct Font {
    ct_font: CTFont
}

unsafe impl Send for Font {}

/// List all family names
pub fn get_family_names() -> Vec<String> {
    // CFArray of CFStringRef
    let names = ct_get_family_names();
    let mut owned_names = Vec::new();

    for name in names.iter() {
        let family: CFString = unsafe { TCFType::wrap_under_get_rule(name as CFStringRef) };
        owned_names.push(format!("{}", family));
    }

    owned_names
}

/// Get descriptors for family name
pub fn descriptors_for_family(family: &str) -> Vec<Descriptor> {
    let mut out = Vec::new();

    let ct_collection = match create_for_family(family) {
        Some(c) => c,
        None => return out,
    };

    // CFArray of CTFontDescriptorRef (i think)
    let descriptors = ct_collection.get_descriptors();
    for descriptor in descriptors.iter() {
        let desc: CTFontDescriptor = unsafe {
            TCFType::wrap_under_get_rule(descriptor as CTFontDescriptorRef)
        };
        out.push(Descriptor {
            family_name: desc.family_name(),
            font_name: desc.font_name(),
            style_name: desc.style_name(),
            display_name: desc.display_name(),
            font_path: desc.font_path(),
            ct_descriptor: desc,
        });
    }

    out
}

impl Descriptor {
    /// Create a Font from this descriptor
    pub fn to_font(&self, pt_size: f64) -> Font {
        let ct_font = ct_new_from_descriptor(&self.ct_descriptor, pt_size);
        Font {
            ct_font: ct_font
        }
    }
}

impl Deref for Font {
    type Target = CTFont;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.ct_font
    }
}

impl Font {
    /// The the bounding rect of a glyph
    pub fn bounding_rect_for_glyph(&self, orientation: FontOrientation, index: u32) -> Rect<f64> {
        let cg_rect = self.ct_font.get_bounding_rects_for_glyphs(orientation as CTFontOrientation,
                                                                 &[index as CGGlyph]);

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
        }
    }

    fn glyph_advance(&self, character: char) -> f64 {
        let index = self.glyph_index(character).unwrap();

        let indices = [index as CGGlyph];

        self.ct_font.get_advances_for_glyphs(FontOrientation::Default as _,
                                             &indices[0],
                                             ptr::null_mut(),
                                             1)
    }

    pub fn get_glyph(&self, character: char, size: f64) -> RasterizedGlyph {
        let glyph_index = match self.glyph_index(character) {
            Some(i) => i,
            None => {
                // TODO refactor this
                return RasterizedGlyph {
                    c: ' ',
                    width: 0,
                    height: 0,
                    top: 0,
                    left: 0,
                    buf: Vec::new()
                };
            }
        };

        let bounds = self.bounding_rect_for_glyph(Default::default(), glyph_index);

        let rasterized_left = bounds.origin.x.floor() as i32;
        let rasterized_width =
            (bounds.origin.x - (rasterized_left as f64) + bounds.size.width).ceil() as u32;
        let rasterized_descent = (-bounds.origin.y).ceil() as i32;
        let rasterized_ascent = (bounds.size.height + bounds.origin.y).ceil() as i32;
        let rasterized_height = (rasterized_descent + rasterized_ascent) as u32;

        if rasterized_width == 0 || rasterized_height == 0 {
            return RasterizedGlyph {
                c: ' ',
                width: 0,
                height: 0,
                top: 0,
                left: 0,
                buf: Vec::new()
            };
        }

        let mut cg_context = CGContext::create_bitmap_context(rasterized_width as usize,
                                                              rasterized_height as usize,
                                                              8, // bits per component
                                                              rasterized_width as usize * 4,
                                                              &CGColorSpace::create_device_rgb(),
                                                              kCGImageAlphaNoneSkipFirst);

        cg_context.set_allows_font_smoothing(true);
        cg_context.set_should_smooth_fonts(true);
        cg_context.set_allows_font_subpixel_quantization(true);
        cg_context.set_should_subpixel_quantize_fonts(true);
        cg_context.set_rgb_fill_color(1.0, 1.0, 1.0, 1.0);

        let rasterization_origin = CGPoint {
            x: -rasterized_left as f64,
            y: rasterized_descent as f64,
        };

        self.ct_font.draw_glyphs(&[glyph_index as CGGlyph],
                                 &[rasterization_origin],
                                 cg_context.clone());

        let rasterized_area = (rasterized_width * rasterized_height) as usize;
        let rasterized_pixels = cg_context.data().to_vec();
        let buf = rasterized_pixels.into_iter()
                                   .enumerate()
                                   .filter(|&(index, _)| (index % 4) != 0)
                                   .map(|(_, val)| val)
                                   .collect::<Vec<_>>();

        RasterizedGlyph {
            c: character,
            left: rasterized_left,
            top: (bounds.size.height + bounds.origin.y).ceil() as i32,
            width: rasterized_width as i32,
            height: rasterized_height as i32,
            buf: buf,
        }
    }

    fn glyph_index(&self, character: char) -> Option<u32> {
        let chars = [character as UniChar];
        let mut glyphs = [0 as CGGlyph];

        let res = self.ct_font.get_glyphs_for_characters(&chars[0], &mut glyphs[0], 1 as CFIndex);

        if res {
            Some(glyphs[0] as u32)
        } else {
            None
        }
    }
}

/// Additional methods needed to render fonts for Alacritty
///
/// TODO upstream these into core_graphics crate
trait CGContextExt {
    fn set_allows_font_subpixel_quantization(&self, bool);
    fn set_should_subpixel_quantize_fonts(&self, bool);
}

impl CGContextExt for CGContext {
    fn set_allows_font_subpixel_quantization(&self, allows: bool) {
        unsafe {
            CGContextSetAllowsFontSubpixelQuantization(self.as_concrete_TypeRef(), allows);
        }
    }

    fn set_should_subpixel_quantize_fonts(&self, should: bool) {
        unsafe {
            CGContextSetShouldSubpixelQuantizeFonts(self.as_concrete_TypeRef(), should);
        }
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
extern {
    fn CGContextSetAllowsFontSubpixelQuantization(c: CGContextRef, allows: bool);
    fn CGContextSetShouldSubpixelQuantizeFonts(c: CGContextRef, should: bool);
}

#[cfg(test)]
mod tests {
    #[test]
    fn get_family_names() {
        let names = super::get_family_names();
        assert!(names.contains(&String::from("Menlo")));
        assert!(names.contains(&String::from("Monaco")));
    }

    #[test]
    fn get_descriptors_and_build_font() {
        let list = super::descriptors_for_family("Menlo");
        assert!(!list.is_empty());
        println!("{:?}", list);

        // Check to_font
        let fonts = list.iter()
                        .map(|desc| desc.to_font(72.))
                        .collect::<Vec<_>>();

        for font in fonts {
            // Check deref
            println!("family: {}", font.family_name());

            // Get a glyph
            for c in &['a', 'b', 'c', 'd'] {
                let glyph = font.get_glyph(*c, 72.);

                // Debug the glyph.. sigh
                for row in 0..glyph.height {
                    for col in 0..glyph.width {
                        let index = ((glyph.width * 3 * row) + (col * 3)) as usize;
                        let value = glyph.buf[index];
                        let c = match value {
                            0...50 => ' ',
                            51...100 => '.',
                            101...150 => '~',
                            151...200 => '*',
                            201...255 => '#',
                            _ => unreachable!()
                        };
                        print!("{}", c);
                    }
                    print!("\n");
                }
            }
        }
    }
}
