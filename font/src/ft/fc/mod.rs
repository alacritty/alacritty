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
use std::fmt;
use std::ptr;

use foreign_types::{ForeignType, ForeignTypeRef};

use fontconfig::fontconfig as ffi;

use self::ffi::FcResultNoMatch;
use self::ffi::{FcFontList, FcFontMatch, FcFontSort};
use self::ffi::{FcMatchFont, FcMatchPattern, FcMatchScan};
use self::ffi::{FcSetApplication, FcSetSystem};
use self::ffi::{FC_SLANT_ITALIC, FC_SLANT_OBLIQUE, FC_SLANT_ROMAN};
use self::ffi::{FC_WEIGHT_BLACK, FC_WEIGHT_BOLD, FC_WEIGHT_EXTRABLACK, FC_WEIGHT_EXTRABOLD};
use self::ffi::{FC_WEIGHT_BOOK, FC_WEIGHT_MEDIUM, FC_WEIGHT_REGULAR, FC_WEIGHT_SEMIBOLD};
use self::ffi::{FC_WEIGHT_EXTRALIGHT, FC_WEIGHT_LIGHT, FC_WEIGHT_THIN};

pub mod config;
pub use self::config::{Config, ConfigRef};

pub mod font_set;
pub use self::font_set::{FontSet, FontSetRef};

pub mod object_set;
pub use self::object_set::{ObjectSet, ObjectSetRef};

pub mod char_set;
pub use self::char_set::{CharSet, CharSetRef};

pub mod pattern;
pub use self::pattern::{Pattern, PatternRef};

/// Find the font closest matching the provided pattern.
///
/// The returned pattern is the result of Pattern::render_prepare.
pub fn font_match(config: &ConfigRef, pattern: &mut PatternRef) -> Option<Pattern> {
    pattern.config_substitute(config, MatchKind::Pattern);
    pattern.default_substitute();

    unsafe {
        // What is this result actually used for? Seems redundant with
        // return type.
        let mut result = FcResultNoMatch;
        let ptr = FcFontMatch(config.as_ptr(), pattern.as_ptr(), &mut result);

        if ptr.is_null() {
            None
        } else {
            Some(Pattern::from_ptr(ptr))
        }
    }
}

/// list fonts by closeness to the pattern
pub fn font_sort(config: &ConfigRef, pattern: &mut PatternRef) -> Option<FontSet> {
    pattern.config_substitute(config, MatchKind::Pattern);
    pattern.default_substitute();

    unsafe {
        // What is this result actually used for? Seems redundant with
        // return type.
        let mut result = FcResultNoMatch;

        let mut charsets: *mut _ = ptr::null_mut();

        let ptr = FcFontSort(
            config.as_ptr(),
            pattern.as_ptr(),
            0, // false
            &mut charsets,
            &mut result,
        );

        if ptr.is_null() {
            None
        } else {
            Some(FontSet::from_ptr(ptr))
        }
    }
}

/// List fonts matching pattern
pub fn font_list(
    config: &ConfigRef,
    pattern: &mut PatternRef,
    objects: &ObjectSetRef,
) -> Option<FontSet> {
    pattern.config_substitute(config, MatchKind::Pattern);
    pattern.default_substitute();

    unsafe {
        let ptr = FcFontList(config.as_ptr(), pattern.as_ptr(), objects.as_ptr());

        if ptr.is_null() {
            None
        } else {
            Some(FontSet::from_ptr(ptr))
        }
    }
}

/// Available font sets
#[derive(Debug, Copy, Clone)]
pub enum SetName {
    System = FcSetSystem as isize,
    Application = FcSetApplication as isize,
}

/// When matching, how to match
#[derive(Debug, Copy, Clone)]
pub enum MatchKind {
    Font = FcMatchFont as isize,
    Pattern = FcMatchPattern as isize,
    Scan = FcMatchScan as isize,
}

#[derive(Debug, Copy, Clone)]
pub enum Slant {
    Italic = FC_SLANT_ITALIC as isize,
    Oblique = FC_SLANT_OBLIQUE as isize,
    Roman = FC_SLANT_ROMAN as isize,
}

#[derive(Debug, Copy, Clone)]
pub enum Weight {
    Thin = FC_WEIGHT_THIN as isize,
    Extralight = FC_WEIGHT_EXTRALIGHT as isize,
    Light = FC_WEIGHT_LIGHT as isize,
    Book = FC_WEIGHT_BOOK as isize,
    Regular = FC_WEIGHT_REGULAR as isize,
    Medium = FC_WEIGHT_MEDIUM as isize,
    Semibold = FC_WEIGHT_SEMIBOLD as isize,
    Bold = FC_WEIGHT_BOLD as isize,
    Extrabold = FC_WEIGHT_EXTRABOLD as isize,
    Black = FC_WEIGHT_BLACK as isize,
    Extrablack = FC_WEIGHT_EXTRABLACK as isize,
}

#[derive(Debug, Copy, Clone)]
pub enum Width {
    Ultracondensed,
    Extracondensed,
    Condensed,
    Semicondensed,
    Normal,
    Semiexpanded,
    Expanded,
    Extraexpanded,
    Ultraexpanded,
    Other(i32),
}

impl Width {
    fn to_isize(self) -> isize {
        use self::Width::*;
        match self {
            Ultracondensed => 50,
            Extracondensed => 63,
            Condensed => 75,
            Semicondensed => 87,
            Normal => 100,
            Semiexpanded => 113,
            Expanded => 125,
            Extraexpanded => 150,
            Ultraexpanded => 200,
            Other(value) => value as isize,
        }
    }
}

impl From<isize> for Width {
    fn from(value: isize) -> Self {
        match value {
            50 => Width::Ultracondensed,
            63 => Width::Extracondensed,
            75 => Width::Condensed,
            87 => Width::Semicondensed,
            100 => Width::Normal,
            113 => Width::Semiexpanded,
            125 => Width::Expanded,
            150 => Width::Extraexpanded,
            200 => Width::Ultraexpanded,
            _ => Width::Other(value as _),
        }
    }
}

/// Subpixel geometry
pub enum Rgba {
    Unknown,
    Rgb,
    Bgr,
    Vrgb,
    Vbgr,
    None,
}

impl Rgba {
    fn to_isize(&self) -> isize {
        match *self {
            Rgba::Unknown => 0,
            Rgba::Rgb => 1,
            Rgba::Bgr => 2,
            Rgba::Vrgb => 3,
            Rgba::Vbgr => 4,
            Rgba::None => 5,
        }
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Rgba::*;
        f.write_str(match *self {
            Unknown => "unknown",
            Rgb => "rgb",
            Bgr => "bgr",
            Vrgb => "vrgb",
            Vbgr => "vbgr",
            None => "none",
        })
    }
}

impl From<isize> for Rgba {
    fn from(val: isize) -> Rgba {
        match val {
            1 => Rgba::Rgb,
            2 => Rgba::Bgr,
            3 => Rgba::Vrgb,
            4 => Rgba::Vbgr,
            5 => Rgba::None,
            _ => Rgba::Unknown,
        }
    }
}

/// Hinting Style
#[derive(Debug, Copy, Clone)]
pub enum HintStyle {
    None,
    Slight,
    Medium,
    Full,
}

impl fmt::Display for HintStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match *self {
            HintStyle::None => "none",
            HintStyle::Slight => "slight",
            HintStyle::Medium => "medium",
            HintStyle::Full => "full",
        })
    }
}

/// Lcd filter, used to reduce color fringing with subpixel rendering
pub enum LcdFilter {
    None,
    Default,
    Light,
    Legacy,
}

impl fmt::Display for LcdFilter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match *self {
            LcdFilter::None => "none",
            LcdFilter::Default => "default",
            LcdFilter::Light => "light",
            LcdFilter::Legacy => "legacy",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_match() {
        let mut pattern = Pattern::new();
        pattern.add_family("monospace");
        pattern.add_style("regular");

        let config = Config::get_current();
        let font = super::font_match(config, &mut pattern).expect("match font monospace");

        print!("index={:?}; ", font.index());
        print!("family={:?}; ", font.family());
        print!("style={:?}; ", font.style());
        print!("antialias={:?}; ", font.antialias());
        print!("autohint={:?}; ", font.autohint());
        print!("hinting={:?}; ", font.hinting());
        print!("rgba={:?}; ", font.rgba());
        print!("embeddedbitmap={:?}; ", font.embeddedbitmap());
        print!("lcdfilter={:?}; ", font.lcdfilter());
        print!("hintstyle={:?}", font.hintstyle());
        println!();
    }

    #[test]
    fn font_sort() {
        let mut pattern = Pattern::new();
        pattern.add_family("monospace");
        pattern.set_slant(Slant::Italic);

        let config = Config::get_current();
        let fonts = super::font_sort(config, &mut pattern).expect("sort font monospace");

        for font in fonts.into_iter().take(10) {
            let font = font.render_prepare(&config, &pattern);
            print!("index={:?}; ", font.index());
            print!("family={:?}; ", font.family());
            print!("style={:?}; ", font.style());
            print!("rgba={:?}", font.rgba());
            print!("rgba={:?}", font.rgba());
            println!();
        }
    }

    #[test]
    fn font_sort_with_glyph() {
        let mut charset = CharSet::new();
        charset.add('💖');
        let mut pattern = Pattern::new();
        pattern.add_charset(&charset);
        drop(charset);

        let config = Config::get_current();
        let fonts = super::font_sort(config, &mut pattern).expect("font_sort");

        for font in fonts.into_iter().take(10) {
            let font = font.render_prepare(&config, &pattern);
            print!("index={:?}; ", font.index());
            print!("family={:?}; ", font.family());
            print!("style={:?}; ", font.style());
            print!("rgba={:?}", font.rgba());
            println!();
        }
    }
}
