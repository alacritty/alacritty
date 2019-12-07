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
use std::{
    ffi::{CStr, CString},
    fmt, mem,
    path::PathBuf,
    ptr::{self, NonNull},
    str,
};

use fontconfig::fontconfig::{
    FcBool, FcChar8, FcConfigSubstitute, FcDefaultSubstitute, FcFontRenderPrepare, FcPattern,
    FcPatternAddCharSet, FcPatternAddDouble, FcPatternAddInteger, FcPatternAddString,
    FcPatternCreate, FcPatternDestroy, FcPatternGetBool, FcPatternGetDouble, FcPatternGetInteger,
    FcPatternGetString, FcPatternPrint, FcResultMatch,
};
use foreign_types::{foreign_type, ForeignType, ForeignTypeRef};
use libc::{c_char, c_double, c_int};

use super::{CharSetRef, ConfigRef, HintStyle, LcdFilter, MatchKind, Rgba, Slant, Weight, Width};

pub struct StringPropertyIter<'a> {
    pattern: &'a PatternRef,
    object: &'a [u8],
    index: usize,
}

impl<'a> StringPropertyIter<'a> {
    fn new<'b>(pattern: &'b PatternRef, object: &'b [u8]) -> StringPropertyIter<'b> {
        StringPropertyIter { pattern, object, index: 0 }
    }

    fn get_value(&self, index: usize) -> Option<&'a str> {
        let mut value: *mut FcChar8 = ptr::null_mut();

        let result = unsafe {
            FcPatternGetString(
                self.pattern.as_ptr(),
                self.object.as_ptr() as *mut c_char,
                index as c_int,
                &mut value,
            )
        };

        if result == FcResultMatch {
            // Transmute here is to extend lifetime of the str to that of the iterator
            //
            // Potential unsafety? What happens if the pattern is modified while this ptr is
            // borrowed out?
            Some(unsafe {
                mem::transmute(CStr::from_ptr(value as *const c_char).to_str().unwrap())
            })
        } else {
            None
        }
    }
}

/// Iterator over integer properties
pub struct BooleanPropertyIter<'a> {
    pattern: &'a PatternRef,
    object: &'a [u8],
    index: usize,
}

impl<'a> BooleanPropertyIter<'a> {
    fn new<'b>(pattern: &'b PatternRef, object: &'b [u8]) -> BooleanPropertyIter<'b> {
        BooleanPropertyIter { pattern, object, index: 0 }
    }

    fn get_value(&self, index: usize) -> Option<bool> {
        let mut value: FcBool = 0;

        let result = unsafe {
            FcPatternGetBool(
                self.pattern.as_ptr(),
                self.object.as_ptr() as *mut c_char,
                index as c_int,
                &mut value,
            )
        };

        if result == FcResultMatch {
            Some(value != 0)
        } else {
            None
        }
    }
}

/// Iterator over integer properties
pub struct IntPropertyIter<'a> {
    pattern: &'a PatternRef,
    object: &'a [u8],
    index: usize,
}

impl<'a> IntPropertyIter<'a> {
    fn new<'b>(pattern: &'b PatternRef, object: &'b [u8]) -> IntPropertyIter<'b> {
        IntPropertyIter { pattern, object, index: 0 }
    }

    fn get_value(&self, index: usize) -> Option<isize> {
        let mut value = 0 as c_int;

        let result = unsafe {
            FcPatternGetInteger(
                self.pattern.as_ptr(),
                self.object.as_ptr() as *mut c_char,
                index as c_int,
                &mut value,
            )
        };

        if result == FcResultMatch {
            Some(value as isize)
        } else {
            None
        }
    }
}

pub struct RgbaPropertyIter<'a> {
    inner: IntPropertyIter<'a>,
}

impl<'a> RgbaPropertyIter<'a> {
    fn new<'b>(pattern: &'b PatternRef, object: &'b [u8]) -> RgbaPropertyIter<'b> {
        RgbaPropertyIter { inner: IntPropertyIter::new(pattern, object) }
    }

    #[inline]
    fn inner<'b>(&'b mut self) -> &'b mut IntPropertyIter<'a> {
        &mut self.inner
    }

    fn get_value(&self, index: usize) -> Option<Rgba> {
        self.inner.get_value(index).map(Rgba::from)
    }
}

pub struct HintStylePropertyIter<'a> {
    inner: IntPropertyIter<'a>,
}

impl<'a> HintStylePropertyIter<'a> {
    fn new(pattern: &PatternRef) -> HintStylePropertyIter {
        HintStylePropertyIter { inner: IntPropertyIter::new(pattern, b"hintstyle\0") }
    }

    #[inline]
    fn inner<'b>(&'b mut self) -> &'b mut IntPropertyIter<'a> {
        &mut self.inner
    }

    fn get_value(&self, index: usize) -> Option<HintStyle> {
        self.inner.get_value(index).and_then(|hint_style| {
            Some(match hint_style {
                0 => HintStyle::None,
                1 => HintStyle::Slight,
                2 => HintStyle::Medium,
                3 => HintStyle::Full,
                _ => return None,
            })
        })
    }
}

pub struct LcdFilterPropertyIter<'a> {
    inner: IntPropertyIter<'a>,
}

impl<'a> LcdFilterPropertyIter<'a> {
    fn new(pattern: &PatternRef) -> LcdFilterPropertyIter {
        LcdFilterPropertyIter { inner: IntPropertyIter::new(pattern, b"lcdfilter\0") }
    }

    #[inline]
    fn inner<'b>(&'b mut self) -> &'b mut IntPropertyIter<'a> {
        &mut self.inner
    }

    fn get_value(&self, index: usize) -> Option<LcdFilter> {
        self.inner.get_value(index).and_then(|hint_style| {
            Some(match hint_style {
                0 => LcdFilter::None,
                1 => LcdFilter::Default,
                2 => LcdFilter::Light,
                3 => LcdFilter::Legacy,
                _ => return None,
            })
        })
    }
}

/// Iterator over integer properties
pub struct DoublePropertyIter<'a> {
    pattern: &'a PatternRef,
    object: &'a [u8],
    index: usize,
}

impl<'a> DoublePropertyIter<'a> {
    fn new<'b>(pattern: &'b PatternRef, object: &'b [u8]) -> DoublePropertyIter<'b> {
        DoublePropertyIter { pattern, object, index: 0 }
    }

    fn get_value(&self, index: usize) -> Option<f64> {
        let mut value = f64::from(0);

        let result = unsafe {
            FcPatternGetDouble(
                self.pattern.as_ptr(),
                self.object.as_ptr() as *mut c_char,
                index as c_int,
                &mut value,
            )
        };

        if result == FcResultMatch {
            Some(value as f64)
        } else {
            None
        }
    }
}

/// Implement debug for a property iterator
macro_rules! impl_property_iter_debug {
    ($iter:ty => $item:ty) => {
        impl<'a> fmt::Debug for $iter {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "[")?;
                for i in 0.. {
                    match self.get_value(i) {
                        Some(val) => {
                            if i > 0 {
                                write!(f, ", {}", val)?;
                            } else {
                                write!(f, "{}", val)?;
                            }
                        },
                        _ => break,
                    }
                }
                write!(f, "]")
            }
        }
    };
}

/// Implement Iterator and Debug for a property iterator
macro_rules! impl_property_iter {
    ($($iter:ty => $item:ty),*) => {
        $(
            impl<'a> Iterator for $iter {
                type Item = $item;

                fn next(&mut self) -> Option<Self::Item> {
                    let res = self.get_value(self.index);
                    self.index += 1;
                    res
                }

                #[inline]
                fn nth(&mut self, n: usize) -> Option<Self::Item> {
                    self.index += n;
                    self.next()
                }
            }
            impl_property_iter_debug!($iter => $item);
        )*
    }
}

/// Implement Iterator and Debug for a property iterator which internally relies
/// on another property iterator.
macro_rules! impl_derived_property_iter {
    ($($iter:ty => $item:ty),*) => {
        $(
            impl<'a> Iterator for $iter {
                type Item = $item;

                fn next(&mut self) -> Option<Self::Item> {
                    let index = { self.inner().index };
                    let res = self.get_value(index);
                    self.inner().index += 1;
                    res
                }

                #[inline]
                fn nth(&mut self, n: usize) -> Option<Self::Item> {
                    self.inner().index += n;
                    self.next()
                }
            }
            impl_property_iter_debug!($iter => $item);
        )*
    }
}

// Basic Iterators
impl_property_iter! {
    StringPropertyIter<'a> => &'a str,
    IntPropertyIter<'a> => isize,
    DoublePropertyIter<'a> => f64,
    BooleanPropertyIter<'a> => bool
}

// Derived Iterators
impl_derived_property_iter! {
    RgbaPropertyIter<'a> => Rgba,
    HintStylePropertyIter<'a> => HintStyle,
    LcdFilterPropertyIter<'a> => LcdFilter
}

foreign_type! {
    pub unsafe type Pattern {
        type CType = FcPattern;
        fn drop = FcPatternDestroy;
    }
}

macro_rules! string_accessor {
    ($([$getter:ident, $setter:ident] => $object_name:expr),*) => {
        $(
            #[inline]
            pub fn $setter(&mut self, value: &str) -> bool {
                unsafe {
                    self.add_string($object_name, value)
                }
            }

            #[inline]
            pub fn $getter(&self) -> StringPropertyIter {
                unsafe {
                    self.get_string($object_name)
                }
            }
        )*
    }
}

impl self::Pattern {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for self::Pattern {
    fn default() -> Self {
        Pattern(unsafe { NonNull::new(FcPatternCreate()).unwrap() })
    }
}

macro_rules! pattern_get_integer {
    ($($method:ident() => $property:expr),+) => {
        $(
            pub fn $method(&self) -> IntPropertyIter {
                unsafe {
                    self.get_integer($property)
                }
            }
        )+
    };
}

macro_rules! boolean_getter {
    ($($method:ident() => $property:expr),*) => {
        $(
            pub fn $method(&self) -> BooleanPropertyIter {
                unsafe {
                    self.get_boolean($property)
                }
            }
        )*
    }
}

macro_rules! double_getter {
    ($($method:ident() => $property:expr),*) => {
        $(
            pub fn $method(&self) -> DoublePropertyIter {
                unsafe {
                    self.get_double($property)
                }
            }
        )*
    }
}

impl PatternRef {
    boolean_getter! {
        antialias() => b"antialias\0",
        hinting() => b"hinting\0",
        verticallayout() => b"verticallayout\0",
        autohint() => b"autohint\0",
        globaladvance() => b"globaladvance\0",
        scalable() => b"scalable\0",
        symbol() => b"symbol\0",
        color() => b"color\0",
        minspace() => b"minspace\0",
        embolden() => b"embolden\0",
        embeddedbitmap() => b"embeddedbitmap\0",
        decorative() => b"decorative\0"
    }

    double_getter! {
        size() => b"size\0",
        aspect() => b"aspect\0",
        pixelsize() => b"pixelsize\0",
        scale() => b"scale\0",
        dpi() => b"dpi\0"
    }

    string_accessor! {
        [family, add_family] => b"family\0",
        [familylang, add_familylang] => b"familylang\0",
        [style, add_style] => b"style\0",
        [stylelang, add_stylelang] => b"stylelang\0",
        [fullname, add_fullname] => b"fullname\0",
        [fullnamelang, add_fullnamelang] => b"fullnamelang\0",
        [foundry, add_foundry] => b"foundry\0",
        [capability, add_capability] => b"capability\0",
        [fontformat, add_fontformat] => b"fontformat\0",
        [fontfeatures, add_fontfeatures] => b"fontfeatures\0",
        [namelang, add_namelang] => b"namelang\0",
        [postscriptname, add_postscriptname] => b"postscriptname\0"
    }

    pattern_get_integer! {
        index() => b"index\0"
    }

    // Prints the pattern to stdout
    //
    // FontConfig doesn't expose a way to iterate over all members of a pattern;
    // instead, we just defer to FcPatternPrint. Otherwise, this could have been
    // a `fmt::Debug` impl.
    pub fn print(&self) {
        unsafe { FcPatternPrint(self.as_ptr()) }
    }

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

        FcPatternAddString(self.as_ptr(), object.as_ptr() as *mut c_char, value as *mut FcChar8)
            == 1
    }

    unsafe fn add_integer(&self, object: &[u8], int: isize) -> bool {
        FcPatternAddInteger(self.as_ptr(), object.as_ptr() as *mut c_char, int as c_int) == 1
    }

    unsafe fn add_double(&self, object: &[u8], value: f64) -> bool {
        FcPatternAddDouble(self.as_ptr(), object.as_ptr() as *mut c_char, value as c_double) == 1
    }

    unsafe fn get_string<'a>(&'a self, object: &'a [u8]) -> StringPropertyIter<'a> {
        StringPropertyIter::new(self, object)
    }

    unsafe fn get_integer<'a>(&'a self, object: &'a [u8]) -> IntPropertyIter<'a> {
        IntPropertyIter::new(self, object)
    }

    unsafe fn get_double<'a>(&'a self, object: &'a [u8]) -> DoublePropertyIter<'a> {
        DoublePropertyIter::new(self, object)
    }

    unsafe fn get_boolean<'a>(&'a self, object: &'a [u8]) -> BooleanPropertyIter<'a> {
        BooleanPropertyIter::new(self, object)
    }

    pub fn hintstyle(&self) -> HintStylePropertyIter {
        HintStylePropertyIter::new(self)
    }

    pub fn lcdfilter(&self) -> LcdFilterPropertyIter {
        LcdFilterPropertyIter::new(self)
    }

    pub fn set_slant(&mut self, slant: Slant) -> bool {
        unsafe { self.add_integer(b"slant\0", slant as isize) }
    }

    pub fn add_pixelsize(&mut self, size: f64) -> bool {
        unsafe { self.add_double(b"pixelsize\0", size) }
    }

    pub fn set_weight(&mut self, weight: Weight) -> bool {
        unsafe { self.add_integer(b"weight\0", weight as isize) }
    }

    pub fn set_width(&mut self, width: Width) -> bool {
        unsafe { self.add_integer(b"width\0", width.to_isize()) }
    }

    pub fn get_width(&self) -> Option<Width> {
        unsafe { self.get_integer(b"width\0").nth(0).map(Width::from) }
    }

    pub fn rgba(&self) -> RgbaPropertyIter {
        RgbaPropertyIter::new(self, b"rgba\0")
    }

    pub fn set_rgba(&self, rgba: &Rgba) -> bool {
        unsafe { self.add_integer(b"rgba\0", rgba.to_isize()) }
    }

    pub fn render_prepare(&self, config: &ConfigRef, request: &PatternRef) -> self::Pattern {
        unsafe {
            let ptr = FcFontRenderPrepare(config.as_ptr(), request.as_ptr(), self.as_ptr());
            Pattern::from_ptr(ptr)
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
                charset.as_ptr(),
            ) == 1
        }
    }

    pub fn file(&self, index: usize) -> Option<PathBuf> {
        unsafe { self.get_string(b"file\0").nth(index) }.map(From::from)
    }

    pub fn config_substitute(&mut self, config: &ConfigRef, kind: MatchKind) {
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
