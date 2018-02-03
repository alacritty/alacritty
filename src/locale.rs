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
    let (language_code, country_code) = unsafe {
        let locale_class = Class::get("NSLocale").unwrap();
        let locale: *const Object = msg_send![locale_class, currentLocale];
        msg_send![locale_class, release];
        let language_code: *const Object = msg_send![locale, languageCode];
        let country_code: *const Object = msg_send![locale, countryCode];
        msg_send![locale, release];
        let language_code_str = nsstring_as_str(language_code).to_owned();
        msg_send![language_code, release];
        let country_code_str = nsstring_as_str(country_code).to_owned();
        msg_send![country_code, release];
        (language_code_str, country_code_str)
    };
    let locale_id = format!("{}_{}.UTF-8", &language_code, &country_code);
    env::set_var("LANG", &locale_id);
    // env::set_var("LC_CTYPE", &locale_id);
}

const UTF8_ENCODING: usize = 4;

unsafe fn nsstring_as_str<'a>(nsstring: *const Object) -> &'a str {
    let cstr: *const c_char = msg_send![nsstring, UTF8String];
    let len: usize = msg_send![nsstring, lengthOfBytesUsingEncoding: UTF8_ENCODING];
    str::from_utf8(slice::from_raw_parts(cstr as *const u8, len)).unwrap()
}

#[cfg(not(target_os = "macos"))]
pub fn set_locale_environment() {}
