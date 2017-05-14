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
use core_foundation::base::TCFType;
use core_graphics::base::CGFloat;
use core_graphics::context::{CGContext, CGContextRef};
use core_graphics::font::{CGFont, CGFontRef, CGGlyph};
use core_graphics::geometry::{CGPoint, CGRect};

use libc::{size_t, c_int};

use darwin::cg_color::{CGColorRef, CGColor};



/// Additional methods needed to render fonts for Alacritty
///
/// TODO upstream these into core_graphics crate
pub trait CGContextExt {
    fn set_allows_font_subpixel_quantization(&self, bool);
    fn set_should_subpixel_quantize_fonts(&self, bool);
    fn set_allows_font_subpixel_positioning(&self, bool);
    fn set_should_subpixel_position_fonts(&self, bool);
    fn set_allows_antialiasing(&self, bool);
    fn set_should_antialias(&self, bool);
    fn fill_rect(&self, rect: CGRect);
    fn set_font_smoothing_background_color(&self, color: CGColor);
    fn show_glyphs_at_positions(&self, &[CGGlyph], &[CGPoint]);
    fn set_font(&self, &CGFont);
    fn set_font_size(&self, size: f64);
    fn set_font_smoothing_style(&self, style: i32);
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

    fn set_should_subpixel_position_fonts(&self, should: bool) {
        unsafe {
            CGContextSetShouldSubpixelPositionFonts(self.as_concrete_TypeRef(), should);
        }
    }

    fn set_allows_font_subpixel_positioning(&self, allows: bool) {
        unsafe {
            CGContextSetAllowsFontSubpixelPositioning(self.as_concrete_TypeRef(), allows);
        }
    }

    fn set_should_antialias(&self, should: bool) {
        unsafe {
            CGContextSetShouldAntialias(self.as_concrete_TypeRef(), should);
        }
    }

    fn set_allows_antialiasing(&self, allows: bool) {
        unsafe {
            CGContextSetAllowsAntialiasing(self.as_concrete_TypeRef(), allows);
        }
    }

    fn fill_rect(&self, rect: CGRect) {
        unsafe {
            CGContextFillRect(self.as_concrete_TypeRef(), rect);
        }
    }

    fn set_font_smoothing_background_color(&self, color: CGColor) {
        unsafe {
            CGContextSetFontSmoothingBackgroundColor(self.as_concrete_TypeRef(),
                                                     color.as_concrete_TypeRef());
        }
    }

    fn show_glyphs_at_positions(&self, glyphs: &[CGGlyph], positions: &[CGPoint]) {
        assert_eq!(glyphs.len(), positions.len());
        unsafe {
            CGContextShowGlyphsAtPositions(self.as_concrete_TypeRef(),
                                           glyphs.as_ptr(),
                                           positions.as_ptr(),
                                           glyphs.len());
        }
    }

    fn set_font(&self, font: &CGFont) {
        unsafe {
            CGContextSetFont(self.as_concrete_TypeRef(), font.as_concrete_TypeRef());
        }
    }

    fn set_font_size(&self, size: f64) {
        unsafe {
            CGContextSetFontSize(self.as_concrete_TypeRef(), size as CGFloat);
        }
    }

    fn set_font_smoothing_style(&self, style: i32) {
        unsafe {
            CGContextSetFontSmoothingStyle(self.as_concrete_TypeRef(), style as _);
        }
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
extern {
    fn CGContextSetAllowsFontSubpixelQuantization(c: CGContextRef, allows: bool);
    fn CGContextSetShouldSubpixelQuantizeFonts(c: CGContextRef, should: bool);
    fn CGContextSetAllowsFontSubpixelPositioning(c: CGContextRef, allows: bool);
    fn CGContextSetShouldSubpixelPositionFonts(c: CGContextRef, should: bool);
    fn CGContextSetAllowsAntialiasing(c: CGContextRef, allows: bool);
    fn CGContextSetShouldAntialias(c: CGContextRef, should: bool);
    fn CGContextFillRect(c: CGContextRef, r: CGRect);
    fn CGContextSetFontSmoothingBackgroundColor(c: CGContextRef, color: CGColorRef);
    fn CGContextShowGlyphsAtPositions(c: CGContextRef, glyphs: *const CGGlyph,
                                      positions: *const CGPoint, count: size_t);
    fn CGContextSetFont(c: CGContextRef, font: CGFontRef);
    fn CGContextSetFontSize(c: CGContextRef, size: CGFloat);
    fn CGContextSetFontSmoothingStyle(c: CGContextRef, style: c_int);
}
