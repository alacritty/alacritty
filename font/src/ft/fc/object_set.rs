use std::ptr::NonNull;

use libc::c_char;

use super::ffi::{FcObjectSet, FcObjectSetAdd, FcObjectSetCreate, FcObjectSetDestroy};
use foreign_types::{foreign_type, ForeignTypeRef};

foreign_type! {
    pub unsafe type ObjectSet {
        type CType = FcObjectSet;
        fn drop = FcObjectSetDestroy;
    }
}

impl ObjectSet {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for ObjectSet {
    fn default() -> Self {
        ObjectSet(unsafe { NonNull::new(FcObjectSetCreate()).unwrap() })
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
