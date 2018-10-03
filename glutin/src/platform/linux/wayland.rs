use std::sync::Arc;
use std::ffi::CString;
use winit;
use winit::os::unix::WindowExt;
use {ContextError, CreationError, GlAttributes, PixelFormat, PixelFormatRequirements};
use api::dlopen;
use api::egl::{self, ffi, Context as EglContext};
use wayland_client::egl as wegl;

pub struct Context {
    egl_surface: Arc<wegl::WlEglSurface>,
    context: EglContext,
}

impl Context {
    pub fn new(
        window_builder: winit::WindowBuilder,
        events_loop: &winit::EventsLoop,
        pf_reqs: &PixelFormatRequirements,
        gl_attr: &GlAttributes<&Context>,
    ) -> Result<(winit::Window, Self), CreationError>
    {
        let window = window_builder.build(events_loop)?;
        let logical_size = window.get_inner_size().unwrap();
        let (w, h) = (logical_size.width, logical_size.height);
        let surface = window.get_wayland_surface().unwrap();
        let egl_surface = unsafe { wegl::WlEglSurface::new_from_raw(surface as *mut _, w as i32, h as i32) };
        let context = {
            let libegl = unsafe { dlopen::dlopen(b"libEGL.so\0".as_ptr() as *const _, dlopen::RTLD_NOW) };
            if libegl.is_null() {
                return Err(CreationError::NotSupported("could not find libEGL"));
            }
            let egl = ::api::egl::ffi::egl::Egl::load_with(|sym| {
                let sym = CString::new(sym).unwrap();
                unsafe { dlopen::dlsym(libegl, sym.as_ptr()) }
            });
            let gl_attr = gl_attr.clone().map_sharing(|_| unimplemented!()); // TODO
            let native_display = egl::NativeDisplay::Wayland(Some(
                window.get_wayland_display().unwrap() as *const _
            ));
            EglContext::new(egl, pf_reqs, &gl_attr, native_display)
                .and_then(|p| p.finish(egl_surface.ptr() as *const _))?
        };
        let context = Context {
            egl_surface: Arc::new(egl_surface),
            context: context,
        };
        Ok((window, context))
    }

    #[inline]
    pub fn resize(&self, width: u32, height: u32) {
        self.egl_surface.resize(width as i32, height as i32, 0, 0);
    }

    #[inline]
    pub unsafe fn make_current(&self) -> Result<(), ContextError> {
        self.context.make_current()
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        self.context.is_current()
    }

    #[inline]
    pub fn get_proc_address(&self, addr: &str) -> *const () {
        self.context.get_proc_address(addr)
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<(), ContextError> {
        self.context.swap_buffers()
    }

    #[inline]
    pub fn get_api(&self) -> ::Api {
        self.context.get_api()
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        self.context.get_pixel_format().clone()
    }

    #[inline]
    pub unsafe fn raw_handle(&self) -> ffi::EGLContext {
        self.context.raw_handle()
    }
}
