#![cfg(target_os = "macos")]

pub use winit::MonitorId;

use CreationError;
use ContextError;
use GlAttributes;
use PixelFormat;
use PixelFormatRequirements;
use Robustness;

use cgl::{CGLEnable, kCGLCECrashOnRemovedFunctions, CGLSetParameter, kCGLCPSurfaceOpacity};
use cocoa::appkit::{self, NSOpenGLContext, NSOpenGLPixelFormat};
use cocoa::base::{id, nil};
use cocoa::foundation::NSAutoreleasePool;
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation::bundle::{CFBundleGetBundleWithIdentifier, CFBundleGetFunctionPointerForName};
use objc::runtime::{BOOL, NO};
use winit;
use winit::os::macos::WindowExt;

use std::str::FromStr;
use std::ops::Deref;
use std::os::raw::c_void;

mod helpers;

pub enum Context {
    WindowedContext(WindowedContext),
    HeadlessContext(HeadlessContext),
}

pub struct WindowedContext {
    // NSOpenGLContext
    context: IdRef,
    pixel_format: PixelFormat,
}

pub struct HeadlessContext {
    context: IdRef,
}

impl Context {
    pub fn new(
        window_builder: winit::WindowBuilder,
        events_loop: &winit::EventsLoop,
        pf_reqs: &PixelFormatRequirements,
        gl_attr: &GlAttributes<&Context>,
    ) -> Result<(winit::Window, Self), CreationError>
    {
        let transparent = window_builder.window.transparent;
        let window = window_builder.build(events_loop)?;

        if gl_attr.sharing.is_some() {
            unimplemented!()
        }

        match gl_attr.robustness {
            Robustness::RobustNoResetNotification |
            Robustness::RobustLoseContextOnReset => {
                return Err(CreationError::RobustnessNotSupported);
            }
            _ => (),
        }

        let view = window.get_nsview() as id;

        let gl_profile = helpers::get_gl_profile(gl_attr, pf_reqs)?;
        let attributes = helpers::build_nsattributes(pf_reqs, gl_profile)?;
        unsafe {
            let pixel_format = IdRef::new(NSOpenGLPixelFormat::alloc(nil)
                .initWithAttributes_(&attributes));
            let pixel_format = match pixel_format.non_nil() {
                None => return Err(CreationError::NoAvailablePixelFormat),
                Some(pf) => pf,
            };

            // TODO: Add context sharing
            let gl_context = IdRef::new(NSOpenGLContext::alloc(nil)
                .initWithFormat_shareContext_(*pixel_format, nil));
            let gl_context = match gl_context.non_nil() {
                Some(gl_context) => gl_context,
                None => return Err(CreationError::NotSupported("could not open gl context")),
            };

            let pixel_format = {
                let get_attr = |attrib: appkit::NSOpenGLPixelFormatAttribute| -> i32 {
                    let mut value = 0;
                    NSOpenGLPixelFormat::getValues_forAttribute_forVirtualScreen_(
                        *pixel_format,
                        &mut value,
                        attrib,
                        NSOpenGLContext::currentVirtualScreen(*gl_context));
                    value
                };

                PixelFormat {
                    hardware_accelerated: get_attr(appkit::NSOpenGLPFAAccelerated) != 0,
                    color_bits: (get_attr(appkit::NSOpenGLPFAColorSize) - get_attr(appkit::NSOpenGLPFAAlphaSize)) as u8,
                    alpha_bits: get_attr(appkit::NSOpenGLPFAAlphaSize) as u8,
                    depth_bits: get_attr(appkit::NSOpenGLPFADepthSize) as u8,
                    stencil_bits: get_attr(appkit::NSOpenGLPFAStencilSize) as u8,
                    stereoscopy: get_attr(appkit::NSOpenGLPFAStereo) != 0,
                    double_buffer: get_attr(appkit::NSOpenGLPFADoubleBuffer) != 0,
                    multisampling: if get_attr(appkit::NSOpenGLPFAMultisample) > 0 {
                        Some(get_attr(appkit::NSOpenGLPFASamples) as u16)
                    } else {
                        None
                    },
                    srgb: true,
                }
            };

            gl_context.setView_(view);
            let value = if gl_attr.vsync { 1 } else { 0 };
            gl_context.setValues_forParameter_(&value, appkit::NSOpenGLContextParameter::NSOpenGLCPSwapInterval);

            if transparent {
                let mut opacity = 0;
                CGLSetParameter(gl_context.CGLContextObj() as *mut _, kCGLCPSurfaceOpacity, &mut opacity);
            }

            CGLEnable(gl_context.CGLContextObj() as *mut _, kCGLCECrashOnRemovedFunctions);

            let context = WindowedContext { context: gl_context, pixel_format: pixel_format };
            Ok((window, Context::WindowedContext(context)))
        }
    }

    #[inline]
    pub fn new_context(
        _el: &winit::EventsLoop,
        pf_reqs: &PixelFormatRequirements,
        gl_attr: &GlAttributes<&Context>,
        shareable_with_windowed_contexts: bool,
    ) -> Result<Self, CreationError> {
        assert!(!shareable_with_windowed_contexts); // TODO: Implement if possible

        let gl_profile = helpers::get_gl_profile(gl_attr, pf_reqs)?;
        let attributes = helpers::build_nsattributes(pf_reqs, gl_profile)?;
        let context = unsafe {
            let pixelformat = NSOpenGLPixelFormat::alloc(nil).initWithAttributes_(&attributes);
            if pixelformat == nil {
                return Err(CreationError::OsError(format!("Could not create the pixel format")));
            }
            let context = NSOpenGLContext::alloc(nil).initWithFormat_shareContext_(pixelformat, nil);
            if context == nil {
                return Err(CreationError::OsError(format!("Could not create the rendering context")));
            }

            IdRef::new(context)
        };

        let headless = HeadlessContext {
            context,
        };

        Ok(Context::HeadlessContext(headless))
    }

    pub fn resize(&self, _width: u32, _height: u32) {
        match *self {
            Context::WindowedContext(ref c) => unsafe { c.context.update() },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub unsafe fn make_current(&self) -> Result<(), ContextError> {
        match *self {
            Context::WindowedContext(ref c) => {
                let _: () = msg_send![*c.context, update];
                c.context.makeCurrentContext();
            }
            Context::HeadlessContext(ref c) => {
                let _: () = msg_send![*c.context, update];
                c.context.makeCurrentContext();
            }
        }
        Ok(())
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        unsafe {
            let context = match *self {
                Context::WindowedContext(ref c) => &c.context,
                Context::HeadlessContext(ref c) => &c.context,
            };

            let pool = NSAutoreleasePool::new(nil);
            let current = NSOpenGLContext::currentContext(nil);
            let res = if current != nil {
                let is_equal: BOOL = msg_send![current, isEqual: context];
                is_equal != NO
            } else {
                false
            };
            let _: () = msg_send![pool, release];
            res
        }
    }

    pub fn get_proc_address(&self, addr: &str) -> *const () {
        let symbol_name: CFString = FromStr::from_str(addr).unwrap();
        let framework_name: CFString = FromStr::from_str("com.apple.opengl").unwrap();
        let framework =
            unsafe { CFBundleGetBundleWithIdentifier(framework_name.as_concrete_TypeRef()) };
        let symbol = unsafe {
            CFBundleGetFunctionPointerForName(framework, symbol_name.as_concrete_TypeRef())
        };
        symbol as *const _
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<(), ContextError> {
        unsafe {
            match *self {
                Context::WindowedContext(ref c) => {
                    let pool = NSAutoreleasePool::new(nil);
                    c.context.flushBuffer();
                    let _: () = msg_send![pool, release];
                }
                Context::HeadlessContext(_) => unreachable!(),
            }
        }
        Ok(())
    }

    #[inline]
    pub fn get_api(&self) -> ::Api {
        ::Api::OpenGl
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        match *self {
            Context::WindowedContext(ref c) => c.pixel_format.clone(),
            Context::HeadlessContext(_) => unreachable!(),
        }
    }

    #[inline]
    pub unsafe fn raw_handle(&self) -> *mut c_void {
        match *self {
            Context::WindowedContext(ref c) => *c.context.deref() as *mut _,
            Context::HeadlessContext(ref c) => *c.context.deref() as *mut _,
        }
    }
}

struct IdRef(id);

impl IdRef {
    fn new(i: id) -> IdRef {
        IdRef(i)
    }

    #[allow(dead_code)]
    fn retain(i: id) -> IdRef {
        if i != nil {
            let _: id = unsafe { msg_send![i, retain] };
        }
        IdRef(i)
    }

    fn non_nil(self) -> Option<IdRef> {
        if self.0 == nil { None } else { Some(self) }
    }
}

impl Drop for IdRef {
    fn drop(&mut self) {
        if self.0 != nil {
            let _: () = unsafe { msg_send![self.0, release] };
        }
    }
}

impl Deref for IdRef {
    type Target = id;
    fn deref<'a>(&'a self) -> &'a id {
        &self.0
    }
}

impl Clone for IdRef {
    fn clone(&self) -> IdRef {
        if self.0 != nil {
            let _: id = unsafe { msg_send![self.0, retain] };
        }
        IdRef(self.0)
    }
}
