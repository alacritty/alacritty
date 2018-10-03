#![allow(non_camel_case_types)]

#[cfg(target_os = "windows")]
extern crate winapi;

pub use self::egl::types::EGLContext;

use libc;

pub mod egl {
    pub type khronos_utime_nanoseconds_t = super::khronos_utime_nanoseconds_t;
    pub type khronos_uint64_t = super::khronos_uint64_t;
    pub type khronos_ssize_t = super::khronos_ssize_t;
    pub type EGLNativeDisplayType = super::EGLNativeDisplayType;
    pub type EGLNativePixmapType = super::EGLNativePixmapType;
    pub type EGLNativeWindowType = super::EGLNativeWindowType;
    pub type EGLint = super::EGLint;
    pub type NativeDisplayType = super::EGLNativeDisplayType;
    pub type NativePixmapType = super::EGLNativePixmapType;
    pub type NativeWindowType = super::EGLNativeWindowType;

    include!(concat!(env!("OUT_DIR"), "/egl_bindings.rs"));
}

pub type khronos_utime_nanoseconds_t = khronos_uint64_t;
pub type khronos_uint64_t = libc::uint64_t;
pub type khronos_ssize_t = libc::c_long;
pub type EGLint = libc::int32_t;
pub type EGLNativeDisplayType = *const libc::c_void;
pub type EGLNativePixmapType = *const libc::c_void;     // FIXME: egl_native_pixmap_t instead

#[cfg(target_os = "windows")]
pub type EGLNativeWindowType = winapi::shared::windef::HWND;
#[cfg(target_os = "linux")]
pub type EGLNativeWindowType = *const libc::c_void;
#[cfg(target_os = "android")]
pub type EGLNativeWindowType = *const libc::c_void;
#[cfg(any(target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]
pub type EGLNativeWindowType = *const libc::c_void;
