#![cfg(target_os = "windows")]

use std::ffi::CString;
use std::ops::{Deref, DerefMut};

use winapi::shared::windef::HGLRC;
use winapi::um::libloaderapi::*;
use winit;

use CreationError;
use GlAttributes;
use PixelFormatRequirements;

use api::egl;
use api::egl::ffi::egl::Egl;

mod context;

/// Context handles available on Windows.
#[derive(Clone, Debug)]
pub enum RawHandle {
    Egl(egl::ffi::EGLContext),
    Wgl(HGLRC),
}

/// Stupid wrapper because `*const libc::c_void` doesn't implement `Sync`.
struct EglWrapper(Egl);
unsafe impl Sync for EglWrapper {}

lazy_static! {
    // An EGL implementation available on the system.
    static ref EGL: Option<EglWrapper> = {
        // the ATI drivers provide an EGL implementation in their DLLs
        let ati_dll_name = if cfg!(target_pointer_width = "64") {
            b"atio6axx.dll\0"
        } else {
            b"atioglxx.dll\0"
        };

        for dll_name in &[b"libEGL.dll\0" as &[u8], ati_dll_name] {
            let dll = unsafe { LoadLibraryA(dll_name.as_ptr() as *const _) };
            if dll.is_null() {
                continue;
            }

            let egl = Egl::load_with(|name| {
                let name = CString::new(name).unwrap();
                unsafe { GetProcAddress(dll, name.as_ptr()) as *const _ }
            });

            return Some(EglWrapper(egl))
        }

        None
    };
}

/// The Win32 implementation of the main `Context` object.
pub struct Context(context::Context);

impl Context {
    /// See the docs in the crate root file.
    #[inline]
    pub unsafe fn new(
        window_builder: winit::WindowBuilder,
        events_loop: &winit::EventsLoop,
        pf_reqs: &PixelFormatRequirements,
        opengl: &GlAttributes<&Self>,
    ) -> Result<(winit::Window, Self), CreationError> {
        context::Context::new(
            window_builder,
            events_loop,
            pf_reqs,
            &opengl.clone().map_sharing(|w| &w.0),
            EGL.as_ref().map(|w| &w.0),
        ).map(|(w, c)| (w, Context(c)))
    }

    /// See the docs in the crate root file.
    #[inline]
    pub unsafe fn new_context(
        events_loop: &winit::EventsLoop,
        pf_reqs: &PixelFormatRequirements,
        gl_attr: &GlAttributes<&Self>,
        shareable_with_windowed_contexts: bool,
    ) -> Result<Self, CreationError> {
        context::Context::new_context(
            events_loop,
            pf_reqs,
            &gl_attr.clone().map_sharing(|w| &w.0),
            shareable_with_windowed_contexts,
            EGL.as_ref().map(|w| &w.0),
        ).map(|c| Context(c))
    }
}

impl Deref for Context {
    type Target = context::Context;

    #[inline]
    fn deref(&self) -> &context::Context {
        &self.0
    }
}

impl DerefMut for Context {
    #[inline]
    fn deref_mut(&mut self) -> &mut context::Context {
        &mut self.0
    }
}
