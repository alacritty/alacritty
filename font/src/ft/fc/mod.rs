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
use std::ptr;

use foreign_types::{ForeignType, ForeignTypeRef};

use fontconfig::fontconfig as ffi;

use self::ffi::{FcSetSystem, FcSetApplication};
use self::ffi::FcResultNoMatch;
use self::ffi::{FcFontMatch, FcFontList, FcFontSort};
use self::ffi::{FcMatchFont, FcMatchPattern, FcMatchScan};
use self::ffi::{FC_SLANT_OBLIQUE, FC_SLANT_ITALIC, FC_SLANT_ROMAN};
use self::ffi::{FC_WEIGHT_THIN, FC_WEIGHT_EXTRALIGHT, FC_WEIGHT_LIGHT};
use self::ffi::{FC_WEIGHT_BOOK, FC_WEIGHT_REGULAR, FC_WEIGHT_MEDIUM, FC_WEIGHT_SEMIBOLD};
use self::ffi::{FC_WEIGHT_BOLD, FC_WEIGHT_EXTRABOLD, FC_WEIGHT_BLACK, FC_WEIGHT_EXTRABLACK};

mod config;
pub use self::config::{Config, ConfigRef};

mod font_set;
pub use self::font_set::{FontSet, FontSetRef};

mod object_set;
pub use self::object_set::{ObjectSet, ObjectSetRef};

mod char_set;
pub use self::char_set::{CharSet, CharSetRef};

mod pattern;
pub use self::pattern::{Pattern, PatternRef};

/// Find the font closest matching the provided pattern.
pub fn font_match(
    config: &ConfigRef,
    pattern: &mut PatternRef,
) -> Option<Pattern> {
    pattern.config_subsitute(config, MatchKind::Pattern);
    pattern.default_substitute();

    unsafe {
        // What is this result actually used for? Seems redundant with
        // return type.
        let mut result = FcResultNoMatch;
        let ptr = FcFontMatch(
            config.as_ptr(),
            pattern.as_ptr(),
            &mut result,
            );

        if ptr.is_null() {
            None
        } else {
            Some(Pattern::from_ptr(ptr))
        }
    }
}

/// list fonts by closeness to the pattern
#[allow(dead_code)]
pub fn font_sort(
    config: &ConfigRef,
    pattern: &mut PatternRef,
) -> Option<FontSet> {
    pattern.config_subsitute(config, MatchKind::Pattern);
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
#[allow(dead_code)]
pub fn font_list(
    config: &ConfigRef,
    pattern: &mut PatternRef,
    objects: &ObjectSetRef,
) -> Option<FontSet> {
    pattern.config_subsitute(config, MatchKind::Pattern);
    pattern.default_substitute();

    unsafe {
        let ptr = FcFontList(
            config.as_ptr(),
            pattern.as_ptr(),
            objects.as_ptr(),
        );

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

        print!("family={:?}", font.family(0));
        for i in 0.. {
            if let Some(style) = font.style(i) {
                print!(", style={:?}, ", style);
            } else {
                break;
            }
        }
        info!("");
    }

    #[test]
    fn font_sort() {
        let mut pattern = Pattern::new();
        pattern.add_family("monospace");
        pattern.set_slant(Slant::Italic);

        let config = Config::get_current();
        let fonts = super::font_sort(config, &mut pattern)
            .expect("sort font monospace");

        for font in fonts.into_iter().take(10) {
            print!("family={:?}", font.family(0));
            for i in 0.. {
                if let Some(style) = font.style(i) {
                    print!(", style={:?}", style);
                } else {
                    break;
                }
            }
            println!("");
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
            print!("family={:?}", font.family(0));
            for i in 0.. {
                if let Some(style) = font.style(i) {
                    print!(", style={:?}", style);
                } else {
                    break;
                }
            }
            println!("");
        }
    }
}
