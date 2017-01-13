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
pub mod fc {
    use std::ptr;
    use std::ffi::{CStr, CString};
    use std::str;
    use std::ops::Deref;
    use std::path::PathBuf;

    use ffi_util::{ForeignType, ForeignTypeRef};

    use libc::{c_char, c_int};
    use fontconfig::fontconfig as ffi;

    use self::ffi::{FcConfigGetCurrent, FcConfigGetFonts, FcSetSystem, FcSetApplication};
    use self::ffi::{FcPatternGetString, FcPatternCreate, FcPatternAddString};
    use self::ffi::{FcPatternGetInteger, FcPatternAddInteger};
    use self::ffi::{FcObjectSetCreate, FcObjectSetAdd};
    use self::ffi::{FcResultMatch, FcResultNoMatch, FcFontSetList};
    use self::ffi::{FcChar8, FcConfig, FcPattern, FcFontSet, FcObjectSet, FcCharSet};
    use self::ffi::{FcFontSetDestroy, FcPatternDestroy, FcObjectSetDestroy, FcConfigDestroy};
    use self::ffi::{FcFontMatch, FcFontList, FcFontSort, FcConfigSubstitute, FcDefaultSubstitute};
    use self::ffi::{FcMatchFont, FcMatchPattern, FcMatchScan, FC_SLANT_ITALIC, FC_SLANT_ROMAN};
    use self::ffi::{FC_SLANT_OBLIQUE};
    use self::ffi::{FC_WEIGHT_THIN, FC_WEIGHT_EXTRALIGHT, FC_WEIGHT_LIGHT};
    use self::ffi::{FC_WEIGHT_BOOK, FC_WEIGHT_REGULAR, FC_WEIGHT_MEDIUM, FC_WEIGHT_SEMIBOLD};
    use self::ffi::{FC_WEIGHT_BOLD, FC_WEIGHT_EXTRABOLD, FC_WEIGHT_BLACK, FC_WEIGHT_EXTRABLACK};

    /// Iterator over a font set
    pub struct FontSetIter<'a> {
        font_set: &'a FontSetRef,
        num_fonts: usize,
        current: usize,
    }

    ffi_type!(Pattern, PatternRef, FcPattern, FcPatternDestroy);
    ffi_type!(Config, ConfigRef, FcConfig, FcConfigDestroy);
    ffi_type!(ObjectSet, ObjectSetRef, FcObjectSet, FcObjectSetDestroy);
    ffi_type!(FontSet, FontSetRef, FcFontSet, FcFontSetDestroy);

    impl ObjectSet {
        #[allow(dead_code)]
        pub fn new() -> ObjectSet {
            ObjectSet(unsafe {
                FcObjectSetCreate()
            })
        }
    }

    /// Find the font closest matching the provided pattern.
    #[allow(dead_code)]
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

            let mut charsets: *mut FcCharSet = ptr::null_mut();

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

    impl ObjectSetRef {
        fn add(&mut self, property: &[u8]) {
            unsafe {
                FcObjectSetAdd(self.as_ptr(), property.as_ptr() as *mut c_char);
            }
        }

        #[inline]
        pub fn add_file(&mut self) {
            self.add(b"file\0");
        }

        #[inline]
        pub fn add_index(&mut self) {
            self.add(b"index\0");
        }

        #[inline]
        pub fn add_style(&mut self) {
            self.add(b"style\0");
        }
    }

    macro_rules! pattern_add_string {
        ($($name:ident => $object:expr),*) => {
            $(
                #[inline]
                pub fn $name(&mut self, value: &str) -> bool {
                    unsafe {
                        self.add_string($object, value)
                    }
                }
            )*
        }
    }

    macro_rules! pattern_add_int {
        ($($name:ident => $object:expr),*) => {
            $(
                #[inline]
                pub fn $name(&mut self, value: &str) -> bool {
                    unsafe {
                        self.add_string($object, value)
                    }
                }
            )*
        }
    }

    impl Pattern {
        pub fn new() -> Pattern {
            Pattern(unsafe { FcPatternCreate() })
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

    pub unsafe fn char8_to_string(fc_str: *mut FcChar8) -> String {
        str::from_utf8(CStr::from_ptr(fc_str as *const c_char).to_bytes()).unwrap().to_owned()
    }

    macro_rules! pattern_get_string {
        ($($method:ident() => $property:expr),+) => {
            $(
                pub fn $method(&self, id: isize) -> Option<String> {
                    unsafe {
                        self.get_string($property, id)
                    }
                }
            )+
        };
    }

    macro_rules! pattern_add_integer {
        ($($method:ident() => $property:expr),+) => {
            $(
                pub fn $method(&self, int: isize) -> bool {
                    unsafe {
                        FcPatternAddInteger(
                            self.as_ptr(),
                            $property.as_ptr() as *mut c_char,
                            int as c_int,
                            &mut index
                        ) == 1
                    }
                }
            )+
        };
    }

    macro_rules! pattern_get_integer {
        ($($method:ident() => $property:expr),+) => {
            $(
                pub fn $method(&self, id: isize) -> Option<isize> {
                    let mut index = 0 as c_int;
                    unsafe {
                        let result = FcPatternGetInteger(
                            self.as_ptr(),
                            $property.as_ptr() as *mut c_char,
                            id as c_int,
                            &mut index
                        );

                        if result == FcResultMatch {
                            Some(index as isize)
                        } else {
                            None
                        }
                    }
                }
            )+
        };
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

    impl PatternRef {
        /// Add a string value to the pattern
        ///
        /// If the returned value is `true`, the value is added at the end of
        /// any existing list, otherwise it is inserted at the beginning.
        ///
        /// # Unsafety
        ///
        /// `object` is not checked to be a valid null-terminated string
        unsafe fn add_string(&mut self, object: &[u8], value: &str) -> bool {
            let value = CString::new(&value[..]).unwrap();
            let value = value.as_ptr();

            FcPatternAddString(
                self.as_ptr(),
                object.as_ptr() as *mut c_char,
                value as *mut FcChar8
            ) == 1
        }

        unsafe fn add_integer(&self, object: &[u8], int: isize) -> bool {
            FcPatternAddInteger(
                self.as_ptr(),
                object.as_ptr() as *mut c_char,
                int as c_int
            ) == 1
        }

        unsafe fn get_string(&self, object: &[u8], index: isize) -> Option<String> {
            let mut format: *mut FcChar8 = ptr::null_mut();

            let result = FcPatternGetString(
                self.as_ptr(),
                object.as_ptr() as *mut c_char,
                index as c_int,
                &mut format
            );

            if result == FcResultMatch {
                Some(char8_to_string(format))
            } else {
                None
            }
        }

        pattern_add_string! {
            add_family => b"family\0",
            add_style => b"style\0"
        }

        pub fn set_slant(&mut self, slant: Slant) -> bool {
            unsafe {
                self.add_integer(b"slant\0", slant as isize)
            }
        }

        pub fn set_weight(&mut self, weight: Weight) -> bool {
            unsafe {
                self.add_integer(b"weight\0", weight as isize)
            }
        }

        pub fn file(&self, index: isize) -> Option<PathBuf> {
            unsafe {
                self.get_string(b"file\0", index)
            }.map(From::from)
        }

        pattern_get_string! {
            fontformat() => b"fontformat\0",
            family() => b"family\0",
            style() => b"style\0"
        }

        pattern_get_integer! {
            index() => b"index\0"
        }

        pub fn config_subsitute(&mut self, config: &ConfigRef, kind: MatchKind) {
            unsafe {
                FcConfigSubstitute(config.as_ptr(), self.as_ptr(), kind as u32);
            }
        }

        pub fn default_substitute(&mut self) {
            unsafe {
                FcDefaultSubstitute(self.as_ptr());
            }
        }
    }

    impl<'a> IntoIterator for &'a FontSet {
        type Item = &'a PatternRef;
        type IntoIter = FontSetIter<'a>;
        fn into_iter(self) -> FontSetIter<'a> {
            let num_fonts = unsafe {
                (*self.as_ptr()).nfont as isize
            };

            info!("num fonts = {}", num_fonts);

            FontSetIter {
                font_set: self.deref(),
                num_fonts: num_fonts as _,
                current: 0,
            }
        }
    }

    impl<'a> IntoIterator for &'a FontSetRef {
        type Item = &'a PatternRef;
        type IntoIter = FontSetIter<'a>;
        fn into_iter(self) -> FontSetIter<'a> {
            let num_fonts = unsafe {
                (*self.as_ptr()).nfont as isize
            };

            info!("num fonts = {}", num_fonts);

            FontSetIter {
                font_set: self,
                num_fonts: num_fonts as _,
                current: 0,
            }
        }
    }

    impl<'a> Iterator for FontSetIter<'a> {
        type Item = &'a PatternRef;

        fn next(&mut self) -> Option<Self::Item> {
            if self.current == self.num_fonts {
                None
            } else {
                let pattern = unsafe {
                    let ptr = *(*self.font_set.as_ptr()).fonts.offset(self.current as isize);
                    PatternRef::from_ptr(ptr)
                };

                self.current += 1;
                Some(pattern)
            }
        }
    }

    impl FontSet {
        pub fn list(
            config: &ConfigRef,
            source: &mut FontSetRef,
            pattern: &PatternRef,
            objects: &ObjectSetRef
        ) -> FontSet {
            let raw = unsafe {
                FcFontSetList(
                    config.as_ptr(),
                    &mut source.as_ptr(),
                    1 /* nsets */,
                    pattern.as_ptr(),
                    objects.as_ptr(),
                )
            };
            FontSet(raw)
        }
    }

    impl Config {
        /// Get the current configuration
        pub fn get_current() -> &'static ConfigRef {
            unsafe {
                ConfigRef::from_ptr(FcConfigGetCurrent())
            }
        }
    }

    impl ConfigRef {
        /// Returns one of the two sets of fonts from the configuration as
        /// specified by `set`.
        pub fn get_fonts<'a>(&'a self, set: SetName) -> &'a FontSetRef {
            unsafe {
                let ptr = FcConfigGetFonts(self.as_ptr(), set as u32);
                FontSetRef::from_ptr(ptr)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fc;

    #[test]
    fn font_match() {
        let mut pattern = fc::Pattern::new();
        pattern.add_family("monospace");
        pattern.add_style("regular");

        let config = fc::Config::get_current();
        let font = fc::font_match(config, &mut pattern).expect("match font monospace");

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
        let mut pattern = fc::Pattern::new();
        pattern.add_family("monospace");
        pattern.set_slant(fc::Slant::Italic);

        let config = fc::Config::get_current();
        let fonts = fc::font_sort(config, &mut pattern)
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
            info!("");
        }
    }
}
