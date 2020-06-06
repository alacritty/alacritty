use std::ops::Deref;
use std::ptr::NonNull;

use foreign_types::{foreign_type, ForeignType, ForeignTypeRef};
use log::trace;

use super::{ConfigRef, ObjectSetRef, PatternRef};

use super::ffi::{FcFontSet, FcFontSetDestroy, FcFontSetList};

foreign_type! {
    pub unsafe type FontSet {
        type CType = FcFontSet;
        fn drop = FcFontSetDestroy;
    }
}

impl FontSet {
    pub fn list(
        config: &ConfigRef,
        source: &mut FontSetRef,
        pattern: &PatternRef,
        objects: &ObjectSetRef,
    ) -> FontSet {
        let raw = unsafe {
            FcFontSetList(
                config.as_ptr(),
                &mut source.as_ptr(),
                1, // nsets.
                pattern.as_ptr(),
                objects.as_ptr(),
            )
        };
        FontSet(NonNull::new(raw).unwrap())
    }
}

/// Iterator over a font set.
pub struct Iter<'a> {
    font_set: &'a FontSetRef,
    num_fonts: usize,
    current: usize,
}

impl<'a> IntoIterator for &'a FontSet {
    type IntoIter = Iter<'a>;
    type Item = &'a PatternRef;

    fn into_iter(self) -> Iter<'a> {
        let num_fonts = unsafe { (*self.as_ptr()).nfont as isize };

        trace!("Number of fonts is {}", num_fonts);

        Iter { font_set: self.deref(), num_fonts: num_fonts as _, current: 0 }
    }
}

impl<'a> IntoIterator for &'a FontSetRef {
    type IntoIter = Iter<'a>;
    type Item = &'a PatternRef;

    fn into_iter(self) -> Iter<'a> {
        let num_fonts = unsafe { (*self.as_ptr()).nfont as isize };

        trace!("Number of fonts is {}", num_fonts);

        Iter { font_set: self, num_fonts: num_fonts as _, current: 0 }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a PatternRef;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.num_fonts {
            None
        } else {
            let pattern = unsafe {
                let ptr = *(*self.font_set.as_ptr()).fonts.add(self.current);
                PatternRef::from_ptr(ptr)
            };

            self.current += 1;
            Some(pattern)
        }
    }
}
