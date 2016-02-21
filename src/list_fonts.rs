use std::ffi::CStr;
use std::ptr;
use std::str;

use libc::{c_char, c_int};

use fontconfig::fontconfig::{FcConfigGetCurrent, FcConfigGetFonts, FcSetSystem};
use fontconfig::fontconfig::{FcPatternGetString};
use fontconfig::fontconfig::{FcResultMatch};
use fontconfig::fontconfig::{FcChar8};

pub fn list_font_names() -> Vec<String> {
    let mut fonts = Vec::new();
    unsafe {
        // https://www.freedesktop.org/software/fontconfig/fontconfig-devel/fcconfiggetcurrent.html
        let config = FcConfigGetCurrent(); // *mut FcConfig

        // https://www.freedesktop.org/software/fontconfig/fontconfig-devel/fcconfiggetfonts.html
        let font_set = FcConfigGetFonts(config, FcSetSystem); // *mut FcFontSet

        let nfont = (*font_set).nfont as isize;
        for i in 0..nfont {
            let font = (*font_set).fonts.offset(i); // *mut FcPattern
            let id = 0 as c_int;
            let mut fullname: *mut FcChar8 = ptr::null_mut();

            // The second parameter here (fullname) is from the "FONT PROPERTIES" table:
            // https://www.freedesktop.org/software/fontconfig/fontconfig-devel/x19.html
            let result = FcPatternGetString(*font,
                                            b"fullname\0".as_ptr() as *mut c_char,
                                            id,
                                            &mut fullname);
            if result != FcResultMatch {
                continue;
            }

            let s = str::from_utf8(CStr::from_ptr(fullname as *const c_char).to_bytes())
                        .unwrap().to_owned();
            fonts.push(s);
        }
    }

    fonts
}

#[cfg(test)]
mod tests {
    use super::list_font_names;

    #[test]
    fn list_fonts() {
        let fonts = list_font_names();
        assert!(!fonts.is_empty());

        println!("fonts: {:?}", fonts);
    }
}
