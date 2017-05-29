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
use core_foundation::base::{CFRelease, CFRetain, CFTypeID, CFTypeRef, TCFType};
//use core_graphics::color_space::{CGColorSpace, CGColorSpaceRef};
//use core_graphics::base::CGFloat;
use std::mem;

#[repr(C)]
pub struct __CGColor;

pub type CGColorRef = *const __CGColor;

pub struct CGColor {
    obj: CGColorRef,
}

impl Drop for CGColor {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.as_CFTypeRef())
        }
    }
}

impl Clone for CGColor {
    fn clone(&self) -> CGColor {
        unsafe {
            TCFType::wrap_under_get_rule(self.as_concrete_TypeRef())
        }
    }
}

impl TCFType<CGColorRef> for CGColor {
    #[inline]
    fn as_concrete_TypeRef(&self) -> CGColorRef {
        self.obj
    }

    #[inline]
    unsafe fn wrap_under_get_rule(reference: CGColorRef) -> CGColor {
        let reference: CGColorRef = mem::transmute(CFRetain(mem::transmute(reference)));
        TCFType::wrap_under_create_rule(reference)
    }

    #[inline]
    fn as_CFTypeRef(&self) -> CFTypeRef {
        unsafe {
            mem::transmute(self.as_concrete_TypeRef())
        }
    }

    #[inline]
    unsafe fn wrap_under_create_rule(obj: CGColorRef) -> CGColor {
        CGColor {
            obj: obj,
        }
    }

    #[inline]
    fn type_id() -> CFTypeID {
        unsafe {
            CGColorGetTypeID()
        }
    }
}

// impl CGColor {
//     pub fn new(color_space: CGColorSpace, values: [CGFloat; 4]) -> CGColor {
//         unsafe {
//             let result = CGColorCreate(color_space.as_concrete_TypeRef(), values.as_ptr());
//             TCFType::wrap_under_create_rule(result)
//         }
//     }
// }

#[link(name = "ApplicationServices", kind = "framework")]
extern {
//    fn CGColorCreate(space: CGColorSpaceRef, vals: *const CGFloat) -> CGColorRef;
    fn CGColorGetTypeID() -> CFTypeID;
}
