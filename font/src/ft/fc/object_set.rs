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
use libc::c_char;

use foreign_types::ForeignTypeRef;
use super::ffi::{FcObjectSetCreate, FcObjectSetAdd, FcObjectSet, FcObjectSetDestroy};

foreign_type! {
    type CType = FcObjectSet;
    fn drop = FcObjectSetDestroy;
    pub struct ObjectSet;
    pub struct ObjectSetRef;
}

impl ObjectSet {
    #[allow(dead_code)]
    pub fn new() -> ObjectSet {
        ObjectSet(unsafe {
            FcObjectSetCreate()
        })
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
