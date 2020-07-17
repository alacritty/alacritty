use std::ptr::NonNull;

use foreign_types::{foreign_type, ForeignType, ForeignTypeRef};

use super::ffi::FcCharSetCreate;
use super::ffi::{
    FcBool, FcCharSet, FcCharSetAddChar, FcCharSetCopy, FcCharSetCount, FcCharSetDestroy,
    FcCharSetHasChar, FcCharSetMerge, FcCharSetSubtract, FcCharSetUnion,
};

foreign_type! {
    pub unsafe type CharSet {
        type CType = FcCharSet;
        fn drop = FcCharSetDestroy;
        fn clone = FcCharSetCopy;
    }
}

impl CharSet {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for CharSet {
    fn default() -> Self {
        CharSet(unsafe { NonNull::new(FcCharSetCreate()).unwrap() })
    }
}

impl CharSetRef {
    pub fn add(&mut self, glyph: char) -> bool {
        unsafe { FcCharSetAddChar(self.as_ptr(), glyph as _) == 1 }
    }

    pub fn has_char(&self, glyph: char) -> bool {
        unsafe { FcCharSetHasChar(self.as_ptr(), glyph as _) == 1 }
    }

    pub fn count(&self) -> u32 {
        unsafe { FcCharSetCount(self.as_ptr()) as u32 }
    }

    pub fn union(&self, other: &CharSetRef) -> CharSet {
        unsafe {
            let ptr = FcCharSetUnion(self.as_ptr() as _, other.as_ptr() as _);
            CharSet::from_ptr(ptr)
        }
    }

    pub fn subtract(&self, other: &CharSetRef) -> CharSet {
        unsafe {
            let ptr = FcCharSetSubtract(self.as_ptr() as _, other.as_ptr() as _);
            CharSet::from_ptr(ptr)
        }
    }

    pub fn merge(&self, other: &CharSetRef) -> Result<bool, ()> {
        unsafe {
            // Value is just an indicator whether something was added or not.
            let mut value: FcBool = 0;
            let res = FcCharSetMerge(self.as_ptr() as _, other.as_ptr() as _, &mut value);
            if res == 0 {
                Err(())
            } else {
                Ok(value != 0)
            }
        }
    }
}
