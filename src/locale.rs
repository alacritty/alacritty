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
#![cfg(target_os = "macos")]
use std::os::raw::c_char;
use std::slice;
use std::str;
use std::env;

use objc::runtime::{Class, Object};

pub fn set_locale_environment() {
    let locale_id = unsafe {
        let locale_class = Class::get("NSLocale").unwrap();
        let locale: *const Object = msg_send![locale_class, currentLocale];
        let _ : () = msg_send![locale_class, release];
        let identifier: *const Object = msg_send![locale, localeIdentifier];
        let _ : () = msg_send![locale, release];
        let identifier_str = nsstring_as_str(identifier).to_owned();
        let _ : () = msg_send![identifier, release];
        identifier_str
    };
    let locale_id = locale_id + ".UTF-8";
    env::set_var("LANG", &locale_id);
    env::set_var("LC_CTYPE", &locale_id);
}

const UTF8_ENCODING: usize = 4;

unsafe fn nsstring_as_str<'a>(nsstring: *const Object) -> &'a str {
    let cstr: *const c_char = msg_send![nsstring, UTF8String];
    let len: usize = msg_send![nsstring, lengthOfBytesUsingEncoding:UTF8_ENCODING];
    str::from_utf8(slice::from_raw_parts(cstr as *const u8, len)).unwrap()
}

#[cfg(not(target_os = "macos"))]
pub fn set_locale_environment() {}
