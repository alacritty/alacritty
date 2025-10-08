#![allow(clippy::let_unit_value)]

use std::ffi::{CStr, CString};
use std::{env, str};

use libc::{LC_ALL, LC_CTYPE, setlocale};
use log::debug;
use objc2::sel;
use objc2_foundation::{NSLocale, NSObjectProtocol};

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

        unsafe { env::set_var("LC_CTYPE", FALLBACK_LOCALE) };
    } else {
        // Use system locale.
        debug!("Using system locale: {}", system_locale);

        unsafe { env::set_var("LC_ALL", system_locale) };
    }
}

/// Determine system locale based on language and country code.
fn system_locale() -> String {
    let locale = NSLocale::currentLocale();

    // `localeIdentifier` returns extra metadata with the locale (including currency and
    // collator) on newer versions of macOS. This is not a valid locale, so we use
    // `languageCode` and `countryCode`, if they're available (macOS 10.12+):
    //
    // https://developer.apple.com/documentation/foundation/nslocale/1416263-localeidentifier?language=objc
    // https://developer.apple.com/documentation/foundation/nslocale/1643060-countrycode?language=objc
    // https://developer.apple.com/documentation/foundation/nslocale/1643026-languagecode?language=objc
    let is_language_code_supported: bool = locale.respondsToSelector(sel!(languageCode));
    let is_country_code_supported: bool = locale.respondsToSelector(sel!(countryCode));
    if is_language_code_supported && is_country_code_supported {
        let language_code = locale.languageCode();
        #[allow(deprecated)]
        if let Some(country_code) = locale.countryCode() {
            format!("{}_{}.UTF-8", language_code, country_code)
        } else {
            // Fall back to en_US in case the country code is not available.
            "en_US.UTF-8".into()
        }
    } else {
        locale.localeIdentifier().to_string() + ".UTF-8"
    }
}
