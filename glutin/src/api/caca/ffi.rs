#![allow(non_camel_case_types)]

use libc;

pub type caca_display_t = libc::c_void;
pub type caca_canvas_t = libc::c_void;
pub type caca_dither_t = libc::c_void;

shared_library!(LibCaca, "libcaca.so.0",
    pub fn caca_create_display(cv: *mut caca_canvas_t) -> *mut caca_display_t,
    pub fn caca_free_display(dp: *mut caca_display_t) -> libc::c_int,
    pub fn caca_get_canvas(dp: *mut caca_display_t) -> *mut caca_canvas_t,
    pub fn caca_refresh_display(dp: *mut caca_display_t) -> libc::c_int,
    pub fn caca_dither_bitmap(cv: *mut caca_canvas_t, x: libc::c_int, y: libc::c_int,
                              w: libc::c_int, h: libc::c_int, d: *const caca_dither_t,
                              pixels: *const libc::c_void) -> libc::c_int,
    pub fn caca_free_dither(d: *mut caca_dither_t) -> libc::c_int,
    pub fn caca_create_dither(bpp: libc::c_int, w: libc::c_int, h: libc::c_int,
                              pitch: libc::c_int, rmask: libc::uint32_t, gmask: libc::uint32_t,
                              bmask: libc::uint32_t, amask: libc::uint32_t) -> *mut caca_dither_t,
    pub fn caca_get_canvas_width(cv: *mut caca_canvas_t) -> libc::c_int,
    pub fn caca_get_canvas_height(cv: *mut caca_canvas_t) -> libc::c_int,
);
