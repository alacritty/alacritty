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
use core_foundation::base::TCFType;
use core_graphics::context::{CGContext, CGContextRef};

use libc::{c_int};

/// Additional methods needed to render fonts for Alacritty
pub trait CGContextExt {
    fn set_font_smoothing_style(&self, style: i32);
}

impl CGContextExt for CGContext {
    fn set_font_smoothing_style(&self, style: i32) {
        unsafe {
            CGContextSetFontSmoothingStyle(self.as_concrete_TypeRef(), style as _);
        }
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
extern {
    /// As of 19 May 2017, this doesn't seem to be part of Apple's
    /// official Core Graphics API.
    fn CGContextSetFontSmoothingStyle(c: CGContextRef, style: c_int);
}
