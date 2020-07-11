#![allow(clippy::let_unit_value)]

use std::env;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::slice;
use std::str;

use libc::{setlocale, LC_ALL, LC_CTYPE};
use log::debug;
use objc::runtime::{Class, Object};
use objc::{msg_send, sel, sel_impl};

const FALLBACK_LOCALE: &str = "UTF-8";

pub fn set_locale_environment() {
    let env_locale_c = CString::new("").unwrap();
    let env_locale_ptr = unsafe { setlocale(LC_ALL, env_locale_c.as_ptr()) };
    if !env_locale_ptr.is_null() {
        let env_locale = unsafe { CStr::from_ptr(env_locale_ptr).to_string_lossy() };

        // Assume `C` locale means unchanged, since it is the default anyways.
        if env_locale != "C" {
            debug!("Using environment locale: {}", env_locale);
            return;
        }
    }

    let system_locale = system_locale();

    // Set locale to system locale.
    let system_locale_c = CString::new(system_locale.clone()).expect("nul byte in system locale");
    let lc_all = unsafe { setlocale(LC_ALL, system_locale_c.as_ptr()) };

    // Check if system locale was valid or not.
    if lc_all.is_null() {
        // Use fallback locale.
        debug!("Using fallback locale: {}", FALLBACK_LOCALE);

        let fallback_locale_c = CString::new(FALLBACK_LOCALE).unwrap();
        unsafe { setlocale(LC_CTYPE, fallback_locale_c.as_ptr()) };

        env::set_var("LC_CTYPE", FALLBACK_LOCALE);
    } else {
        // Use system locale.
        debug!("Using system locale: {}", system_locale);

        env::set_var("LC_ALL", system_locale);
    }
}

/// Determine system locale based on language and country code.
fn system_locale() -> String {
    unsafe {
        let locale_class = Class::get("NSLocale").unwrap();
        let locale: *const Object = msg_send![locale_class, currentLocale];
        let _: () = msg_send![locale_class, release];

        // `localeIdentifier` returns extra metadata with the locale (including currency and
        // collator) on newer versions of macOS. This is not a valid locale, so we use
        // `languageCode` and `countryCode`, if they're available (macOS 10.12+):
        //
        // https://developer.apple.com/documentation/foundation/nslocale/1416263-localeidentifier?language=objc
        // https://developer.apple.com/documentation/foundation/nslocale/1643060-countrycode?language=objc
        // https://developer.apple.com/documentation/foundation/nslocale/1643026-languagecode?language=objc
        let is_language_code_supported: bool =
            msg_send![locale, respondsToSelector: sel!(languageCode)];
        let is_country_code_supported: bool =
            msg_send![locale, respondsToSelector: sel!(countryCode)];
        let locale_id = if is_language_code_supported && is_country_code_supported {
            let language_code: *const Object = msg_send![locale, languageCode];
            let language_code_str = nsstring_as_str(language_code).to_owned();
            let _: () = msg_send![language_code, release];

            let country_code: *const Object = msg_send![locale, countryCode];
            let country_code_str = nsstring_as_str(country_code).to_owned();
            let _: () = msg_send![country_code, release];

            format!("{}_{}.UTF-8", &language_code_str, &country_code_str)
        } else {
            let identifier: *const Object = msg_send![locale, localeIdentifier];
            let identifier_str = nsstring_as_str(identifier).to_owned();
            let _: () = msg_send![identifier, release];

            identifier_str + ".UTF-8"
        };

        let _: () = msg_send![locale, release];

        locale_id
    }
}

const UTF8_ENCODING: usize = 4;

unsafe fn nsstring_as_str<'a>(nsstring: *const Object) -> &'a str {
    let cstr: *const c_char = msg_send![nsstring, UTF8String];
    let len: usize = msg_send![nsstring, lengthOfBytesUsingEncoding: UTF8_ENCODING];
    str::from_utf8(slice::from_raw_parts(cstr as *const u8, len)).unwrap()
}
