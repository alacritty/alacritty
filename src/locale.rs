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
#![cfg_attr(feature = "cargo-clippy", allow(let_unit_value))]
#![cfg(target_os = "macos")]
use libc::{LC_CTYPE, setlocale};
use std::ffi::{CString, CStr};
use std::os::raw::c_char;
use std::ptr::null;
use std::slice;
use std::str;
use std::env;

use objc::runtime::{Class, Object};

pub fn set_locale_environment() {
    let locale_id = unsafe {
        let locale_class = Class::get("NSLocale").unwrap();
        let locale: *const Object = msg_send![locale_class, currentLocale];
        let _ : () = msg_send![locale_class, release];
        // `localeIdentifier` returns extra metadata with the locale (including currency and
        // collator) on newer versions of macOS. This is not a valid locale, so we use
        // `languageCode` and `countryCode`, if they're available (macOS 10.12+):
        // https://developer.apple.com/documentation/foundation/nslocale/1416263-localeidentifier?language=objc
        // https://developer.apple.com/documentation/foundation/nslocale/1643060-countrycode?language=objc
        // https://developer.apple.com/documentation/foundation/nslocale/1643026-languagecode?language=objc
        let is_language_code_supported: bool = msg_send![locale, respondsToSelector:sel!(languageCode)];
        let is_country_code_supported: bool = msg_send![locale, respondsToSelector:sel!(countryCode)];
        let locale_id = if is_language_code_supported && is_country_code_supported {
            let language_code: *const Object = msg_send![locale, languageCode];
            let country_code: *const Object = msg_send![locale, countryCode];
            let language_code_str = nsstring_as_str(language_code).to_owned();
            let _ : () = msg_send![language_code, release];
            let country_code_str = nsstring_as_str(country_code).to_owned();
            let _ : () = msg_send![country_code, release];
            format!("{}_{}.UTF-8", &language_code_str, &country_code_str)
        } else {
            let identifier: *const Object = msg_send![locale, localeIdentifier];
            let identifier_str = nsstring_as_str(identifier).to_owned();
            let _ : () = msg_send![identifier, release];
            identifier_str + ".UTF-8"
        };
        let _ : () = msg_send![locale, release];
        locale_id
    };
    // check if locale_id is valid
    let locale_c_str = CString::new(locale_id.to_owned()).unwrap();
    let locale_ptr = locale_c_str.as_ptr();
    let locale_id = unsafe {
        // save a copy of original setting
        let original = setlocale(LC_CTYPE, null());
        let saved_original = if original.is_null() {
            CString::new("").unwrap()
        } else {
            CStr::from_ptr(original).to_owned()
        };
        // try setting `locale_id`
        let modified = setlocale(LC_CTYPE, locale_ptr);
        let result = if modified.is_null() {
            String::new()
        } else {
            locale_id
        };
        // restore original setting
        setlocale(LC_CTYPE, saved_original.as_ptr());
        result
    };

    env::set_var("LANG", &locale_id);
}

const UTF8_ENCODING: usize = 4;

unsafe fn nsstring_as_str<'a>(nsstring: *const Object) -> &'a str {
    let cstr: *const c_char = msg_send![nsstring, UTF8String];
    let len: usize = msg_send![nsstring, lengthOfBytesUsingEncoding: UTF8_ENCODING];
    str::from_utf8(slice::from_raw_parts(cstr as *const u8, len)).unwrap()
}

#[cfg(not(target_os = "macos"))]
pub fn set_locale_environment() {}
