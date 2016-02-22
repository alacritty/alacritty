use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::str::from_utf8;

use libc::{c_char, c_int};

use fontconfig::fontconfig::{FcConfigGetCurrent, FcConfigGetFonts, FcSetSystem};
use fontconfig::fontconfig::{FcPatternGetString, FcPatternCreate, FcPatternAddString};
use fontconfig::fontconfig::{FcPatternGetInteger};
use fontconfig::fontconfig::{FcObjectSetCreate, FcObjectSetAdd};
use fontconfig::fontconfig::{FcResultMatch, FcFontSetList};
use fontconfig::fontconfig::{FcChar8};
use fontconfig::fontconfig::{FcFontSetDestroy, FcPatternDestroy, FcObjectSetDestroy};

unsafe fn fc_char8_to_string(fc_str: *mut FcChar8) -> String {
    from_utf8(CStr::from_ptr(fc_str as *const c_char).to_bytes()).unwrap().to_owned()
}

fn list_families() -> Vec<String> {
    let mut families = Vec::new();
    unsafe {
        // https://www.freedesktop.org/software/fontconfig/fontconfig-devel/fcconfiggetcurrent.html
        let config = FcConfigGetCurrent(); // *mut FcConfig

        // https://www.freedesktop.org/software/fontconfig/fontconfig-devel/fcconfiggetfonts.html
        let font_set = FcConfigGetFonts(config, FcSetSystem); // *mut FcFontSet

        let nfont = (*font_set).nfont as isize;
        for i in 0..nfont {
            let font = (*font_set).fonts.offset(i); // *mut FcPattern
            let id = 0 as c_int;
            let mut family: *mut FcChar8 = ptr::null_mut();
            let mut format: *mut FcChar8 = ptr::null_mut();

            let result = FcPatternGetString(*font,
                                            b"fontformat\0".as_ptr() as *mut c_char,
                                            id,
                                            &mut format);

            if result != FcResultMatch {
                continue;
            }

            let format = fc_char8_to_string(format);

            if format != "TrueType" && format != "CFF" {
                continue
            }

            let mut id = 0;
            while FcPatternGetString(*font, b"family\0".as_ptr() as *mut c_char, id, &mut family) == FcResultMatch {
                let safe_family = fc_char8_to_string(family);
                id += 1;
                families.push(safe_family);
            }
        }
    }

    families.sort();
    families.dedup();
    families
}

#[derive(Debug)]
pub struct Variant {
    style: String,
    file: PathBuf,
    index: usize,
}

impl Variant {
    #[inline]
    pub fn filepath(&self) -> &::std::path::Path {
        self.file.as_path()
    }
}

#[derive(Debug)]
pub struct Family {
    name: String,
    variants: Vec<Variant>,
}

impl Family {
    #[inline]
    pub fn name(&self) -> &str {
        &self.name[..]
    }

    #[inline]
    pub fn variants(&self) -> &[Variant] {
        &self.variants[..]
    }
}

static FILE: &'static [u8] = b"file\0";
static FAMILY: &'static [u8] = b"family\0";
static INDEX: &'static [u8] = b"index\0";
static STYLE: &'static [u8] = b"style\0";

pub fn get_family_info(family: String) -> Family {

    let mut members = Vec::new();

    unsafe {
        let config = FcConfigGetCurrent(); // *mut FcConfig
        let mut font_set = FcConfigGetFonts(config, FcSetSystem); // *mut FcFontSet

        let pattern = FcPatternCreate();
        let family_name = CString::new(&family[..]).unwrap();
        let family_name = family_name.as_ptr();

        // Add family name to pattern. Use this for searching.
        FcPatternAddString(pattern, FAMILY.as_ptr() as *mut c_char, family_name as *mut FcChar8);

        // Request filename, style, and index for each variant in family
        let object_set = FcObjectSetCreate(); // *mut FcObjectSet
        FcObjectSetAdd(object_set, FILE.as_ptr() as *mut c_char);
        FcObjectSetAdd(object_set, INDEX.as_ptr() as *mut c_char);
        FcObjectSetAdd(object_set, STYLE.as_ptr() as *mut c_char);

        let variants = FcFontSetList(config, &mut font_set, 1 /* nsets */, pattern, object_set);
        let num_variant = (*variants).nfont as isize;

        for i in 0..num_variant {
            let font = (*variants).fonts.offset(i);
            let mut file: *mut FcChar8 = ptr::null_mut();
            assert_eq!(FcPatternGetString(*font, FILE.as_ptr() as *mut c_char, 0, &mut file),
                       FcResultMatch);
            let file = fc_char8_to_string(file);

            let mut style: *mut FcChar8 = ptr::null_mut();
            assert_eq!(FcPatternGetString(*font, STYLE.as_ptr() as *mut c_char, 0, &mut style),
                       FcResultMatch);
            let style = fc_char8_to_string(style);

            let mut index = 0 as c_int;
            assert_eq!(FcPatternGetInteger(*font, INDEX.as_ptr() as *mut c_char, 0, &mut index),
                       FcResultMatch);

            members.push(Variant {
                style: style,
                file: PathBuf::from(file),
                index: index as usize,
            });
        }

        FcFontSetDestroy(variants);
        FcPatternDestroy(pattern);
        FcObjectSetDestroy(object_set);
    }

    Family {
        name: family,
        variants: members
    }
}

pub fn get_font_families() -> Vec<Family> {
    list_families().into_iter()
                   .map(|family| get_family_info(family))
                   .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn get_font_families() {
        let families = super::get_font_families();
        assert!(!families.is_empty());
    }
}
