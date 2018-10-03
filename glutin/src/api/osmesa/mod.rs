#![cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]

extern crate osmesa_sys;

use Api;
use ContextError;
use CreationError;
use GlAttributes;
use GlProfile;
use GlRequest;
use PixelFormat;
use PixelFormatRequirements;
use Robustness;
use libc;

use std::error::Error;
use std::ffi::CString;
use std::fmt::{Debug, Display, Error as FormatError, Formatter};
use std::{mem, ptr};
use std::os::raw::c_void;

pub mod ffi {
    pub use super::osmesa_sys::OSMesaContext;
}

pub struct OsMesaContext {
    context: osmesa_sys::OSMesaContext,
    buffer: Vec<u32>,
    width: u32,
    height: u32,
}

#[derive(Debug)]
struct NoEsOrWebGlSupported;

impl Display for NoEsOrWebGlSupported {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FormatError> {
        write!(f, "OsMesa only works with desktop OpenGL; OpenGL ES or WebGL are not supported")
    }
}

impl Error for NoEsOrWebGlSupported {
    fn description(&self) -> &str {
        "OsMesa only works with desktop OpenGL"
    }
}

#[derive(Debug)]
struct LoadingError(String);

impl LoadingError {
    fn new<D: Debug>(d: D) -> Self {
        LoadingError(format!("{:?}", d))
    }
}

impl Display for LoadingError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FormatError> {
        write!(f, "Failed to load OsMesa dynamic library: {}", self.0)
    }
}

impl Error for LoadingError {
    fn description(&self) -> &str {
        "The library or a symbol of it could not be loaded"
    }
}

impl OsMesaContext {
    pub fn new(
        dimensions: (u32, u32),
        _pf_reqs: &PixelFormatRequirements,
        opengl: &GlAttributes<&OsMesaContext>,
    ) -> Result<OsMesaContext, CreationError>
    {
        osmesa_sys::OsMesa::try_loading()
            .map_err(LoadingError::new)
            .map_err(|e| CreationError::NoBackendAvailable(Box::new(e)))?;

        if opengl.sharing.is_some() { panic!("Context sharing not possible with OsMesa") }

        match opengl.robustness {
            Robustness::RobustNoResetNotification | Robustness::RobustLoseContextOnReset => {
                return Err(CreationError::RobustnessNotSupported.into());
            },
            _ => ()
        }

        // TODO: use `pf_reqs` for the format

        let mut attribs = Vec::new();

        if let Some(profile) = opengl.profile {
            attribs.push(osmesa_sys::OSMESA_PROFILE);

            match profile {
                GlProfile::Compatibility => {
                    attribs.push(osmesa_sys::OSMESA_COMPAT_PROFILE);
                }
                GlProfile::Core => {
                    attribs.push(osmesa_sys::OSMESA_CORE_PROFILE);
                }
            }
        }

        match opengl.version {
            GlRequest::Latest => {},
            GlRequest::Specific(Api::OpenGl, (major, minor)) => {
                attribs.push(osmesa_sys::OSMESA_CONTEXT_MAJOR_VERSION);
                attribs.push(major as libc::c_int);
                attribs.push(osmesa_sys::OSMESA_CONTEXT_MINOR_VERSION);
                attribs.push(minor as libc::c_int);
            },
            GlRequest::Specific(Api::OpenGlEs, _) | GlRequest::Specific(Api::WebGl, _) => {
                return Err(CreationError::NoBackendAvailable(Box::new(NoEsOrWebGlSupported)));
            },
            GlRequest::GlThenGles { opengl_version: (major, minor), .. } => {
                attribs.push(osmesa_sys::OSMESA_CONTEXT_MAJOR_VERSION);
                attribs.push(major as libc::c_int);
                attribs.push(osmesa_sys::OSMESA_CONTEXT_MINOR_VERSION);
                attribs.push(minor as libc::c_int);
            },
        }

        // attribs array must be NULL terminated.
        attribs.push(0);

        Ok(OsMesaContext {
            width: dimensions.0,
            height: dimensions.1,
            buffer: ::std::iter::repeat(unsafe { mem::uninitialized() })
                .take((dimensions.0 * dimensions.1) as usize).collect(),
            context: unsafe {
                let ctxt = osmesa_sys::OSMesaCreateContextAttribs(attribs.as_ptr(), ptr::null_mut());
                if ctxt.is_null() {
                    return Err(CreationError::OsError("OSMesaCreateContextAttribs failed".to_string()));
                }
                ctxt
            }
        })
    }

    #[inline]
    pub fn get_framebuffer(&self) -> &[u32] {
        &self.buffer
    }

    #[inline]
    pub fn get_dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[inline]
    pub unsafe fn make_current(&self) -> Result<(), ContextError> {
        let ret = osmesa_sys::OSMesaMakeCurrent(self.context, self.buffer.as_ptr()
                                                as *mut _, 0x1401, self.width
                                                as libc::c_int, self.height as libc::c_int);

        // an error can only happen in case of invalid parameter, which would indicate a bug
        // in glutin
        if ret == 0 {
            panic!("OSMesaMakeCurrent failed");
        }

        Ok(())
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        unsafe { osmesa_sys::OSMesaGetCurrentContext() == self.context }
    }

    #[inline]
    pub fn get_proc_address(&self, addr: &str) -> *const () {
        unsafe {
            let c_str = CString::new(addr.as_bytes().to_vec()).unwrap();
            mem::transmute(osmesa_sys::OSMesaGetProcAddress(mem::transmute(c_str.as_ptr())))
        }
    }

    #[inline]
    pub fn get_api(&self) -> Api {
        Api::OpenGl
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        unimplemented!();
    }

    #[inline]
    pub unsafe fn raw_handle(&self) -> *mut c_void {
        self.context as *mut _
    }
}

impl Drop for OsMesaContext {
    #[inline]
    fn drop(&mut self) {
        unsafe { osmesa_sys::OSMesaDestroyContext(self.context) }
    }
}

unsafe impl Send for OsMesaContext {}
unsafe impl Sync for OsMesaContext {}
