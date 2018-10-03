pub use self::platform::*;

#[cfg(target_os = "windows")]
#[path="windows/mod.rs"]
mod platform;
#[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]
#[path="linux/mod.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path="macos/mod.rs"]
mod platform;
#[cfg(target_os = "android")]
#[path="android/mod.rs"]
mod platform;
#[cfg(target_os = "ios")]
#[path="ios/mod.rs"]
mod platform;
#[cfg(target_os = "emscripten")]
#[path="emscripten/mod.rs"]
mod platform;

#[cfg(all(not(target_os = "ios"), not(target_os = "windows"), not(target_os = "linux"),
  not(target_os = "macos"), not(target_os = "android"), not(target_os = "dragonfly"),
  not(target_os = "freebsd"), not(target_os = "openbsd"), not(target_os = "emscripten")))]
use this_platform_is_not_supported;
