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
use super::ffi::{FcConfig, FcConfigDestroy, FcConfigGetCurrent, FcConfigGetFonts};
use foreign_types::{foreign_type, ForeignTypeRef};

use super::{FontSetRef, SetName};

foreign_type! {
    pub unsafe type Config {
        type CType = FcConfig;
        fn drop = FcConfigDestroy;
    }
}

impl Config {
    /// Get the current configuration
    pub fn get_current() -> &'static ConfigRef {
        unsafe { ConfigRef::from_ptr(FcConfigGetCurrent()) }
    }
}

impl ConfigRef {
    /// Returns one of the two sets of fonts from the configuration as
    /// specified by `set`.
    pub fn get_fonts(&self, set: SetName) -> &FontSetRef {
        unsafe {
            let ptr = FcConfigGetFonts(self.as_ptr(), set as u32);
            FontSetRef::from_ptr(ptr)
        }
    }
}
