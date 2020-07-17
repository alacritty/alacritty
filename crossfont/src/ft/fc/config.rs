use foreign_types::{foreign_type, ForeignTypeRef};

use super::ffi::{FcConfig, FcConfigDestroy, FcConfigGetCurrent, FcConfigGetFonts};
use super::{FontSetRef, SetName};

foreign_type! {
    pub unsafe type Config {
        type CType = FcConfig;
        fn drop = FcConfigDestroy;
    }
}

impl Config {
    /// Get the current configuration.
    pub fn get_current() -> &'static ConfigRef {
        unsafe { ConfigRef::from_ptr(FcConfigGetCurrent()) }
    }
}

impl ConfigRef {
    /// Returns one of the two sets of fonts from the configuration as
    /// specified by `set`.
    pub fn get_fonts(&self, set: SetName) -> &FontSetRef {
        unsafe {
            let ptr = FcConfigGetFonts(self.as_ptr(), set as u32);
            FontSetRef::from_ptr(ptr)
        }
    }
}
