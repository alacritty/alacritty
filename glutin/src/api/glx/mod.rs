#![cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]

use {
    Api,
    ContextError,
    CreationError,
    GlAttributes,
    GlProfile,
    GlRequest,
    PixelFormat,
    PixelFormatRequirements,
    ReleaseBehavior,
    Robustness,
};

use std::{mem, ptr, slice};
use std::ffi::{CStr, CString};
use std::sync::Arc;

use libc::{self, c_int};

use winit::os::unix::x11::XConnection;

pub mod ffi {
    pub use x11_dl::xlib::*;
    pub use self::glx::types::GLXContext;

    /// GLX bindings
    pub mod glx {
        include!(concat!(env!("OUT_DIR"), "/glx_bindings.rs"));
    }

    /// Functions that are not necessarily always available
    pub mod glx_extra {
        include!(concat!(env!("OUT_DIR"), "/glx_extra_bindings.rs"));
    }
}

pub struct Context {
    glx: ffi::glx::Glx,
    xconn: Arc<XConnection>,
    window: ffi::Window,
    context: ffi::GLXContext,
    pixel_format: PixelFormat,
}

impl Context {
    pub fn new<'a>(
        glx: ffi::glx::Glx,
        xconn: Arc<XConnection>,
        pf_reqs: &PixelFormatRequirements,
        opengl: &'a GlAttributes<&'a Context>,
        screen_id: libc::c_int,
        transparent: bool,
    ) -> Result<ContextPrototype<'a>, CreationError>
    {
        // This is completely ridiculous, but VirtualBox's OpenGL driver needs some call handled by
        // *it* (i.e. not Mesa) to occur before anything else can happen. That is because
        // VirtualBox's OpenGL driver is going to apply binary patches to Mesa in the DLL
        // constructor and until it's loaded it won't have a chance to do that.
        //
        // The easiest way to do this is to just call `glXQueryVersion()` before doing anything
        // else. See: https://www.virtualbox.org/ticket/8293
        let (mut major, mut minor) = (0, 0);
        unsafe {
            glx.QueryVersion(xconn.display as *mut _, &mut major, &mut minor);
        }

        // loading the list of extensions
        let extensions = unsafe {
            let extensions = glx.QueryExtensionsString(xconn.display as *mut _, screen_id);
            if extensions.is_null() {
                return Err(CreationError::OsError(format!("`glXQueryExtensionsString` found no glX extensions")));
            }
            let extensions = CStr::from_ptr(extensions).to_bytes().to_vec();
            String::from_utf8(extensions).unwrap()
        };

        // finding the pixel format we want
        let (fb_config, pixel_format) = unsafe {
            choose_fbconfig(
                &glx,
                &extensions,
                &xconn.xlib,
                xconn.display,
                screen_id,
                pf_reqs,
                transparent,
            ).map_err(|_| CreationError::NoAvailablePixelFormat)?
        };

        // getting the visual infos
        let visual_infos: ffi::glx::types::XVisualInfo = unsafe {
            let vi = glx.GetVisualFromFBConfig(xconn.display as *mut _, fb_config);
            if vi.is_null() {
                return Err(CreationError::OsError(
                    format!("`glXGetVisualFromFBConfig` failed: invalid `GLXFBConfig`")
                ));
            }
            let vi_copy = ptr::read(vi as *const _);
            (xconn.xlib.XFree)(vi as *mut _);
            vi_copy
        };

        Ok(ContextPrototype {
            glx,
            extensions,
            xconn,
            opengl,
            fb_config,
            visual_infos: unsafe { mem::transmute(visual_infos) },
            pixel_format,
        })
    }

    pub unsafe fn make_current(&self) -> Result<(), ContextError> {
        let res = self.glx.MakeCurrent(self.xconn.display as *mut _, self.window, self.context);
        if res == 0 {
            let err = self.xconn.check_errors();
            Err(ContextError::OsError(format!("`glXMakeCurrent` failed: {:?}", err)))
        } else {
            Ok(())
        }
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        unsafe { self.glx.GetCurrentContext() == self.context }
    }

    pub fn get_proc_address(&self, addr: &str) -> *const () {
        let addr = CString::new(addr.as_bytes()).unwrap();
        let addr = addr.as_ptr();
        unsafe {
            self.glx.GetProcAddress(addr as *const _) as *const _
        }
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<(), ContextError> {
        unsafe { self.glx.SwapBuffers(self.xconn.display as *mut _, self.window); }
        if let Err(err) = self.xconn.check_errors() {
            Err(ContextError::OsError(format!("`glXSwapBuffers` failed: {:?}", err)))
        } else {
            Ok(())
        }
    }

    #[inline]
    pub fn get_api(&self) -> ::Api {
        ::Api::OpenGl
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        self.pixel_format.clone()
    }

    #[inline]
    pub unsafe fn raw_handle(&self) -> ffi::GLXContext {
        self.context
    }
}

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            if self.is_current() {
                self.glx.MakeCurrent(self.xconn.display as *mut _, 0, ptr::null_mut());
            }

            self.glx.DestroyContext(self.xconn.display as *mut _, self.context);
        }
    }
}

pub struct ContextPrototype<'a> {
    glx: ffi::glx::Glx,
    extensions: String,
    xconn: Arc<XConnection>,
    opengl: &'a GlAttributes<&'a Context>,
    fb_config: ffi::glx::types::GLXFBConfig,
    visual_infos: ffi::XVisualInfo,
    pixel_format: PixelFormat,
}

impl<'a> ContextPrototype<'a> {
    #[inline]
    pub fn get_visual_infos(&self) -> &ffi::XVisualInfo {
        &self.visual_infos
    }

    pub fn finish(self, window: ffi::Window) -> Result<Context, CreationError> {
        let share = match self.opengl.sharing {
            Some(ctxt) => ctxt.context,
            None => ptr::null()
        };

        // loading the extra GLX functions
        let extra_functions = ffi::glx_extra::Glx::load_with(|proc_name| {
            let c_str = CString::new(proc_name).unwrap();
            unsafe { self.glx.GetProcAddress(c_str.as_ptr() as *const u8) as *const _ }
        });

        // creating GL context
        let context = match self.opengl.version {
            GlRequest::Latest => {
                let opengl_versions = [(4, 6), (4, 5), (4, 4), (4, 3), (4, 2), (4, 1), (4, 0),
                                       (3, 3), (3, 2), (3, 1)];
                let ctxt;
                'outer: loop
                {
                    // Try all OpenGL versions in descending order because some non-compliant
                    // drivers don't return the latest supported version but the one requested
                    for opengl_version in opengl_versions.iter()
                    {
                        match create_context(&self.glx, &extra_functions, &self.extensions, &self.xconn.xlib,
                                             *opengl_version, self.opengl.profile,
                                             self.opengl.debug, self.opengl.robustness, share,
                                             self.xconn.display, self.fb_config, &self.visual_infos)
                        {
                            Ok(x) => {
                                ctxt = x;
                                break 'outer;
                            },
                            Err(_) => continue
                        }
                    }
                    ctxt = create_context(&self.glx, &extra_functions, &self.extensions, &self.xconn.xlib, (1, 0),
                                               self.opengl.profile, self.opengl.debug,
                                               self.opengl.robustness, share,
                                               self.xconn.display, self.fb_config, &self.visual_infos)?;
                    break;
                }
                ctxt
            },
            GlRequest::Specific(Api::OpenGl, (major, minor)) => {
                create_context(&self.glx, &extra_functions, &self.extensions, &self.xconn.xlib, (major, minor),
                                    self.opengl.profile, self.opengl.debug,
                                    self.opengl.robustness, share, self.xconn.display, self.fb_config,
                                    &self.visual_infos)?
            },
            GlRequest::Specific(_, _) => panic!("Only OpenGL is supported"),
            GlRequest::GlThenGles { opengl_version: (major, minor), .. } => {
                create_context(&self.glx, &extra_functions, &self.extensions, &self.xconn.xlib, (major, minor),
                                    self.opengl.profile, self.opengl.debug,
                                    self.opengl.robustness, share, self.xconn.display, self.fb_config,
                                    &self.visual_infos)?
            },
        };

        // vsync
        if self.opengl.vsync {
            unsafe { self.glx.MakeCurrent(self.xconn.display as *mut _, window, context) };

            if check_ext(&self.extensions, "GLX_EXT_swap_control") && extra_functions.SwapIntervalEXT.is_loaded() {
                // this should be the most common extension
                unsafe {
                    extra_functions.SwapIntervalEXT(self.xconn.display as *mut _, window, 1);
                }

                // checking that it worked
                // TODO: handle this
                /*if self.builder.strict {
                    let mut swap = unsafe { mem::uninitialized() };
                    unsafe {
                        self.glx.QueryDrawable(self.xconn.display as *mut _, window,
                                               ffi::glx_extra::SWAP_INTERVAL_EXT as i32,
                                               &mut swap);
                    }

                    if swap != 1 {
                        return Err(CreationError::OsError(format!("Couldn't setup vsync: expected \
                                                    interval `1` but got `{}`", swap)));
                    }
                }*/

            // GLX_MESA_swap_control is not official
            /*} else if extra_functions.SwapIntervalMESA.is_loaded() {
                unsafe {
                    extra_functions.SwapIntervalMESA(1);
                }*/

            } else if check_ext(&self.extensions, "GLX_SGI_swap_control") && extra_functions.SwapIntervalSGI.is_loaded() {
                unsafe {
                    extra_functions.SwapIntervalSGI(1);
                }

            }/* else if self.builder.strict {
                // TODO: handle this
                return Err(CreationError::OsError(format!("Couldn't find any available vsync extension")));
            }*/

            unsafe { self.glx.MakeCurrent(self.xconn.display as *mut _, 0, ptr::null()) };
        }

        Ok(Context {
            glx: self.glx,
            xconn: self.xconn,
            window,
            context,
            pixel_format: self.pixel_format,
        })
    }
}

extern fn x_error_callback(_dpy: *mut ffi::Display, _err: *mut ffi::XErrorEvent) -> i32
{
    0
}


fn create_context(glx: &ffi::glx::Glx, extra_functions: &ffi::glx_extra::Glx, extensions: &str, xlib: &ffi::Xlib,
                  version: (u8, u8), profile: Option<GlProfile>, debug: bool,
                  robustness: Robustness, share: ffi::GLXContext, display: *mut ffi::Display,
                  fb_config: ffi::glx::types::GLXFBConfig,
                  visual_infos: &ffi::XVisualInfo)
                  -> Result<ffi::GLXContext, CreationError>
{
    unsafe {
        let old_callback = (xlib.XSetErrorHandler)(Some(x_error_callback));
        let context = if check_ext(extensions, "GLX_ARB_create_context") {
            let mut attributes = Vec::with_capacity(9);

            attributes.push(ffi::glx_extra::CONTEXT_MAJOR_VERSION_ARB as c_int);
            attributes.push(version.0 as c_int);
            attributes.push(ffi::glx_extra::CONTEXT_MINOR_VERSION_ARB as c_int);
            attributes.push(version.1 as c_int);

            if let Some(profile) = profile {
                let flag = match profile {
                    GlProfile::Compatibility =>
                        ffi::glx_extra::CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB,
                    GlProfile::Core =>
                        ffi::glx_extra::CONTEXT_CORE_PROFILE_BIT_ARB,
                };

                attributes.push(ffi::glx_extra::CONTEXT_PROFILE_MASK_ARB as c_int);
                attributes.push(flag as c_int);
            }

            let flags = {
                let mut flags = 0;

                // robustness
                if check_ext(extensions, "GLX_ARB_create_context_robustness") {
                    match robustness {
                        Robustness::RobustNoResetNotification | Robustness::TryRobustNoResetNotification => {
                            attributes.push(ffi::glx_extra::CONTEXT_RESET_NOTIFICATION_STRATEGY_ARB as c_int);
                            attributes.push(ffi::glx_extra::NO_RESET_NOTIFICATION_ARB as c_int);
                            flags = flags | ffi::glx_extra::CONTEXT_ROBUST_ACCESS_BIT_ARB as c_int;
                        },
                        Robustness::RobustLoseContextOnReset | Robustness::TryRobustLoseContextOnReset => {
                            attributes.push(ffi::glx_extra::CONTEXT_RESET_NOTIFICATION_STRATEGY_ARB as c_int);
                            attributes.push(ffi::glx_extra::LOSE_CONTEXT_ON_RESET_ARB as c_int);
                            flags = flags | ffi::glx_extra::CONTEXT_ROBUST_ACCESS_BIT_ARB as c_int;
                        },
                        Robustness::NotRobust => (),
                        Robustness::NoError => (),
                    }
                } else {
                    match robustness {
                        Robustness::RobustNoResetNotification | Robustness::RobustLoseContextOnReset => {
                            return Err(CreationError::RobustnessNotSupported);
                        },
                        _ => ()
                    }
                }

                if debug {
                    flags = flags | ffi::glx_extra::CONTEXT_DEBUG_BIT_ARB as c_int;
                }

                flags
            };

            attributes.push(ffi::glx_extra::CONTEXT_FLAGS_ARB as c_int);
            attributes.push(flags);

            attributes.push(0);

            extra_functions.CreateContextAttribsARB(display as *mut _, fb_config, share, 1,
                                                    attributes.as_ptr())

        } else {
            let visual_infos: *const ffi::XVisualInfo = visual_infos;
            glx.CreateContext(display as *mut _, visual_infos as *mut _, share, 1)
        };

        (xlib.XSetErrorHandler)(old_callback);

        if context.is_null() {
            // TODO: check for errors and return `OpenGlVersionNotSupported`
            return Err(CreationError::OsError(format!("GL context creation failed")));
        }

        Ok(context)
    }
}

/// Enumerates all available FBConfigs
unsafe fn choose_fbconfig(glx: &ffi::glx::Glx, extensions: &str, xlib: &ffi::Xlib,
                          display: *mut ffi::Display, screen_id: libc::c_int,
                          reqs: &PixelFormatRequirements, transparent: bool)
                          -> Result<(ffi::glx::types::GLXFBConfig, PixelFormat), ()>
{
    let descriptor = {
        let mut out: Vec<c_int> = Vec::with_capacity(37);

        out.push(ffi::glx::X_RENDERABLE as c_int);
        out.push(1);

        out.push(ffi::glx::X_VISUAL_TYPE as c_int);
        out.push(ffi::glx::TRUE_COLOR as c_int);

        out.push(ffi::glx::DRAWABLE_TYPE as c_int);
        out.push(ffi::glx::WINDOW_BIT as c_int);

        out.push(ffi::glx::RENDER_TYPE as c_int);
        if reqs.float_color_buffer {
            if check_ext(extensions, "GLX_ARB_fbconfig_float") {
                out.push(ffi::glx_extra::RGBA_FLOAT_BIT_ARB as c_int);
            } else {
                return Err(());
            }
        } else {
            out.push(ffi::glx::RGBA_BIT as c_int);
        }

        if let Some(color) = reqs.color_bits {
            out.push(ffi::glx::RED_SIZE as c_int);
            out.push((color / 3) as c_int);
            out.push(ffi::glx::GREEN_SIZE as c_int);
            out.push((color / 3 + if color % 3 != 0 { 1 } else { 0 }) as c_int);
            out.push(ffi::glx::BLUE_SIZE as c_int);
            out.push((color / 3 + if color % 3 == 2 { 1 } else { 0 }) as c_int);
        }

        if let Some(alpha) = reqs.alpha_bits {
            out.push(ffi::glx::ALPHA_SIZE as c_int);
            out.push(alpha as c_int);
        }

        if let Some(depth) = reqs.depth_bits {
            out.push(ffi::glx::DEPTH_SIZE as c_int);
            out.push(depth as c_int);
        }

        if let Some(stencil) = reqs.stencil_bits {
            out.push(ffi::glx::STENCIL_SIZE as c_int);
            out.push(stencil as c_int);
        }

        let double_buffer = reqs.double_buffer.unwrap_or(true);
        out.push(ffi::glx::DOUBLEBUFFER as c_int);
        out.push(if double_buffer { 1 } else { 0 });

        if let Some(multisampling) = reqs.multisampling {
            if check_ext(extensions, "GLX_ARB_multisample") {
                out.push(ffi::glx_extra::SAMPLE_BUFFERS_ARB as c_int);
                out.push(if multisampling == 0 { 0 } else { 1 });
                out.push(ffi::glx_extra::SAMPLES_ARB as c_int);
                out.push(multisampling as c_int);
            } else {
                return Err(());
            }
        }

        out.push(ffi::glx::STEREO as c_int);
        out.push(if reqs.stereoscopy { 1 } else { 0 });

        if reqs.srgb {
            if check_ext(extensions, "GLX_ARB_framebuffer_sRGB") {
                out.push(ffi::glx_extra::FRAMEBUFFER_SRGB_CAPABLE_ARB as c_int);
                out.push(1);
            } else if check_ext(extensions, "GLX_EXT_framebuffer_sRGB") {
                out.push(ffi::glx_extra::FRAMEBUFFER_SRGB_CAPABLE_EXT as c_int);
                out.push(1);
            } else {
                return Err(());
            }
        }

        match reqs.release_behavior {
            ReleaseBehavior::Flush => (),
            ReleaseBehavior::None => {
                if check_ext(extensions, "GLX_ARB_context_flush_control") {
                    out.push(ffi::glx_extra::CONTEXT_RELEASE_BEHAVIOR_ARB as c_int);
                    out.push(ffi::glx_extra::CONTEXT_RELEASE_BEHAVIOR_NONE_ARB as c_int);
                }
            },
        }

        out.push(ffi::glx::CONFIG_CAVEAT as c_int);
        out.push(ffi::glx::DONT_CARE as c_int);

        out.push(0);
        out
    };

    // calling glXChooseFBConfig
    let fb_config = {
        let mut num_configs = 1;
        let configs = glx.ChooseFBConfig(display as *mut _, screen_id, descriptor.as_ptr(),
                                        &mut num_configs);
        if configs.is_null() { return Err(()); }
        if num_configs == 0 { return Err(()); }

        let config = if transparent {
            let configs = slice::from_raw_parts(configs, num_configs as usize);
            configs.iter().find(|&config| {
                let vi = glx.GetVisualFromFBConfig(display as *mut _, *config);
                // Transparency was requested, so only choose configs with 32 bits for RGBA.
                let found = !vi.is_null() && (*vi).depth == 32;
                (xlib.XFree)(vi as *mut _);

                found
            })
        } else {
            Some(&*configs)
        };

        let res = if let Some(&conf) = config {
            Ok(conf)
        } else {
            Err(())
        };

        (xlib.XFree)(configs as *mut _);
        res?
    };

    let get_attrib = |attrib: c_int| -> i32 {
        let mut value = 0;
        glx.GetFBConfigAttrib(display as *mut _, fb_config, attrib, &mut value);
        // TODO: check return value
        value
    };

    let pf_desc = PixelFormat {
        hardware_accelerated: get_attrib(ffi::glx::CONFIG_CAVEAT as c_int) !=
                                                            ffi::glx::SLOW_CONFIG as c_int,
        color_bits: get_attrib(ffi::glx::RED_SIZE as c_int) as u8 +
                    get_attrib(ffi::glx::GREEN_SIZE as c_int) as u8 +
                    get_attrib(ffi::glx::BLUE_SIZE as c_int) as u8,
        alpha_bits: get_attrib(ffi::glx::ALPHA_SIZE as c_int) as u8,
        depth_bits: get_attrib(ffi::glx::DEPTH_SIZE as c_int) as u8,
        stencil_bits: get_attrib(ffi::glx::STENCIL_SIZE as c_int) as u8,
        stereoscopy: get_attrib(ffi::glx::STEREO as c_int) != 0,
        double_buffer: get_attrib(ffi::glx::DOUBLEBUFFER as c_int) != 0,
        multisampling: if get_attrib(ffi::glx::SAMPLE_BUFFERS as c_int) != 0 {
            Some(get_attrib(ffi::glx::SAMPLES as c_int) as u16)
        } else {
            None
        },
        srgb: get_attrib(ffi::glx_extra::FRAMEBUFFER_SRGB_CAPABLE_ARB as c_int) != 0 ||
              get_attrib(ffi::glx_extra::FRAMEBUFFER_SRGB_CAPABLE_EXT as c_int) != 0,
    };

    Ok((fb_config, pf_desc))
}

/// Checks if `ext` is available.
fn check_ext(extensions: &str, ext: &str) -> bool {
    extensions.split(' ').find(|&s| s == ext).is_some()
}
