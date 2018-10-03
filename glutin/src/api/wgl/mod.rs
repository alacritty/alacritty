#![cfg(any(target_os = "windows"))]

use ContextError;
use CreationError;
use GlAttributes;
use GlRequest;
use GlProfile;
use PixelFormat;
use PixelFormatRequirements;
use ReleaseBehavior;
use Robustness;
use Api;

use self::make_current_guard::CurrentContextGuard;

use std::ffi::{CStr, CString, OsStr};
use std::os::raw::{c_void, c_int};
use std::os::windows::ffi::OsStrExt;
use std::{io, mem, ptr};

use winapi::shared::windef::{HDC, HGLRC, HWND};
use winapi::shared::minwindef::HMODULE;
use winapi::um::wingdi::*;
use winapi::shared::minwindef::*;
use winapi::um::winuser::*;
use winapi::um::libloaderapi::*;
use winapi::shared::ntdef::LPCWSTR;

mod make_current_guard;
mod gl;

/// A WGL context.
///
/// Note: should be destroyed before its window.
pub struct Context {
    context: ContextWrapper,

    hdc: HDC,

    /// Bound to `opengl32.dll`.
    ///
    /// `wglGetProcAddress` returns null for GL 1.1 functions because they are
    ///  already defined by the system. This module contains them.
    gl_library: HMODULE,

    /// The pixel format that has been used to create this context.
    pixel_format: PixelFormat,
}

/// A simple wrapper that destroys the window when it is destroyed.
struct WindowWrapper(HWND, HDC);

impl Drop for WindowWrapper {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}

/// Wraps around a context so that it is destroyed when necessary.
struct ContextWrapper(HGLRC);

impl Drop for ContextWrapper {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            gl::wgl::DeleteContext(self.0 as *const _);
        }
    }
}

impl Context {
    /// Attempt to build a new WGL context on a window.
    ///
    /// The window must **not** have had `SetPixelFormat` called on it.
    ///
    /// # Unsafety
    ///
    /// The `window` must continue to exist as long as the resulting `Context` exists.
    pub unsafe fn new(pf_reqs: &PixelFormatRequirements, opengl: &GlAttributes<HGLRC>,
                      window: HWND) -> Result<Context, CreationError>
    {
        let hdc = GetDC(window);
        if hdc.is_null() {
            let err = Err(CreationError::OsError(format!("GetDC function failed: {}",
                                                format!("{}", io::Error::last_os_error()))));
            return err;
        }

        // loading the functions that are not guaranteed to be supported
        let extra_functions = load_extra_functions(window)?;

        // getting the list of the supported extensions
        let extensions = if extra_functions.GetExtensionsStringARB.is_loaded() {
            let data = extra_functions.GetExtensionsStringARB(hdc as *const _);
            let data = CStr::from_ptr(data).to_bytes().to_vec();
            String::from_utf8(data).unwrap()

        } else if extra_functions.GetExtensionsStringEXT.is_loaded() {
            let data = extra_functions.GetExtensionsStringEXT();
            let data = CStr::from_ptr(data).to_bytes().to_vec();
            String::from_utf8(data).unwrap()

        } else {
            format!("")
        };

        // calling SetPixelFormat
        let pixel_format = {
            let (id, f) = if extensions.split(' ').find(|&i| i == "WGL_ARB_pixel_format")
                                                  .is_some()
            {
                choose_arb_pixel_format(&extra_functions, &extensions, hdc, pf_reqs)
                                            .map_err(|_| CreationError::NoAvailablePixelFormat)?
            } else {
                choose_native_pixel_format(hdc, pf_reqs)
                                            .map_err(|_| CreationError::NoAvailablePixelFormat)?
            };

            set_pixel_format(hdc, id)?;
            f
        };

        // creating the OpenGL context
        let context = try!(create_context(Some((&extra_functions, pf_reqs, opengl, &extensions)),
                                          window, hdc));

        // loading the opengl32 module
        let gl_library = load_opengl32_dll()?;

        // handling vsync
        if extensions.split(' ').find(|&i| i == "WGL_EXT_swap_control").is_some() {
            let _guard = CurrentContextGuard::make_current(hdc, context.0)?;

            if extra_functions.SwapIntervalEXT(if opengl.vsync { 1 } else { 0 }) == 0 {
                return Err(CreationError::OsError(format!("wglSwapIntervalEXT failed")));
            }
        }

        Ok(Context {
            context: context,
            hdc: hdc,
            gl_library: gl_library,
            pixel_format: pixel_format,
        })
    }

    /// Returns the raw HGLRC.
    #[inline]
    pub fn get_hglrc(&self) -> HGLRC {
        self.context.0
    }

    #[inline]
    pub unsafe fn make_current(&self) -> Result<(), ContextError> {
        if gl::wgl::MakeCurrent(self.hdc as *const _, self.context.0 as *const _) != 0 {
            Ok(())
        } else {
            Err(ContextError::IoError(io::Error::last_os_error()))
        }
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        unsafe { gl::wgl::GetCurrentContext() == self.context.0 as *const c_void }
    }

    pub fn get_proc_address(&self, addr: &str) -> *const () {
        let addr = CString::new(addr.as_bytes()).unwrap();
        let addr = addr.as_ptr();

        unsafe {
            let p = gl::wgl::GetProcAddress(addr) as *const ();
            if !p.is_null() { return p; }
            GetProcAddress(self.gl_library, addr) as *const _
        }
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<(), ContextError> {
        // TODO: decide how to handle the error
        /*if unsafe { SwapBuffers(self.hdc) } != 0 {
            Ok(())
        } else {
            Err(ContextError::IoError(io::Error::last_os_error()))
        }*/
        unsafe { SwapBuffers(self.hdc) };
        Ok(())
    }

    #[inline]
    pub fn get_api(&self) -> Api {
        // FIXME: can be opengl es
        Api::OpenGl
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        self.pixel_format.clone()
    }
}

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

/// Creates an OpenGL context.
///
/// If `extra` is `Some`, this function will attempt to use the latest WGL functions to create the
/// context.
///
/// Otherwise, only the basic API will be used and the chances of `CreationError::NotSupported`
/// being returned increase.
unsafe fn create_context(extra: Option<(&gl::wgl_extra::Wgl, &PixelFormatRequirements,
                                        &GlAttributes<HGLRC>, &str)>,
                         _: HWND, hdc: HDC)
                         -> Result<ContextWrapper, CreationError>
{
    let share;

    if let Some((extra_functions, _pf_reqs, opengl, extensions)) = extra {
        share = opengl.sharing.unwrap_or(ptr::null_mut());

        if extensions.split(' ').find(|&i| i == "WGL_ARB_create_context").is_some() {
            let mut attributes = Vec::new();

            match opengl.version {
                GlRequest::Latest => {},
                GlRequest::Specific(Api::OpenGl, (major, minor)) => {
                    attributes.push(gl::wgl_extra::CONTEXT_MAJOR_VERSION_ARB as c_int);
                    attributes.push(major as c_int);
                    attributes.push(gl::wgl_extra::CONTEXT_MINOR_VERSION_ARB as c_int);
                    attributes.push(minor as c_int);
                },
                GlRequest::Specific(Api::OpenGlEs, (major, minor)) => {
                    if extensions.split(' ').find(|&i| i == "WGL_EXT_create_context_es2_profile")
                                            .is_some()
                    {
                        attributes.push(gl::wgl_extra::CONTEXT_PROFILE_MASK_ARB as c_int);
                        attributes.push(gl::wgl_extra::CONTEXT_ES2_PROFILE_BIT_EXT as c_int);
                    } else {
                        return Err(CreationError::OpenGlVersionNotSupported);
                    }

                    attributes.push(gl::wgl_extra::CONTEXT_MAJOR_VERSION_ARB as c_int);
                    attributes.push(major as c_int);
                    attributes.push(gl::wgl_extra::CONTEXT_MINOR_VERSION_ARB as c_int);
                    attributes.push(minor as c_int);
                },
                GlRequest::Specific(_, _) => return Err(CreationError::OpenGlVersionNotSupported),
                GlRequest::GlThenGles { opengl_version: (major, minor), .. } => {
                    attributes.push(gl::wgl_extra::CONTEXT_MAJOR_VERSION_ARB as c_int);
                    attributes.push(major as c_int);
                    attributes.push(gl::wgl_extra::CONTEXT_MINOR_VERSION_ARB as c_int);
                    attributes.push(minor as c_int);
                },
            }

            if let Some(profile) = opengl.profile {
                if extensions.split(' ').find(|&i| i == "WGL_ARB_create_context_profile").is_some()
                {
                    let flag = match profile {
                        GlProfile::Compatibility =>
                            gl::wgl_extra::CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB,
                        GlProfile::Core =>
                            gl::wgl_extra::CONTEXT_CORE_PROFILE_BIT_ARB,
                    };
                    attributes.push(gl::wgl_extra::CONTEXT_PROFILE_MASK_ARB as c_int);
                    attributes.push(flag as c_int);
                } else {
                    return Err(CreationError::NotSupported(
                            "required extension \"WGL_ARB_create_context_profile\" not found"));
                }
            }

            let flags = {
                let mut flags = 0;

                // robustness
                if extensions.split(' ').find(|&i| i == "WGL_ARB_create_context_robustness").is_some() {
                    match opengl.robustness {
                        Robustness::RobustNoResetNotification | Robustness::TryRobustNoResetNotification => {
                            attributes.push(gl::wgl_extra::CONTEXT_RESET_NOTIFICATION_STRATEGY_ARB as c_int);
                            attributes.push(gl::wgl_extra::NO_RESET_NOTIFICATION_ARB as c_int);
                            flags = flags | gl::wgl_extra::CONTEXT_ROBUST_ACCESS_BIT_ARB as c_int;
                        },
                        Robustness::RobustLoseContextOnReset | Robustness::TryRobustLoseContextOnReset => {
                            attributes.push(gl::wgl_extra::CONTEXT_RESET_NOTIFICATION_STRATEGY_ARB as c_int);
                            attributes.push(gl::wgl_extra::LOSE_CONTEXT_ON_RESET_ARB as c_int);
                            flags = flags | gl::wgl_extra::CONTEXT_ROBUST_ACCESS_BIT_ARB as c_int;
                        },
                        Robustness::NotRobust => (),
                        Robustness::NoError => (),
                    }
                } else {
                    match opengl.robustness {
                        Robustness::RobustNoResetNotification | Robustness::RobustLoseContextOnReset => {
                            return Err(CreationError::RobustnessNotSupported);
                        },
                        _ => ()
                    }
                }

                if opengl.debug {
                    flags = flags | gl::wgl_extra::CONTEXT_DEBUG_BIT_ARB as c_int;
                }

                flags
            };

            attributes.push(gl::wgl_extra::CONTEXT_FLAGS_ARB as c_int);
            attributes.push(flags);

            attributes.push(0);

            let ctxt = extra_functions.CreateContextAttribsARB(hdc as *const c_void,
                                                               share as *const c_void,
                                                               attributes.as_ptr());

            if ctxt.is_null() {
                return Err(CreationError::OsError(format!("wglCreateContextAttribsARB failed: {}",
                                                      format!("{}", io::Error::last_os_error()))));
            } else {
                return Ok(ContextWrapper(ctxt as HGLRC));
            }
        }

    } else {
        share = ptr::null_mut();
    }

    let ctxt = gl::wgl::CreateContext(hdc as *const c_void);
    if ctxt.is_null() {
        return Err(CreationError::OsError(format!("wglCreateContext failed: {}",
                                                  format!("{}", io::Error::last_os_error()))));
    }

    if !share.is_null() {
        if gl::wgl::ShareLists(share as *const c_void, ctxt) == 0 {
            return Err(CreationError::OsError(format!("wglShareLists failed: {}",
                                                      format!("{}", io::Error::last_os_error()))));
        }
    };

    Ok(ContextWrapper(ctxt as HGLRC))
}

/// Chooses a pixel formats without using WGL.
///
/// Gives less precise results than `enumerate_arb_pixel_formats`.
unsafe fn choose_native_pixel_format(hdc: HDC, reqs: &PixelFormatRequirements)
                                     -> Result<(c_int, PixelFormat), ()>
{
    // TODO: hardware acceleration is not handled

    // handling non-supported stuff
    if reqs.float_color_buffer {
        return Err(());
    }

    match reqs.multisampling {
        Some(0) => (),
        None => (),
        Some(_) => return Err(())
    };

    if reqs.stereoscopy {
        return Err(());
    }

    if reqs.srgb {
        return Err(());
    }

    if reqs.release_behavior != ReleaseBehavior::Flush {
        return Err(());
    }

    // building the descriptor to pass to ChoosePixelFormat
    let descriptor = PIXELFORMATDESCRIPTOR {
        nSize: mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
        nVersion: 1,
        dwFlags: {
            let f1 = match reqs.double_buffer {
                None => PFD_DOUBLEBUFFER, // Should be PFD_DOUBLEBUFFER_DONTCARE after you can choose
                Some(true) => PFD_DOUBLEBUFFER,
                Some(false) => 0,
            };

            let f2 = if reqs.stereoscopy {
                PFD_STEREO
            } else {
                0
            };

            PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | f1 | f2
        },
        iPixelType: PFD_TYPE_RGBA,
        cColorBits: reqs.color_bits.unwrap_or(0),
        cRedBits: 0,
        cRedShift: 0,
        cGreenBits: 0,
        cGreenShift: 0,
        cBlueBits: 0,
        cBlueShift: 0,
        cAlphaBits: reqs.alpha_bits.unwrap_or(0),
        cAlphaShift: 0,
        cAccumBits: 0,
        cAccumRedBits: 0,
        cAccumGreenBits: 0,
        cAccumBlueBits: 0,
        cAccumAlphaBits: 0,
        cDepthBits: reqs.depth_bits.unwrap_or(0),
        cStencilBits: reqs.stencil_bits.unwrap_or(0),
        cAuxBuffers: 0,
        iLayerType: PFD_MAIN_PLANE,
        bReserved: 0,
        dwLayerMask: 0,
        dwVisibleMask: 0,
        dwDamageMask: 0,
    };

    // now querying
    let pf_id = ChoosePixelFormat(hdc, &descriptor);
    if pf_id == 0 {
        return Err(());
    }

    // querying back the capabilities of what windows told us
    let mut output: PIXELFORMATDESCRIPTOR = mem::zeroed();
    if DescribePixelFormat(hdc, pf_id, mem::size_of::<PIXELFORMATDESCRIPTOR>() as u32,
                                  &mut output) == 0
    {
        return Err(());
    }

    // windows may return us a non-conforming pixel format if none are supported, so we have to
    // check this
    if (output.dwFlags & PFD_DRAW_TO_WINDOW) == 0 {
        return Err(());
    }
    if (output.dwFlags & PFD_SUPPORT_OPENGL) == 0 {
        return Err(());
    }
    if output.iPixelType != PFD_TYPE_RGBA {
        return Err(());
    }

    let pf_desc = PixelFormat {
        hardware_accelerated: (output.dwFlags & PFD_GENERIC_FORMAT) == 0,
        color_bits: output.cRedBits + output.cGreenBits + output.cBlueBits,
        alpha_bits: output.cAlphaBits,
        depth_bits: output.cDepthBits,
        stencil_bits: output.cStencilBits,
        stereoscopy: (output.dwFlags & PFD_STEREO) != 0,
        double_buffer: (output.dwFlags & PFD_DOUBLEBUFFER) != 0,
        multisampling: None,
        srgb: false,
    };

    if pf_desc.alpha_bits < reqs.alpha_bits.unwrap_or(0) {
        return Err(());
    }
    if pf_desc.depth_bits < reqs.depth_bits.unwrap_or(0) {
        return Err(());
    }
    if pf_desc.stencil_bits < reqs.stencil_bits.unwrap_or(0) {
        return Err(());
    }
    if pf_desc.color_bits < reqs.color_bits.unwrap_or(0) {
        return Err(());
    }
    if let Some(req) = reqs.hardware_accelerated {
        if pf_desc.hardware_accelerated != req {
            return Err(());
        }
    }
    if let Some(req) = reqs.double_buffer {
        if pf_desc.double_buffer != req {
            return Err(());
        }
    }

    Ok((pf_id, pf_desc))
}

/// Enumerates the list of pixel formats by using extra WGL functions.
///
/// Gives more precise results than `enumerate_native_pixel_formats`.
unsafe fn choose_arb_pixel_format(extra: &gl::wgl_extra::Wgl, extensions: &str,
                                  hdc: HDC, reqs: &PixelFormatRequirements)
                                  -> Result<(c_int, PixelFormat), ()>
{
    let descriptor = {
        let mut out: Vec<c_int> = Vec::with_capacity(37);

        out.push(gl::wgl_extra::DRAW_TO_WINDOW_ARB as c_int);
        out.push(1);

        out.push(gl::wgl_extra::SUPPORT_OPENGL_ARB as c_int);
        out.push(1);

        out.push(gl::wgl_extra::PIXEL_TYPE_ARB as c_int);
        if reqs.float_color_buffer {
            if extensions.split(' ').find(|&i| i == "WGL_ARB_pixel_format_float").is_some() {
                out.push(gl::wgl_extra::TYPE_RGBA_FLOAT_ARB as c_int);
            } else {
                return Err(());
            }
        } else {
            out.push(gl::wgl_extra::TYPE_RGBA_ARB as c_int);
        }

        if let Some(hardware_accelerated) = reqs.hardware_accelerated {
            out.push(gl::wgl_extra::ACCELERATION_ARB as c_int);
            out.push(if hardware_accelerated {
                gl::wgl_extra::FULL_ACCELERATION_ARB as c_int
            } else {
                gl::wgl_extra::NO_ACCELERATION_ARB as c_int
            });
        }

        if let Some(color) = reqs.color_bits {
            out.push(gl::wgl_extra::COLOR_BITS_ARB as c_int);
            out.push(color as c_int);
        }

        if let Some(alpha) = reqs.alpha_bits {
            out.push(gl::wgl_extra::ALPHA_BITS_ARB as c_int);
            out.push(alpha as c_int);
        }

        if let Some(depth) = reqs.depth_bits {
            out.push(gl::wgl_extra::DEPTH_BITS_ARB as c_int);
            out.push(depth as c_int);
        }

        if let Some(stencil) = reqs.stencil_bits {
            out.push(gl::wgl_extra::STENCIL_BITS_ARB as c_int);
            out.push(stencil as c_int);
        }

        // Prefer double buffering if unspecified (probably shouldn't once you can choose)
        let double_buffer = reqs.double_buffer.unwrap_or(true);
        out.push(gl::wgl_extra::DOUBLE_BUFFER_ARB as c_int);
        out.push(if double_buffer { 1 } else { 0 });

        if let Some(multisampling) = reqs.multisampling {
            if extensions.split(' ').find(|&i| i == "WGL_ARB_multisample").is_some() {
                out.push(gl::wgl_extra::SAMPLE_BUFFERS_ARB as c_int);
                out.push(if multisampling == 0 { 0 } else { 1 });
                out.push(gl::wgl_extra::SAMPLES_ARB as c_int);
                out.push(multisampling as c_int);
            } else {
                return Err(());
            }
        }

        out.push(gl::wgl_extra::STEREO_ARB as c_int);
        out.push(if reqs.stereoscopy { 1 } else { 0 });

        if reqs.srgb {
            if extensions.split(' ').find(|&i| i == "WGL_ARB_framebuffer_sRGB").is_some() {
                out.push(gl::wgl_extra::FRAMEBUFFER_SRGB_CAPABLE_ARB as c_int);
                out.push(1);
            } else if extensions.split(' ').find(|&i| i == "WGL_EXT_framebuffer_sRGB").is_some() {
                out.push(gl::wgl_extra::FRAMEBUFFER_SRGB_CAPABLE_EXT as c_int);
                out.push(1);
            } else {
                return Err(());
            }
        }

        match reqs.release_behavior {
            ReleaseBehavior::Flush => (),
            ReleaseBehavior::None => {
                if extensions.split(' ').find(|&i| i == "WGL_ARB_context_flush_control").is_some() {
                    out.push(gl::wgl_extra::CONTEXT_RELEASE_BEHAVIOR_ARB as c_int);
                    out.push(gl::wgl_extra::CONTEXT_RELEASE_BEHAVIOR_NONE_ARB as c_int);
                }
            },
        }

        out.push(0);
        out
    };

    let mut format_id = mem::uninitialized();
    let mut num_formats = mem::uninitialized();
    if extra.ChoosePixelFormatARB(hdc as *const _, descriptor.as_ptr(), ptr::null(), 1,
                                  &mut format_id, &mut num_formats) == 0
    {
        return Err(());
    }

    if num_formats == 0 {
        return Err(());
    }

    let get_info = |attrib: u32| {
        let mut value = mem::uninitialized();
        extra.GetPixelFormatAttribivARB(hdc as *const _, format_id as c_int,
                                        0, 1, [attrib as c_int].as_ptr(),
                                        &mut value);
        value as u32
    };

    let pf_desc = PixelFormat {
        hardware_accelerated: get_info(gl::wgl_extra::ACCELERATION_ARB) !=
                                                                gl::wgl_extra::NO_ACCELERATION_ARB,
        color_bits: get_info(gl::wgl_extra::RED_BITS_ARB) as u8 +
                    get_info(gl::wgl_extra::GREEN_BITS_ARB) as u8 +
                    get_info(gl::wgl_extra::BLUE_BITS_ARB) as u8,
        alpha_bits: get_info(gl::wgl_extra::ALPHA_BITS_ARB) as u8,
        depth_bits: get_info(gl::wgl_extra::DEPTH_BITS_ARB) as u8,
        stencil_bits: get_info(gl::wgl_extra::STENCIL_BITS_ARB) as u8,
        stereoscopy: get_info(gl::wgl_extra::STEREO_ARB) != 0,
        double_buffer: get_info(gl::wgl_extra::DOUBLE_BUFFER_ARB) != 0,
        multisampling: {
            if extensions.split(' ').find(|&i| i == "WGL_ARB_multisample").is_some() {
                match get_info(gl::wgl_extra::SAMPLES_ARB) {
                    0 => None,
                    a => Some(a as u16),
                }
            } else {
                None
            }
        },
        srgb: if extensions.split(' ').find(|&i| i == "WGL_ARB_framebuffer_sRGB").is_some() {
            get_info(gl::wgl_extra::FRAMEBUFFER_SRGB_CAPABLE_ARB) != 0
        } else if extensions.split(' ').find(|&i| i == "WGL_EXT_framebuffer_sRGB").is_some() {
            get_info(gl::wgl_extra::FRAMEBUFFER_SRGB_CAPABLE_EXT) != 0
        } else {
            false
        },
    };

    Ok((format_id, pf_desc))
}

/// Calls `SetPixelFormat` on a window.
unsafe fn set_pixel_format(hdc: HDC, id: c_int) -> Result<(), CreationError> {
    let mut output: PIXELFORMATDESCRIPTOR = mem::zeroed();

    if DescribePixelFormat(hdc, id, mem::size_of::<PIXELFORMATDESCRIPTOR>()
                                  as UINT, &mut output) == 0
    {
        return Err(CreationError::OsError(format!("DescribePixelFormat function failed: {}",
                                                  format!("{}", io::Error::last_os_error()))));
    }

    if SetPixelFormat(hdc, id, &output) == 0 {
        return Err(CreationError::OsError(format!("SetPixelFormat function failed: {}",
                                                  format!("{}", io::Error::last_os_error()))));
    }

    Ok(())
}

/// Loads the `opengl32.dll` library.
unsafe fn load_opengl32_dll() -> Result<HMODULE, CreationError> {
    let name = OsStr::new("opengl32.dll").encode_wide().chain(Some(0).into_iter())
                                         .collect::<Vec<_>>();

    let lib = LoadLibraryW(name.as_ptr());

    if lib.is_null() {
        return Err(CreationError::OsError(format!("LoadLibrary function failed: {}",
                                                  format!("{}", io::Error::last_os_error()))));
    }

    Ok(lib)
}

/// Loads the WGL functions that are not guaranteed to be supported.
///
/// The `window` must be passed because the driver can vary depending on the window's
/// characteristics.
unsafe fn load_extra_functions(window: HWND) -> Result<gl::wgl_extra::Wgl, CreationError> {
    let (ex_style, style) = (WS_EX_APPWINDOW, WS_POPUP |
                             WS_CLIPSIBLINGS | WS_CLIPCHILDREN);

    // creating a dummy invisible window
    let dummy_window = {
        // getting the rect of the real window
        let rect = {
            let mut placement: WINDOWPLACEMENT = mem::zeroed();
            placement.length = mem::size_of::<WINDOWPLACEMENT>() as UINT;
            if GetWindowPlacement(window, &mut placement) == 0 {
                panic!();
            }
            placement.rcNormalPosition
        };

        // getting the class name of the real window
        let mut class_name = [0u16; 128];
        if GetClassNameW(window, class_name.as_mut_ptr(), 128) == 0 {
            return Err(CreationError::OsError(format!("GetClassNameW function failed: {}",
                                              format!("{}", io::Error::last_os_error()))));
        }

        // access to class information of the real window
        let instance = GetModuleHandleW(ptr::null());
        let mut class: WNDCLASSEXW = mem::zeroed();

        if GetClassInfoExW(instance, class_name.as_ptr(), &mut class) == 0 {
            return Err(CreationError::OsError(format!("GetClassInfoExW function failed: {}",
                                              format!("{}", io::Error::last_os_error()))));
        }

        // register a new class for the dummy window,
        // similar to the class of the real window but with a different callback
        let class_name = OsStr::new("WglDummy Class").encode_wide().chain(Some(0).into_iter())
                                                   .collect::<Vec<_>>();

        class.cbSize = mem::size_of::<WNDCLASSEXW>() as UINT;
        class.lpszClassName = class_name.as_ptr();
        class.lpfnWndProc = Some(DefWindowProcW);

        // this shouldn't fail if the registration of the real window class worked.
        // multiple registrations of the window class trigger an error which we want
        // to ignore silently (e.g for multi-window setups)
        RegisterClassExW(&class);

        // this dummy window should match the real one enough to get the same OpenGL driver
        let title = OsStr::new("dummy window").encode_wide().chain(Some(0).into_iter())
                                              .collect::<Vec<_>>();
        let win = CreateWindowExW(ex_style, class_name.as_ptr(),
                                          title.as_ptr() as LPCWSTR, style,
                                          CW_USEDEFAULT, CW_USEDEFAULT,
                                          rect.right - rect.left,
                                          rect.bottom - rect.top,
                                          ptr::null_mut(), ptr::null_mut(),
                                          GetModuleHandleW(ptr::null()),
                                          ptr::null_mut());

        if win.is_null() {
            return Err(CreationError::OsError(format!("CreateWindowEx function failed: {}",
                                              format!("{}", io::Error::last_os_error()))));
        }

        let hdc = GetDC(win);
        if hdc.is_null() {
            let err = Err(CreationError::OsError(format!("GetDC function failed: {}",
                                               format!("{}", io::Error::last_os_error()))));
            return err;
        }

        WindowWrapper(win, hdc)
    };

    // getting the pixel format that we will use and setting it
    {
        let id = choose_dummy_pixel_format(dummy_window.1)?;
        set_pixel_format(dummy_window.1, id)?;
    }

    // creating the dummy OpenGL context and making it current
    let dummy_context = create_context(None, dummy_window.0, dummy_window.1)?;
    let _current_context = CurrentContextGuard::make_current(dummy_window.1,
                                                                  dummy_context.0)?;

    // loading the extra WGL functions
    Ok(gl::wgl_extra::Wgl::load_with(|addr| {
        let addr = CString::new(addr.as_bytes()).unwrap();
        let addr = addr.as_ptr();
        gl::wgl::GetProcAddress(addr) as *const c_void
    }))
}

/// This function chooses a pixel format that is likely to be provided by
/// the main video driver of the system.
fn choose_dummy_pixel_format(hdc: HDC) -> Result<c_int, CreationError> {
    // building the descriptor to pass to ChoosePixelFormat
    let descriptor = PIXELFORMATDESCRIPTOR {
        nSize: mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
        nVersion: 1,
        dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
        iPixelType: PFD_TYPE_RGBA,
        cColorBits: 24,
        cRedBits: 0,
        cRedShift: 0,
        cGreenBits: 0,
        cGreenShift: 0,
        cBlueBits: 0,
        cBlueShift: 0,
        cAlphaBits: 8,
        cAlphaShift: 0,
        cAccumBits: 0,
        cAccumRedBits: 0,
        cAccumGreenBits: 0,
        cAccumBlueBits: 0,
        cAccumAlphaBits: 0,
        cDepthBits: 24,
        cStencilBits: 8,
        cAuxBuffers: 0,
        iLayerType: PFD_MAIN_PLANE,
        bReserved: 0,
        dwLayerMask: 0,
        dwVisibleMask: 0,
        dwDamageMask: 0,
    };

    // now querying
    let pf_id = unsafe { ChoosePixelFormat(hdc, &descriptor) };
    if pf_id == 0 {
        return Err(CreationError::OsError("No available pixel format".to_owned()));
    }

    Ok(pf_id)
}
