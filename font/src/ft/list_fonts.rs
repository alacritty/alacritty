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
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

mod fc {
    use std::ptr;
    use std::ffi::{CStr, CString};
    use std::str;
    use std::ops::{Deref, DerefMut};

    use libc::{c_char, c_int};
    use fontconfig::fontconfig as ffi;

    use self::ffi::{FcConfigGetCurrent, FcConfigGetFonts};
    use self::ffi::{FcPatternGetString, FcPatternCreate, FcPatternAddString};
    use self::ffi::{FcPatternGetInteger};
    use self::ffi::{FcObjectSetCreate, FcObjectSetAdd};
    use self::ffi::{FcResultMatch, FcFontSetList};
    use self::ffi::{FcChar8, FcConfig, FcPattern, FcFontSet, FcObjectSet};
    use self::ffi::{FcFontSetDestroy, FcPatternDestroy, FcObjectSetDestroy, FcConfigDestroy};

    /// FcConfig - Font Configuration
    pub struct Config(*mut FcConfig);

    /// FcFontSet
    pub struct FontSet(*mut FcFontSet);

    /// FcFontSet reference
    pub struct FontSetRef(*mut FcFontSet);

    /// Iterator over a font set
    pub struct FontSetIter<'a> {
        font_set: &'a FontSetRef,
        num_fonts: usize,
        current: usize,
    }

    macro_rules! ref_type {
        ($($owned:ty => $refty:ty),*) => {
            $(
                impl Deref for $owned {
                    type Target = $refty;
                    fn deref(&self) -> &Self::Target {
                        unsafe {
                            &*(self.0 as *mut _)
                        }
                    }
                }

                impl DerefMut for $owned {
                    fn deref_mut(&mut self) -> &mut Self::Target {
                        unsafe {
                            &mut *(self.0 as *mut _)
                        }
                    }
                }
            )*
        }
    }

    /// FcPattern
    pub struct Pattern(*mut FcPattern);

    /// FcObjectSet
    pub struct ObjectSet(*mut FcObjectSet);

    /// FcObjectSet reference
    pub struct ObjectSetRef(*mut FcObjectSet);

    ref_type! {
        ObjectSet => ObjectSetRef,
        Pattern => PatternRef,
        FontSet => FontSetRef
    }

    impl Drop for ObjectSet {
        fn drop(&mut self) {
            unsafe {
                FcObjectSetDestroy(self.0);
            }
        }
    }

    impl ObjectSet {
        pub fn new() -> ObjectSet {
            ObjectSet(unsafe {
                FcObjectSetCreate()
            })
        }
    }


    impl ObjectSetRef {
        fn add(&mut self, property: &[u8]) {
            unsafe {
                FcObjectSetAdd(self.0, property.as_ptr() as *mut c_char);
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
        ($name:ident => $object:expr) => {
            #[inline]
            pub fn $name(&mut self, value: &str) -> bool {
                unsafe {
                    self.add_string($object, value)
                }
            }
        }
    }

    impl Pattern {
        pub fn new() -> Pattern {
            Pattern(unsafe { FcPatternCreate() })
        }
    }

    impl Drop for Pattern {
        fn drop(&mut self) {
            unsafe {
                FcPatternDestroy(self.0);
            }
        }
    }

    /// FcPattern reference
    pub struct PatternRef(*mut FcPattern);

    /// Available font sets
    pub enum SetName {
        System = 0,
        Application = 1,
    }

    pub unsafe fn char8_to_string(fc_str: *mut FcChar8) -> String {
        str::from_utf8(CStr::from_ptr(fc_str as *const c_char).to_bytes()).unwrap().to_owned()
    }

    macro_rules! pattern_get_string {
        ($($method:ident() => $property:expr),+) => {
            $(
                pub fn $method(&self, id: isize) -> Option<String> {
                    unsafe {
                        let mut format: *mut FcChar8 = ptr::null_mut();

                        let result = FcPatternGetString(
                            self.0,
                            $property.as_ptr() as *mut c_char,
                            id as c_int,
                            &mut format
                        );

                        if result == FcResultMatch {
                            Some(char8_to_string(format))
                        } else {
                            None
                        }
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
                            self.0,
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
                self.0,
                object.as_ptr() as *mut c_char,
                value as *mut FcChar8
            ) == 1
        }

        pattern_add_string! {
            add_family => b"family\0"
        }

        pattern_get_string! {
            fontformat() => b"fontformat\0",
            family() => b"family\0",
            file() => b"file\0",
            style() => b"style\0"
        }

        pattern_get_integer! {
            index() => b"index\0"
        }
    }

    impl<'a> IntoIterator for &'a FontSet {
        type Item = &'a PatternRef;
        type IntoIter = FontSetIter<'a>;
        fn into_iter(self) -> FontSetIter<'a> {
            let num_fonts = unsafe {
                (*self.0).nfont as isize
            };

            FontSetIter {
                font_set: unsafe { &*(self.0 as *mut _) },
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
                (*self.0).nfont as isize
            };

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
                    let ptr = *(*self.font_set.0).fonts.offset(self.current as isize);
                    &*(ptr as *mut _)
                };

                self.current += 1;
                Some(pattern)
            }
        }
    }

    impl FontSet {
        pub fn list(
            config: &Config,
            source: &mut FontSetRef,
            pattern: &PatternRef,
            objects: &ObjectSetRef
        ) -> FontSet {
            let raw = unsafe {
                FcFontSetList(
                    config.0,
                    &mut source.0,
                    1 /* nsets */,
                    pattern.0,
                    objects.0
                )
            };
            FontSet(raw)
        }
    }

    impl Config {
        /// Get the current configuration
        pub fn get_current() -> Config {
            Config(unsafe { FcConfigGetCurrent() })
        }

        /// Returns one of the two sets of fonts from the configuration as
        /// specified by `set`.
        pub fn get_fonts<'a>(&'a self, set: SetName) -> &'a FontSetRef {
            unsafe {
                let ptr = FcConfigGetFonts(self.0, set as u32);
                &*(ptr as *mut _)
            }
        }
    }

    impl Drop for FontSet {
        fn drop(&mut self) {
            unsafe {
                FcFontSetDestroy(self.0)
            }
        }
    }

    impl Drop for Config {
        fn drop(&mut self) {
            unsafe {
                if self.0 != FcConfigGetCurrent() {
                    FcConfigDestroy(self.0)
                }
            }
        }
    }
}

fn list_families() -> Vec<String> {
    let mut families = Vec::new();

    let config = fc::Config::get_current();
    let font_set = config.get_fonts(fc::SetName::System);
    for font in font_set {
        if let Some(format) = font.fontformat(0) {
            if format == "TrueType" || format == "CFF" {
                let id = 0;
                while let Some(family) = font.family(id) {
                    families.push(family);
                }
            }
        }
    }

    families.sort();
    families.dedup();
    families
}

#[derive(Debug)]
pub struct Variant {
    style: String,
    file: PathBuf,
    index: isize,
}

impl Variant {
    #[inline]
    pub fn path(&self) -> &::std::path::Path {
        self.file.as_path()
    }

    #[inline]
    pub fn index(&self) -> isize {
        self.index
    }
}

#[derive(Debug)]
pub struct Family {
    name: String,
    variants: HashMap<String, Variant>,
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: ", self.name)?;
        for (k, _v) in &self.variants {
            write!(f, "{}, ", k)?;
        }

        Ok(())
    }
}

impl Family {
    #[inline]
    pub fn variants(&self) -> &HashMap<String, Variant> {
        &self.variants
    }
}

#[allow(mutable_transmutes)]
pub fn get_family_info(family: String) -> Family {
    let mut members = Vec::new();
    let config = fc::Config::get_current();
    let font_set = config.get_fonts(fc::SetName::System);

    let mut pattern = fc::Pattern::new();
    pattern.add_family(&family);

    let mut objects = fc::ObjectSet::new();
    objects.add_file();
    objects.add_index();
    objects.add_style();

    let variants = fc::FontSet::list(&config, unsafe { ::std::mem::transmute(font_set) }, &pattern, &objects);
    for variant in &variants {
        if let Some(file) = variant.file(0) {
            if let Some(style) = variant.style(0) {
                if let Some(index) = variant.index(0) {
                    members.push(Variant {
                        style: style,
                        file: PathBuf::from(file),
                        index: index as isize,
                    });
                }
            }
        }
    }

    Family {
        name: family,
        variants: members.into_iter().map(|v| (v.style.clone(), v)).collect()
    }
}

pub fn get_font_families() -> HashMap<String, Family> {
    list_families()
        .into_iter()
        .map(|family| (family.clone(), get_family_info(family)))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn get_font_families() {
        let families = super::get_font_families();
        assert!(!families.is_empty());
    }
}
