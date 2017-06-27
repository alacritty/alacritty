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
use std::ptr;
use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::str;

use libc::{c_char, c_int};
use foreign_types::{ForeignTypeRef};

use super::ffi::FcResultMatch;
use super::ffi::{FcPatternDestroy, FcPatternAddCharSet};
use super::ffi::{FcPatternGetString, FcPatternCreate, FcPatternAddString};
use super::ffi::{FcPatternGetInteger, FcPatternAddInteger};
use super::ffi::{FcChar8, FcPattern, FcDefaultSubstitute, FcConfigSubstitute};

use super::{MatchKind, ConfigRef, CharSetRef, Weight, Slant};

foreign_type! {
    type CType = FcPattern;
    fn drop = FcPatternDestroy;
    pub struct Pattern;
    pub struct PatternRef;
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

unsafe fn char8_to_string(fc_str: *mut FcChar8) -> String {
    str::from_utf8(CStr::from_ptr(fc_str as *const c_char).to_bytes()).unwrap().to_owned()
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


    /// Add charset to the pattern
    ///
    /// The referenced charset is copied by fontconfig internally using
    /// FcValueSave so that no references to application provided memory are
    /// retained. That is, the CharSet can be safely dropped immediately
    /// after being added to the pattern.
    pub fn add_charset(&self, charset: &CharSetRef) -> bool {
        unsafe {
            FcPatternAddCharSet(
                self.as_ptr(),
                b"charset\0".as_ptr() as *mut c_char,
                charset.as_ptr()
            ) == 1
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

