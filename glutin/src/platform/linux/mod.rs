#![cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]

use {ContextError, CreationError, GlAttributes, PixelFormat, PixelFormatRequirements};
use api::egl;
use api::glx;
use self::x11::GlContext;

use winit;
use winit::os::unix::EventsLoopExt;

mod wayland;
mod x11;
use api::osmesa;


/// Context handles available on Unix-like platforms.
#[derive(Clone, Debug)]
pub enum RawHandle {
    Glx(glx::ffi::GLXContext),
    Egl(egl::ffi::EGLContext),
}

pub enum ContextType {
    X11,
    Wayland,
    OsMesa,
}

pub enum Context {
    WindowedX11(x11::Context),
    HeadlessX11(winit::Window, x11::Context),
    WindowedWayland(wayland::Context),
    HeadlessWayland(winit::Window, wayland::Context),
    OsMesa(osmesa::OsMesaContext),
}

impl Context {
    fn is_compatible(c: &Option<&Context>, ct: ContextType) -> Result<(), CreationError> {
        if let Some(c) = *c {
            match ct {
                ContextType::OsMesa => {
                    match *c {
                        Context::OsMesa(_) => Ok(()),
                        _ => {
                            let msg = "Cannot share an OSMesa context with a non-OSMesa context";
                            return Err(CreationError::PlatformSpecific(msg.into()));
                        }
                    }
                }
                ContextType::X11 => {
                    match *c {
                        Context::WindowedX11(_) | Context::HeadlessX11(_, _) => Ok(()),
                        _ => {
                            let msg = "Cannot share an X11 context with a non-X11 context";
                            return Err(CreationError::PlatformSpecific(msg.into()));
                        }
                    }
                }
                ContextType::Wayland => {
                    match *c {
                        Context::WindowedWayland(_) | Context::HeadlessWayland(_, _) => Ok(()),
                        _ => {
                            let msg = "Cannot share a Wayland context with a non-Wayland context";
                            return Err(CreationError::PlatformSpecific(msg.into()));
                        }
                    }
                }
            }
        } else {
            Ok(())
        }
    }

    #[inline]
    pub unsafe fn new(
        window_builder: winit::WindowBuilder,
        events_loop: &winit::EventsLoop,
        pf_reqs: &PixelFormatRequirements,
        gl_attr: &GlAttributes<&Context>,
    ) -> Result<(winit::Window, Self), CreationError>
    {
        if events_loop.is_wayland() {
            Context::is_compatible(&gl_attr.sharing, ContextType::Wayland)?;

            let gl_attr = gl_attr.clone().map_sharing(|ctxt| match ctxt {
                &Context::WindowedWayland(ref ctxt) | &Context::HeadlessWayland(_, ref ctxt) => ctxt,
                _ => unreachable!(),
            });
            wayland::Context::new(window_builder, events_loop, pf_reqs, &gl_attr)
                .map(|(window, context)| (window, Context::WindowedWayland(context)))
        } else {
            Context::is_compatible(&gl_attr.sharing, ContextType::X11)?;
            let gl_attr = gl_attr.clone().map_sharing(|ctxt| match ctxt {
                &Context::WindowedX11(ref ctxt) | &Context::HeadlessX11(_, ref ctxt) => ctxt,
                _ => unreachable!(),
            });
            x11::Context::new(window_builder, events_loop, pf_reqs, &gl_attr)
                .map(|(window, context)| (window, Context::WindowedX11(context)))
        }
    }

    #[inline]
    pub unsafe fn new_context(
        el: &winit::EventsLoop,
        pf_reqs: &PixelFormatRequirements,
        gl_attr: &GlAttributes<&Context>,
        shareable_with_windowed_contexts: bool,
    ) -> Result<Self, CreationError>
    {
        if shareable_with_windowed_contexts {
            let wb = winit::WindowBuilder::new().with_visibility(false);

            if el.is_wayland() {
                Context::is_compatible(&gl_attr.sharing, ContextType::Wayland)?;
                let gl_attr = gl_attr.clone().map_sharing(|ctxt| match ctxt {
                    &Context::WindowedWayland(ref ctxt) | &Context::HeadlessWayland(_, ref ctxt) => ctxt,
                    _ => unreachable!(),
                });
                wayland::Context::new(wb, &el, pf_reqs, &gl_attr)
                    .map(|(window, context)| Context::HeadlessWayland(window, context))
            } else {
                Context::is_compatible(&gl_attr.sharing, ContextType::X11)?;
                let gl_attr = gl_attr.clone().map_sharing(|ctxt| match ctxt {
                    &Context::WindowedX11(ref ctxt) | &Context::HeadlessX11(_, ref ctxt) => ctxt,
                    _ => unreachable!(),
                });
                x11::Context::new(wb, &el, pf_reqs, &gl_attr)
                    .map(|(window, context)| Context::HeadlessX11(window, context))
            }
        } else {
            Context::is_compatible(&gl_attr.sharing, ContextType::OsMesa)?;
            let gl_attr = gl_attr.clone().map_sharing(|ctxt| match ctxt {
                &Context::OsMesa(ref ctxt) => ctxt,
                _ => unreachable!(),
            });
            osmesa::OsMesaContext::new((1, 1), pf_reqs, &gl_attr)
                .map(|context| Context::OsMesa(context))
        }
    }

    #[inline]
    pub fn resize(&self, width: u32, height: u32) {
        match *self {
            Context::WindowedX11(_) => (),
            Context::WindowedWayland(ref ctxt) => ctxt.resize(width, height),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub unsafe fn make_current(&self) -> Result<(), ContextError> {
        match *self {
            Context::WindowedX11(ref ctxt) | Context::HeadlessX11(_, ref ctxt) => ctxt.make_current(),
            Context::WindowedWayland(ref ctxt) | Context::HeadlessWayland(_, ref ctxt) => ctxt.make_current(),
            Context::OsMesa(ref ctxt) => ctxt.make_current(),
        }
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        match *self {
            Context::WindowedX11(ref ctxt) | Context::HeadlessX11(_, ref ctxt) => ctxt.is_current(),
            Context::WindowedWayland(ref ctxt) | Context::HeadlessWayland(_, ref ctxt) => ctxt.is_current(),
            Context::OsMesa(ref ctxt) => ctxt.is_current(),
        }
    }

    #[inline]
    pub fn get_proc_address(&self, addr: &str) -> *const () {
        match *self {
            Context::WindowedX11(ref ctxt) | Context::HeadlessX11(_, ref ctxt) => ctxt.get_proc_address(addr),
            Context::WindowedWayland(ref ctxt) | Context::HeadlessWayland(_, ref ctxt) => ctxt.get_proc_address(addr),
            Context::OsMesa(ref ctxt) => ctxt.get_proc_address(addr),
        }
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<(), ContextError> {
        match *self {
            Context::WindowedX11(ref ctxt) => ctxt.swap_buffers(),
            Context::WindowedWayland(ref ctxt) => ctxt.swap_buffers(),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn get_api(&self) -> ::Api {
        match *self {
            Context::WindowedX11(ref ctxt) | Context::HeadlessX11(_, ref ctxt) => ctxt.get_api(),
            Context::WindowedWayland(ref ctxt) | Context::HeadlessWayland(_, ref ctxt) => ctxt.get_api(),
            Context::OsMesa(ref ctxt) => ctxt.get_api(),
        }
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        match *self {
            Context::WindowedX11(ref ctxt) => ctxt.get_pixel_format(),
            Context::WindowedWayland(ref ctxt) => ctxt.get_pixel_format(),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub unsafe fn raw_handle(&self) -> RawHandle {
        match *self {
            Context::WindowedX11(ref ctxt) | Context::HeadlessX11(_, ref ctxt) => match *ctxt.raw_handle() {
                GlContext::Glx(ref ctxt) => RawHandle::Glx(ctxt.raw_handle()),
                GlContext::Egl(ref ctxt) => RawHandle::Egl(ctxt.raw_handle()),
                GlContext::None => panic!()
            },
            Context::WindowedWayland(ref ctxt) | Context::HeadlessWayland(_, ref ctxt) => RawHandle::Egl(ctxt.raw_handle()),
            Context::OsMesa(ref ctxt) => RawHandle::Egl(ctxt.raw_handle()),
        }
    }
}
