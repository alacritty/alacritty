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
use std::ptr::NonNull;

use super::ffi::{FcObjectSet, FcObjectSetAdd, FcObjectSetCreate, FcObjectSetDestroy};
use foreign_types::{foreign_type, ForeignTypeRef};
use libc::c_char;

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
