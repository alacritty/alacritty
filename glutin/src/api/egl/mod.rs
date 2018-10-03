#![cfg(any(target_os = "windows", target_os = "linux", target_os = "android",
           target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]
#![allow(unused_variables)]

use ContextError;
use CreationError;
use GlAttributes;
use GlRequest;
use PixelFormat;
use PixelFormatRequirements;
use ReleaseBehavior;
use Robustness;
use Api;

use std::ffi::{CStr, CString};
use std::os::raw::{c_void, c_int};
use std::{mem, ptr};
use std::cell::Cell;

pub mod ffi;

/// Specifies the type of display passed as `native_display`.
#[allow(dead_code)]
pub enum NativeDisplay {
    /// `None` means `EGL_DEFAULT_DISPLAY`.
    X11(Option<ffi::EGLNativeDisplayType>),
    /// `None` means `EGL_DEFAULT_DISPLAY`.
    Gbm(Option<ffi::EGLNativeDisplayType>),
    /// `None` means `EGL_DEFAULT_DISPLAY`.
    Wayland(Option<ffi::EGLNativeDisplayType>),
    /// `EGL_DEFAULT_DISPLAY` is mandatory for Android.
    Android,
    // TODO: should be `EGLDeviceEXT`
    Device(ffi::EGLNativeDisplayType),
    /// Don't specify any display type. Useful on windows. `None` means `EGL_DEFAULT_DISPLAY`.
    Other(Option<ffi::EGLNativeDisplayType>),
}

pub struct Context {
    egl: ffi::egl::Egl,
    display: ffi::egl::types::EGLDisplay,
    context: ffi::egl::types::EGLContext,
    surface: Cell<ffi::egl::types::EGLSurface>,
    api: Api,
    pixel_format: PixelFormat,
    #[cfg(target_os = "android")]
    config_id: ffi::egl::types::EGLConfig,
}

#[cfg(target_os = "android")]
#[inline]
fn get_native_display(egl: &ffi::egl::Egl,
                      native_display: NativeDisplay) -> *const c_void {
    unsafe { egl.GetDisplay(ffi::egl::DEFAULT_DISPLAY as *mut _) }
}

#[cfg(not(target_os = "android"))]
fn get_native_display(egl: &ffi::egl::Egl,
                      native_display: NativeDisplay) -> *const c_void {
    // the first step is to query the list of extensions without any display, if supported
    let dp_extensions = unsafe {
        let p = egl.QueryString(ffi::egl::NO_DISPLAY, ffi::egl::EXTENSIONS as i32);

        // this possibility is available only with EGL 1.5 or EGL_EXT_platform_base, otherwise
        // `eglQueryString` returns an error
        if p.is_null() {
            vec![]
        } else {
            let p = CStr::from_ptr(p);
            let list = String::from_utf8(p.to_bytes().to_vec()).unwrap_or_else(|_| format!(""));
            list.split(' ').map(|e| e.to_string()).collect::<Vec<_>>()
        }
    };

    let has_dp_extension = |e: &str| dp_extensions.iter().find(|s| s == &e).is_some();

    match native_display {
        // Note: Some EGL implementations are missing the `eglGetPlatformDisplay(EXT)` symbol
        //       despite reporting `EGL_EXT_platform_base`. I'm pretty sure this is a bug.
        //       Therefore we detect whether the symbol is loaded in addition to checking for
        //       extensions.
        NativeDisplay::X11(display) if has_dp_extension("EGL_KHR_platform_x11") &&
                                       egl.GetPlatformDisplay.is_loaded() =>
        {
            let d = display.unwrap_or(ffi::egl::DEFAULT_DISPLAY as *const _);
            // TODO: `PLATFORM_X11_SCREEN_KHR`
            unsafe { egl.GetPlatformDisplay(ffi::egl::PLATFORM_X11_KHR, d as *mut _,
                                            ptr::null()) }
        },

        NativeDisplay::X11(display) if has_dp_extension("EGL_EXT_platform_x11") &&
                                       egl.GetPlatformDisplayEXT.is_loaded() =>
        {
            let d = display.unwrap_or(ffi::egl::DEFAULT_DISPLAY as *const _);
            // TODO: `PLATFORM_X11_SCREEN_EXT`
            unsafe { egl.GetPlatformDisplayEXT(ffi::egl::PLATFORM_X11_EXT, d as *mut _,
                                               ptr::null()) }
        },

        NativeDisplay::Gbm(display) if has_dp_extension("EGL_KHR_platform_gbm") &&
                                       egl.GetPlatformDisplay.is_loaded() =>
        {
            let d = display.unwrap_or(ffi::egl::DEFAULT_DISPLAY as *const _);
            unsafe { egl.GetPlatformDisplay(ffi::egl::PLATFORM_GBM_KHR, d as *mut _,
                                            ptr::null()) }
        },

        NativeDisplay::Gbm(display) if has_dp_extension("EGL_MESA_platform_gbm") &&
                                       egl.GetPlatformDisplayEXT.is_loaded() =>
        {
            let d = display.unwrap_or(ffi::egl::DEFAULT_DISPLAY as *const _);
            unsafe { egl.GetPlatformDisplayEXT(ffi::egl::PLATFORM_GBM_KHR, d as *mut _,
                                               ptr::null()) }
        },

        NativeDisplay::Wayland(display) if has_dp_extension("EGL_KHR_platform_wayland") &&
                                           egl.GetPlatformDisplay.is_loaded() =>
        {
            let d = display.unwrap_or(ffi::egl::DEFAULT_DISPLAY as *const _);
            unsafe { egl.GetPlatformDisplay(ffi::egl::PLATFORM_WAYLAND_KHR, d as *mut _,
                                            ptr::null()) }
        },

        NativeDisplay::Wayland(display) if has_dp_extension("EGL_EXT_platform_wayland") &&
                                           egl.GetPlatformDisplayEXT.is_loaded() =>
        {
            let d = display.unwrap_or(ffi::egl::DEFAULT_DISPLAY as *const _);
            unsafe { egl.GetPlatformDisplayEXT(ffi::egl::PLATFORM_WAYLAND_EXT, d as *mut _,
                                               ptr::null()) }
        },

        // TODO: This will never be reached right now, as the android egl bindings
        // use the static generator, so can't rely on GetPlatformDisplay(EXT).
        NativeDisplay::Android if has_dp_extension("EGL_KHR_platform_android") &&
                                  egl.GetPlatformDisplay.is_loaded() =>
        {
            unsafe { egl.GetPlatformDisplay(ffi::egl::PLATFORM_ANDROID_KHR,
                                            ffi::egl::DEFAULT_DISPLAY as *mut _, ptr::null()) }
        },

        NativeDisplay::Device(display) if has_dp_extension("EGL_EXT_platform_device") &&
                                          egl.GetPlatformDisplay.is_loaded() =>
        {
            unsafe { egl.GetPlatformDisplay(ffi::egl::PLATFORM_DEVICE_EXT, display as *mut _,
                                            ptr::null()) }
        },

        NativeDisplay::X11(Some(display)) | NativeDisplay::Gbm(Some(display)) |
        NativeDisplay::Wayland(Some(display)) | NativeDisplay::Device(display) |
        NativeDisplay::Other(Some(display)) => {
            unsafe { egl.GetDisplay(display as *mut _) }
        }

        NativeDisplay::X11(None) | NativeDisplay::Gbm(None) | NativeDisplay::Wayland(None) |
        NativeDisplay::Android | NativeDisplay::Other(None) => {
            unsafe { egl.GetDisplay(ffi::egl::DEFAULT_DISPLAY as *mut _) }
        },
    }
}

impl Context {
    /// Start building an EGL context.
    ///
    /// This function initializes some things and chooses the pixel format.
    ///
    /// To finish the process, you must call `.finish(window)` on the `ContextPrototype`.
    pub fn new<'a>(
        egl: ffi::egl::Egl,
        pf_reqs: &PixelFormatRequirements,
        opengl: &'a GlAttributes<&'a Context>,
        native_display: NativeDisplay,
    ) -> Result<ContextPrototype<'a>, CreationError>
    {
        if opengl.sharing.is_some() {
            unimplemented!()
        }

        // calling `eglGetDisplay` or equivalent
        let display = get_native_display(&egl, native_display);

        if display.is_null() {
            return Err(CreationError::OsError("Could not create EGL display object".to_string()));
        }

        let egl_version = unsafe {
            let mut major: ffi::egl::types::EGLint = mem::uninitialized();
            let mut minor: ffi::egl::types::EGLint = mem::uninitialized();

            if egl.Initialize(display, &mut major, &mut minor) == 0 {
                return Err(CreationError::OsError(format!("eglInitialize failed")))
            }

            (major, minor)
        };

        // the list of extensions supported by the client once initialized is different from the
        // list of extensions obtained earlier
        let extensions = if egl_version >= (1, 2) {
            let p = unsafe { CStr::from_ptr(egl.QueryString(display, ffi::egl::EXTENSIONS as i32)) };
            let list = String::from_utf8(p.to_bytes().to_vec()).unwrap_or_else(|_| format!(""));
            list.split(' ').map(|e| e.to_string()).collect::<Vec<_>>()

        } else {
            vec![]
        };

        // binding the right API and choosing the version
        let (version, api) = unsafe {
            match opengl.version {
                GlRequest::Latest => {
                    if egl_version >= (1, 4) {
                        if egl.BindAPI(ffi::egl::OPENGL_API) != 0 {
                            (None, Api::OpenGl)
                        } else if egl.BindAPI(ffi::egl::OPENGL_ES_API) != 0 {
                            (None, Api::OpenGlEs)
                        } else {
                            return Err(CreationError::OpenGlVersionNotSupported);
                        }
                    } else {
                        (None, Api::OpenGlEs)
                    }
                },
                GlRequest::Specific(Api::OpenGlEs, version) => {
                    if egl_version >= (1, 2) {
                        if egl.BindAPI(ffi::egl::OPENGL_ES_API) == 0 {
                            return Err(CreationError::OpenGlVersionNotSupported);
                        }
                    }
                    (Some(version), Api::OpenGlEs)
                },
                GlRequest::Specific(Api::OpenGl, version) => {
                    if egl_version < (1, 4) {
                        return Err(CreationError::OpenGlVersionNotSupported);
                    }
                    if egl.BindAPI(ffi::egl::OPENGL_API) == 0 {
                        return Err(CreationError::OpenGlVersionNotSupported);
                    }
                    (Some(version), Api::OpenGl)
                },
                GlRequest::Specific(_, _) => return Err(CreationError::OpenGlVersionNotSupported),
                GlRequest::GlThenGles { opengles_version, opengl_version } => {
                    if egl_version >= (1, 4) {
                        if egl.BindAPI(ffi::egl::OPENGL_API) != 0 {
                            (Some(opengl_version), Api::OpenGl)
                        } else if egl.BindAPI(ffi::egl::OPENGL_ES_API) != 0 {
                            (Some(opengles_version), Api::OpenGlEs)
                        } else {
                            return Err(CreationError::OpenGlVersionNotSupported);
                        }
                    } else {
                        (Some(opengles_version), Api::OpenGlEs)
                    }
                },
            }
        };

        let (config_id, pixel_format) = unsafe {
            choose_fbconfig(&egl, display, &egl_version, api, version, pf_reqs)?
        };

        Ok(ContextPrototype {
            opengl: opengl,
            egl: egl,
            display: display,
            egl_version: egl_version,
            extensions: extensions,
            api: api,
            version: version,
            config_id: config_id,
            pixel_format: pixel_format,
        })
    }

    pub unsafe fn make_current(&self) -> Result<(), ContextError> {
        let ret = self.egl.MakeCurrent(self.display, self.surface.get(), self.surface.get(), self.context);

        if ret == 0 {
            match self.egl.GetError() as u32 {
                ffi::egl::CONTEXT_LOST => return Err(ContextError::ContextLost),
                err => panic!("eglMakeCurrent failed (eglGetError returned 0x{:x})", err)
            }

        } else {
            Ok(())
        }
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        unsafe { self.egl.GetCurrentContext() == self.context }
    }

    pub fn get_proc_address(&self, addr: &str) -> *const () {
        let addr = CString::new(addr.as_bytes()).unwrap();
        let addr = addr.as_ptr();
        unsafe {
            self.egl.GetProcAddress(addr) as *const _
        }
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<(), ContextError> {
        if self.surface.get() == ffi::egl::NO_SURFACE {
            return Err(ContextError::ContextLost);
        }

        let ret = unsafe {
            self.egl.SwapBuffers(self.display, self.surface.get())
        };

        if ret == 0 {
            match unsafe { self.egl.GetError() } as u32 {
                ffi::egl::CONTEXT_LOST => return Err(ContextError::ContextLost),
                err => panic!("eglSwapBuffers failed (eglGetError returned 0x{:x})", err)
            }

        } else {
            Ok(())
        }
    }

    #[inline]
    pub fn get_api(&self) -> Api {
        self.api
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        self.pixel_format.clone()
    }

    #[inline]
    pub unsafe fn raw_handle(&self) -> ffi::egl::types::EGLContext {
        self.context
    }

    // Handle Android Life Cycle.
    // Android has started the activity or sent it to foreground.
    // Create a new surface and attach it to the recreated ANativeWindow.
    // Restore the EGLContext.
    #[cfg(target_os = "android")]
    pub unsafe fn on_surface_created(&self, native_window: ffi::EGLNativeWindowType) {
        if self.surface.get() != ffi::egl::NO_SURFACE {
            return;
        }
        self.surface.set(self.egl.CreateWindowSurface(self.display, self.config_id, native_window, ptr::null()));
        if self.surface.get().is_null() {
            panic!("on_surface_created: eglCreateWindowSurface failed")
        }
        let ret = self.egl.MakeCurrent(self.display, self.surface.get(), self.surface.get(), self.context);
        if ret == 0 {
            panic!("on_surface_created: eglMakeCurrent failed");
        }
    }

    // Handle Android Life Cycle.
    // Android has stopped the activity or sent it to background.
    // Release the surface attached to the destroyed ANativeWindow.
    // The EGLContext is not destroyed so it can be restored later.
    #[cfg(target_os = "android")]
    pub unsafe fn on_surface_destroyed(&self) {
        if self.surface.get() == ffi::egl::NO_SURFACE {
            return;
        }
        let ret = self.egl.MakeCurrent(self.display, ffi::egl::NO_SURFACE, ffi::egl::NO_SURFACE, ffi::egl::NO_CONTEXT);
        if ret == 0 {
            panic!("on_surface_destroyed: eglMakeCurrent failed");
        }

        self.egl.DestroySurface(self.display, self.surface.get());
        self.surface.set(ffi::egl::NO_SURFACE);
    }
}

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            // we don't call MakeCurrent(0, 0) because we are not sure that the context
            // is still the current one
            self.egl.DestroyContext(self.display, self.context);
            self.egl.DestroySurface(self.display, self.surface.get());
            self.egl.Terminate(self.display);
        }
    }
}

pub struct ContextPrototype<'a> {
    opengl: &'a GlAttributes<&'a Context>,
    egl: ffi::egl::Egl,
    display: ffi::egl::types::EGLDisplay,
    egl_version: (ffi::egl::types::EGLint, ffi::egl::types::EGLint),
    extensions: Vec<String>,
    api: Api,
    version: Option<(u8, u8)>,
    config_id: ffi::egl::types::EGLConfig,
    pixel_format: PixelFormat,
}

impl<'a> ContextPrototype<'a> {
    pub fn get_native_visual_id(&self) -> ffi::egl::types::EGLint {
        let mut value = unsafe { mem::uninitialized() };
        let ret = unsafe { self.egl.GetConfigAttrib(self.display, self.config_id,
                                                    ffi::egl::NATIVE_VISUAL_ID
                                                    as ffi::egl::types::EGLint, &mut value) };
        if ret == 0 { panic!("eglGetConfigAttrib failed") };
        value
    }

    pub fn finish(self, native_window: ffi::EGLNativeWindowType)
                  -> Result<Context, CreationError>
    {
        let surface = unsafe {
            let surface = self.egl.CreateWindowSurface(self.display, self.config_id, native_window,
                                                       ptr::null());
            if surface.is_null() {
                return Err(CreationError::OsError(format!("eglCreateWindowSurface failed")))
            }
            surface
        };

        self.finish_impl(surface)
    }

    pub fn finish_pbuffer(self, dimensions: (u32, u32)) -> Result<Context, CreationError> {
        let attrs = &[
            ffi::egl::WIDTH as c_int, dimensions.0 as c_int,
            ffi::egl::HEIGHT as c_int, dimensions.1 as c_int,
            ffi::egl::NONE as c_int,
        ];

        let surface = unsafe {
            let surface = self.egl.CreatePbufferSurface(self.display, self.config_id,
                                                        attrs.as_ptr());
            if surface.is_null() {
                return Err(CreationError::OsError(format!("eglCreatePbufferSurface failed")))
            }
            surface
        };

        self.finish_impl(surface)
    }

    fn finish_impl(self, surface: ffi::egl::types::EGLSurface)
                   -> Result<Context, CreationError>
    {
        let context = unsafe {
            if let Some(version) = self.version {
                create_context(&self.egl, self.display, &self.egl_version,
                                    &self.extensions, self.api, version, self.config_id,
                                    self.opengl.debug, self.opengl.robustness)?

            } else if self.api == Api::OpenGlEs {
                if let Ok(ctxt) = create_context(&self.egl, self.display, &self.egl_version,
                                                 &self.extensions, self.api, (2, 0), self.config_id,
                                                 self.opengl.debug, self.opengl.robustness)
                {
                    ctxt
                } else if let Ok(ctxt) = create_context(&self.egl, self.display, &self.egl_version,
                                                        &self.extensions, self.api, (1, 0),
                                                        self.config_id, self.opengl.debug,
                                                        self.opengl.robustness)
                {
                    ctxt
                } else {
                    return Err(CreationError::OpenGlVersionNotSupported);
                }

            } else {
                if let Ok(ctxt) = create_context(&self.egl, self.display, &self.egl_version,
                                                 &self.extensions, self.api, (3, 2), self.config_id,
                                                 self.opengl.debug, self.opengl.robustness)
                {
                    ctxt
                } else if let Ok(ctxt) = create_context(&self.egl, self.display, &self.egl_version,
                                                        &self.extensions, self.api, (3, 1),
                                                        self.config_id, self.opengl.debug,
                                                        self.opengl.robustness)
                {
                    ctxt
                } else if let Ok(ctxt) = create_context(&self.egl, self.display, &self.egl_version,
                                                        &self.extensions, self.api, (1, 0),
                                                        self.config_id, self.opengl.debug,
                                                        self.opengl.robustness)
                {
                    ctxt
                } else {
                    return Err(CreationError::OpenGlVersionNotSupported);
                }
            }
        };

        Ok(Context {
            egl: self.egl,
            display: self.display,
            context: context,
            surface: Cell::new(surface),
            api: self.api,
            pixel_format: self.pixel_format,
            #[cfg(target_os = "android")]
            config_id: self.config_id,
        })
    }
}

unsafe fn choose_fbconfig(egl: &ffi::egl::Egl, display: ffi::egl::types::EGLDisplay,
                          egl_version: &(ffi::egl::types::EGLint, ffi::egl::types::EGLint),
                          api: Api, version: Option<(u8, u8)>, reqs: &PixelFormatRequirements)
                          -> Result<(ffi::egl::types::EGLConfig, PixelFormat), CreationError>
{
    let descriptor = {
        let mut out: Vec<c_int> = Vec::with_capacity(37);

        if egl_version >= &(1, 2) {
            out.push(ffi::egl::COLOR_BUFFER_TYPE as c_int);
            out.push(ffi::egl::RGB_BUFFER as c_int);
        }

        out.push(ffi::egl::SURFACE_TYPE as c_int);
        // TODO: Some versions of Mesa report a BAD_ATTRIBUTE error
        // if we ask for PBUFFER_BIT as well as WINDOW_BIT
        out.push((ffi::egl::WINDOW_BIT) as c_int);

        match (api, version) {
            (Api::OpenGlEs, Some((3, _))) => {
                if egl_version < &(1, 3) { return Err(CreationError::NoAvailablePixelFormat); }
                out.push(ffi::egl::RENDERABLE_TYPE as c_int);
                out.push(ffi::egl::OPENGL_ES3_BIT as c_int);
                out.push(ffi::egl::CONFORMANT as c_int);
                out.push(ffi::egl::OPENGL_ES3_BIT as c_int);
            },
            (Api::OpenGlEs, Some((2, _))) => {
                if egl_version < &(1, 3) { return Err(CreationError::NoAvailablePixelFormat); }
                out.push(ffi::egl::RENDERABLE_TYPE as c_int);
                out.push(ffi::egl::OPENGL_ES2_BIT as c_int);
                out.push(ffi::egl::CONFORMANT as c_int);
                out.push(ffi::egl::OPENGL_ES2_BIT as c_int);
            },
            (Api::OpenGlEs, Some((1, _))) => {
                if egl_version >= &(1, 3) {
                    out.push(ffi::egl::RENDERABLE_TYPE as c_int);
                    out.push(ffi::egl::OPENGL_ES_BIT as c_int);
                    out.push(ffi::egl::CONFORMANT as c_int);
                    out.push(ffi::egl::OPENGL_ES_BIT as c_int);
                }
            },
            (Api::OpenGlEs, _) => unimplemented!(),
            (Api::OpenGl, _) => {
                if egl_version < &(1, 3) { return Err(CreationError::NoAvailablePixelFormat); }
                out.push(ffi::egl::RENDERABLE_TYPE as c_int);
                out.push(ffi::egl::OPENGL_BIT as c_int);
                out.push(ffi::egl::CONFORMANT as c_int);
                out.push(ffi::egl::OPENGL_BIT as c_int);
            },
            (_, _) => unimplemented!(),
        };

        if let Some(hardware_accelerated) = reqs.hardware_accelerated {
            out.push(ffi::egl::CONFIG_CAVEAT as c_int);
            out.push(if hardware_accelerated {
                ffi::egl::NONE as c_int
            } else {
                ffi::egl::SLOW_CONFIG as c_int
            });
        }

        if let Some(color) = reqs.color_bits {
            out.push(ffi::egl::RED_SIZE as c_int);
            out.push((color / 3) as c_int);
            out.push(ffi::egl::GREEN_SIZE as c_int);
            out.push((color / 3 + if color % 3 != 0 { 1 } else { 0 }) as c_int);
            out.push(ffi::egl::BLUE_SIZE as c_int);
            out.push((color / 3 + if color % 3 == 2 { 1 } else { 0 }) as c_int);
        }

        if let Some(alpha) = reqs.alpha_bits {
            out.push(ffi::egl::ALPHA_SIZE as c_int);
            out.push(alpha as c_int);
        }

        if let Some(depth) = reqs.depth_bits {
            out.push(ffi::egl::DEPTH_SIZE as c_int);
            out.push(depth as c_int);
        }

        if let Some(stencil) = reqs.stencil_bits {
            out.push(ffi::egl::STENCIL_SIZE as c_int);
            out.push(stencil as c_int);
        }

        if let Some(true) = reqs.double_buffer {
            return Err(CreationError::NoAvailablePixelFormat);
        }

        if let Some(multisampling) = reqs.multisampling {
            out.push(ffi::egl::SAMPLES as c_int);
            out.push(multisampling as c_int);
        }

        if reqs.stereoscopy {
            return Err(CreationError::NoAvailablePixelFormat);
        }

        // FIXME: srgb is not taken into account

        match reqs.release_behavior {
            ReleaseBehavior::Flush => (),
            ReleaseBehavior::None => {
                // TODO: with EGL you need to manually set the behavior
                unimplemented!()
            },
        }

        out.push(ffi::egl::NONE as c_int);
        out
    };

    // calling `eglChooseConfig`
    let mut config_id = mem::uninitialized();
    let mut num_configs = mem::uninitialized();
    if egl.ChooseConfig(display, descriptor.as_ptr(), &mut config_id, 1, &mut num_configs) == 0 {
        return Err(CreationError::OsError(format!("eglChooseConfig failed")));
    }
    if num_configs == 0 {
        return Err(CreationError::NoAvailablePixelFormat);
    }

    // analyzing each config
    macro_rules! attrib {
        ($egl:expr, $display:expr, $config:expr, $attr:expr) => (
            {
                let mut value = mem::uninitialized();
                let res = $egl.GetConfigAttrib($display, $config,
                                               $attr as ffi::egl::types::EGLint, &mut value);
                if res == 0 {
                    return Err(CreationError::OsError(format!("eglGetConfigAttrib failed")));
                }
                value
            }
        )
    };

    let desc = PixelFormat {
        hardware_accelerated: attrib!(egl, display, config_id, ffi::egl::CONFIG_CAVEAT)
                                      != ffi::egl::SLOW_CONFIG as i32,
        color_bits: attrib!(egl, display, config_id, ffi::egl::RED_SIZE) as u8 +
                    attrib!(egl, display, config_id, ffi::egl::BLUE_SIZE) as u8 +
                    attrib!(egl, display, config_id, ffi::egl::GREEN_SIZE) as u8,
        alpha_bits: attrib!(egl, display, config_id, ffi::egl::ALPHA_SIZE) as u8,
        depth_bits: attrib!(egl, display, config_id, ffi::egl::DEPTH_SIZE) as u8,
        stencil_bits: attrib!(egl, display, config_id, ffi::egl::STENCIL_SIZE) as u8,
        stereoscopy: false,
        double_buffer: true,
        multisampling: match attrib!(egl, display, config_id, ffi::egl::SAMPLES) {
            0 | 1 => None,
            a => Some(a as u16),
        },
        srgb: false,        // TODO: use EGL_KHR_gl_colorspace to know that
    };

    Ok((config_id, desc))
}

unsafe fn create_context(egl: &ffi::egl::Egl, display: ffi::egl::types::EGLDisplay,
                         egl_version: &(ffi::egl::types::EGLint, ffi::egl::types::EGLint),
                         extensions: &[String], api: Api, version: (u8, u8),
                         config_id: ffi::egl::types::EGLConfig, gl_debug: bool,
                         gl_robustness: Robustness)
                         -> Result<ffi::egl::types::EGLContext, CreationError>
{
    let mut context_attributes = Vec::with_capacity(10);
    let mut flags = 0;

    if egl_version >= &(1, 5) || extensions.iter().find(|s| s == &"EGL_KHR_create_context")
                                                  .is_some()
    {
        context_attributes.push(ffi::egl::CONTEXT_MAJOR_VERSION as i32);
        context_attributes.push(version.0 as i32);
        context_attributes.push(ffi::egl::CONTEXT_MINOR_VERSION as i32);
        context_attributes.push(version.1 as i32);

        // handling robustness
        let supports_robustness = egl_version >= &(1, 5) ||
                                  extensions.iter()
                                            .find(|s| s == &"EGL_EXT_create_context_robustness")
                                            .is_some();

        match gl_robustness {
            Robustness::NotRobust => (),

            Robustness::NoError => {
                if extensions.iter().find(|s| s == &"EGL_KHR_create_context_no_error").is_some() {
                    context_attributes.push(ffi::egl::CONTEXT_OPENGL_NO_ERROR_KHR as c_int);
                    context_attributes.push(1);
                }
            },

            Robustness::RobustNoResetNotification => {
                if supports_robustness {
                    context_attributes.push(ffi::egl::CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY
                                            as c_int);
                    context_attributes.push(ffi::egl::NO_RESET_NOTIFICATION as c_int);
                    flags = flags | ffi::egl::CONTEXT_OPENGL_ROBUST_ACCESS as c_int;
                } else {
                    return Err(CreationError::RobustnessNotSupported);
                }
            },

            Robustness::TryRobustNoResetNotification => {
                if supports_robustness {
                    context_attributes.push(ffi::egl::CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY
                                            as c_int);
                    context_attributes.push(ffi::egl::NO_RESET_NOTIFICATION as c_int);
                    flags = flags | ffi::egl::CONTEXT_OPENGL_ROBUST_ACCESS as c_int;
                }
            },

            Robustness::RobustLoseContextOnReset => {
                if supports_robustness {
                    context_attributes.push(ffi::egl::CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY
                                            as c_int);
                    context_attributes.push(ffi::egl::LOSE_CONTEXT_ON_RESET as c_int);
                    flags = flags | ffi::egl::CONTEXT_OPENGL_ROBUST_ACCESS as c_int;
                } else {
                    return Err(CreationError::RobustnessNotSupported);
                }
            },

            Robustness::TryRobustLoseContextOnReset => {
                if supports_robustness {
                    context_attributes.push(ffi::egl::CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY
                                            as c_int);
                    context_attributes.push(ffi::egl::LOSE_CONTEXT_ON_RESET as c_int);
                    flags = flags | ffi::egl::CONTEXT_OPENGL_ROBUST_ACCESS as c_int;
                }
            },
        }

        if gl_debug {
            if egl_version >= &(1, 5) {
                context_attributes.push(ffi::egl::CONTEXT_OPENGL_DEBUG as i32);
                context_attributes.push(ffi::egl::TRUE as i32);
            }

            // TODO: using this flag sometimes generates an error
            //       there was a change in the specs that added this flag, so it may not be
            //       supported everywhere ; however it is not possible to know whether it is
            //       supported or not
            //flags = flags | ffi::egl::CONTEXT_OPENGL_DEBUG_BIT_KHR as i32;
        }

        // In at least some configurations, the Android emulator’s GL implementation
        // advertises support for the EGL_KHR_create_context extension
        // but returns BAD_ATTRIBUTE when CONTEXT_FLAGS_KHR is used.
        if flags != 0 {
            context_attributes.push(ffi::egl::CONTEXT_FLAGS_KHR as i32);
            context_attributes.push(flags);
        }

    } else if egl_version >= &(1, 3) && api == Api::OpenGlEs {
        // robustness is not supported
        match gl_robustness {
            Robustness::RobustNoResetNotification | Robustness::RobustLoseContextOnReset => {
                return Err(CreationError::RobustnessNotSupported);
            },
            _ => ()
        }

        context_attributes.push(ffi::egl::CONTEXT_CLIENT_VERSION as i32);
        context_attributes.push(version.0 as i32);
    }

    context_attributes.push(ffi::egl::NONE as i32);

    let context = egl.CreateContext(display, config_id, ptr::null(),
                                    context_attributes.as_ptr());

    if context.is_null() {
        match egl.GetError() as u32 {
            ffi::egl::BAD_MATCH |
            ffi::egl::BAD_ATTRIBUTE => return Err(CreationError::OpenGlVersionNotSupported),
            e => panic!("eglCreateContext failed: 0x{:x}", e),
        }
    }

    Ok(context)
}
