#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use libc;
use std::os::raw;

#[link(name = "android")]
#[link(name = "EGL")]
#[link(name = "GLESv2")]
extern {}

/**
 * asset_manager.h
 */
pub type AAssetManager = raw::c_void;

/**
 * native_window.h
 */
pub type ANativeWindow = raw::c_void;

extern {
    pub fn ANativeWindow_getHeight(window: *const ANativeWindow) -> libc::int32_t;
    pub fn ANativeWindow_getWidth(window: *const ANativeWindow) -> libc::int32_t;
}

/**
 * native_activity.h
 */
pub type JavaVM = ();
pub type JNIEnv = ();
pub type jobject = *const libc::c_void;

pub type AInputQueue = ();  // FIXME: wrong
pub type ARect = ();  // FIXME: wrong

#[repr(C)]
pub struct ANativeActivity {
    pub callbacks: *mut ANativeActivityCallbacks,
    pub vm: *mut JavaVM,
    pub env: *mut JNIEnv,
    pub clazz: jobject,
    pub internalDataPath: *const libc::c_char,
    pub externalDataPath: *const libc::c_char,
    pub sdkVersion: libc::int32_t,
    pub instance: *mut libc::c_void,
    pub assetManager: *mut AAssetManager,
    pub obbPath: *const libc::c_char,
}

#[repr(C)]
pub struct ANativeActivityCallbacks {
    pub onStart: extern fn(*mut ANativeActivity),
    pub onResume: extern fn(*mut ANativeActivity),
    pub onSaveInstanceState: extern fn(*mut ANativeActivity, *mut libc::size_t),
    pub onPause: extern fn(*mut ANativeActivity),
    pub onStop: extern fn(*mut ANativeActivity),
    pub onDestroy: extern fn(*mut ANativeActivity),
    pub onWindowFocusChanged: extern fn(*mut ANativeActivity, libc::c_int),
    pub onNativeWindowCreated: extern fn(*mut ANativeActivity, *const ANativeWindow),
    pub onNativeWindowResized: extern fn(*mut ANativeActivity, *const ANativeWindow),
    pub onNativeWindowRedrawNeeded: extern fn(*mut ANativeActivity, *const ANativeWindow),
    pub onNativeWindowDestroyed: extern fn(*mut ANativeActivity, *const ANativeWindow),
    pub onInputQueueCreated: extern fn(*mut ANativeActivity, *mut AInputQueue),
    pub onInputQueueDestroyed: extern fn(*mut ANativeActivity, *mut AInputQueue),
    pub onContentRectChanged: extern fn(*mut ANativeActivity, *const ARect),
    pub onConfigurationChanged: extern fn(*mut ANativeActivity),
    pub onLowMemory: extern fn(*mut ANativeActivity),
}

/**
 * looper.h
 */
pub type ALooper = ();

#[link(name = "android")]
extern {
    pub fn ALooper_forThread() -> *const ALooper;
    pub fn ALooper_acquire(looper: *const ALooper);
    pub fn ALooper_release(looper: *const ALooper);
    pub fn ALooper_prepare(opts: libc::c_int) -> *const ALooper;
    pub fn ALooper_pollOnce(timeoutMillis: libc::c_int, outFd: *mut libc::c_int,
        outEvents: *mut libc::c_int, outData: *mut *mut libc::c_void) -> libc::c_int;
    pub fn ALooper_pollAll(timeoutMillis: libc::c_int, outFd: *mut libc::c_int,
        outEvents: *mut libc::c_int, outData: *mut *mut libc::c_void) -> libc::c_int;
    pub fn ALooper_wake(looper: *const ALooper);
    pub fn ALooper_addFd(looper: *const ALooper, fd: libc::c_int, ident: libc::c_int,
        events: libc::c_int, callback: ALooper_callbackFunc, data: *mut libc::c_void)
        -> libc::c_int;
    pub fn ALooper_removeFd(looper: *const ALooper, fd: libc::c_int) -> libc::c_int;
}

pub const ALOOPER_PREPARE_ALLOW_NON_CALLBACKS: libc::c_int = 1 << 0;

pub const ALOOPER_POLL_WAKE: libc::c_int = -1;
pub const ALOOPER_POLL_CALLBACK: libc::c_int = -2;
pub const ALOOPER_POLL_TIMEOUT: libc::c_int = -3;
pub const ALOOPER_POLL_ERROR: libc::c_int = -4;

pub const ALOOPER_EVENT_INPUT: libc::c_int = 1 << 0;
pub const ALOOPER_EVENT_OUTPUT: libc::c_int = 1 << 1;
pub const ALOOPER_EVENT_ERROR: libc::c_int = 1 << 2;
pub const ALOOPER_EVENT_HANGUP: libc::c_int = 1 << 3;
pub const ALOOPER_EVENT_INVALID: libc::c_int = 1 << 4;

pub type ALooper_callbackFunc = extern fn(libc::c_int, libc::c_int, *mut libc::c_void) -> libc::c_int;
