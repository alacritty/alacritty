#![cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]
#![allow(dead_code)]

use std::os::raw::{c_void, c_char, c_int};

pub const RTLD_LAZY: c_int = 0x001;
pub const RTLD_NOW: c_int = 0x002;

#[link="dl"]
extern {
    pub fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    pub fn dlerror() -> *mut c_char;
    pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    pub fn dlclose(handle: *mut c_void) -> c_int;
}
