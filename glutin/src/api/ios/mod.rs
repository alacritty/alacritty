//! iOS support
//!
//! # Building app
//! To build ios app you will need rustc built for this targets:
//!
//!  - armv7-apple-ios
//!  - armv7s-apple-ios
//!  - i386-apple-ios
//!  - aarch64-apple-ios
//!  - x86_64-apple-ios
//!
//! Then
//!
//! ```
//! cargo build --target=...
//! ```
//! The simplest way to integrate your app into xcode environment is to build it
//! as a static library. Wrap your main function and export it.
//!
//! ```rust, ignore
//! #[no_mangle]
//! pub extern fn start_glutin_app() {
//!     start_inner()
//! }
//!
//! fn start_inner() {
//!    ...
//! }
//!
//! ```
//!
//! Compile project and then drag resulting .a into Xcode project. Add glutin.h to xcode.
//!
//! ```c
//! void start_glutin_app();
//! ```
//!
//! Use start_glutin_app inside your xcode's main function.
//!
//!
//! # App lifecycle and events
//!
//! iOS environment is very different from other platforms and you must be very
//! careful with it's events. Familiarize yourself with [app lifecycle](https://developer.apple.com/library/ios/documentation/UIKit/Reference/UIApplicationDelegate_Protocol/).
//!
//!
//! This is how those event are represented in glutin:
//!
//!  - applicationDidBecomeActive is Focused(true)
//!  - applicationWillResignActive is Focused(false)
//!  - applicationDidEnterBackground is Suspended(true)
//!  - applicationWillEnterForeground is Suspended(false)
//!  - applicationWillTerminate is Destroyed
//!
//! Keep in mind that after Destroyed event is received every attempt to draw with opengl will result in segfault.
//!
//! Also note that app will not receive Destroyed event if suspended, it will be SIGKILL'ed

#![cfg(target_os = "ios")]

use std::io;
use std::ffi::CString;
use std::mem;
use std::os::raw::*;

use objc::declare::ClassDecl;
use objc::runtime::{BOOL, Class, NO, Object, Sel, YES};

use {
    Api,
    ContextError,
    CreationError,
    EventsLoop,
    GlAttributes,
    GlRequest,
    PixelFormat,
    PixelFormatRequirements,
    Window,
    WindowBuilder,
};
use os::GlContextExt;
use os::ios::{WindowExt, WindowBuilderExt};

mod ffi;
use self::ffi::*;
pub use self::ffi::id;

#[derive(Debug, PartialEq)]
enum ColorFormat {
    Rgba8888 = 0,
    Rgb565 = 1,
    Srgba8888 = 2,
}

impl ColorFormat {
    #[allow(non_upper_case_globals)]
    pub fn for_view(view: id) -> Self {
        let color_format: NSUInteger = unsafe { msg_send![view, drawableColorFormat] };
        match color_format{
            GLKViewDrawableColorFormatRGBA8888 => ColorFormat::Rgba8888,
            GLKViewDrawableColorFormatRGB565 => ColorFormat::Rgb565,
            GLKViewDrawableColorFormatSRGBA8888 => ColorFormat::Srgba8888,
            _ => unreachable!(),
        }
    }

    pub fn color_bits(&self) -> u8 {
        if *self == ColorFormat::Rgba8888 || *self == ColorFormat::Srgba8888 {
            8
        } else {
            16
        }
    }

    pub fn alpha_bits(&self) -> u8 {
        if *self == ColorFormat::Rgba8888 || *self == ColorFormat::Srgba8888 {
            8
        } else {
            0
        }
    }

    pub fn srgb(&self) -> bool {
        *self == ColorFormat::Srgba8888
    }
}

#[allow(non_upper_case_globals)]
fn depth_for_view(view: id) -> u8 {
    let depth_format: NSUInteger = unsafe { msg_send![view, drawableDepthFormat] };
    match depth_format {
        GLKViewDrawableDepthFormatNone => 0,
        GLKViewDrawableDepthFormat16 => 16,
        GLKViewDrawableDepthFormat24 => 24,
        _ => unreachable!(),
    }
}

#[allow(non_upper_case_globals)]
fn stencil_for_view(view: id) -> u8 {
    let stencil_format: NSUInteger = unsafe { msg_send![view, drawableStencilFormat] };
    match stencil_format {
        GLKViewDrawableStencilFormatNone => 0,
        GLKViewDrawableStencilFormat8 => 8,
        _ => unreachable!(),
    }
}

#[allow(non_upper_case_globals)]
fn multisampling_for_view(view: id) -> Option<u16> {
    let ms_format: NSUInteger = unsafe { msg_send![view, drawableMultisample] };
    match ms_format {
        GLKViewDrawableMultisampleNone => None,
        GLKViewDrawableMultisample4X => Some(4),
        _ => unreachable!(),
    }
}

pub struct Context {
    eagl_context: id,
    view: id, // this will be invalid after the `EventsLoop` is dropped
}

fn validate_version(version: u8) -> Result<NSUInteger, CreationError> {
    let version = version as NSUInteger;
    if version >= kEAGLRenderingAPIOpenGLES1 && version <= kEAGLRenderingAPIOpenGLES3 {
        Ok(version)
    } else {
        Err(CreationError::OsError(format!(
            "Specified OpenGL ES version ({:?}) is not availble on iOS. Only 1, 2, and 3 are valid options",
            version,
        )))
    }
}

impl Context {
    pub fn new(
        builder: WindowBuilder,
        event_loop: &EventsLoop,
        _: &PixelFormatRequirements,
        gl_attrs: &GlAttributes<&Context>,
    ) -> Result<(Window, Self), CreationError> {
        create_view_class();
        let view_class = Class::get("MainGLView").expect("Failed to get class `MainGLView`");
        let builder = builder.with_root_view_class(view_class as *const _ as *const _);
        if gl_attrs.sharing.is_some() { unimplemented!("Shared contexts are unimplemented on iOS."); }
        let version = match gl_attrs.version {
            GlRequest::Latest => kEAGLRenderingAPIOpenGLES3,
            GlRequest::Specific(api, (major, _minor)) => if api == Api::OpenGlEs {
                validate_version(major)?
            } else {
                return Err(CreationError::OsError(format!(
                    "Specified API ({:?}) is not availble on iOS. Only `Api::OpenGlEs` can be used",
                    api,
                )));
            },
            GlRequest::GlThenGles { opengles_version: (major, _minor), .. } => {
                validate_version(major)?
            },
        };
        let window = builder.build(event_loop)?;
        let context = unsafe {
            let eagl_context = Context::create_context(version)?;
            let view = window.get_uiview() as id;
            let mut context = Context { eagl_context, view };
            context.init_context(&window);
            context
        };
        Ok((window, context))
    }

    pub fn new_context(
        el: &EventsLoop,
        pf_reqs: &PixelFormatRequirements,
        gl_attr: &GlAttributes<&Context>,
        _shareable_with_windowed_contexts: bool,
    ) -> Result<Self, CreationError> {
        let wb = WindowBuilder::new().with_visibility(false);
        Self::new(wb, el, pf_reqs, gl_attr)
            .map(|(_window, context)| context)
    }

    unsafe fn create_context(mut version: NSUInteger) -> Result<id, CreationError> {
        let context_class = Class::get("EAGLContext").expect("Failed to get class `EAGLContext`");
        let eagl_context: id = msg_send![context_class, alloc];
        let mut valid_context = nil;
        while valid_context == nil && version > 0 {
            valid_context = msg_send![eagl_context, initWithAPI:version];
            version -= 1;
        }
        if valid_context == nil {
            Err(CreationError::OsError(format!("Failed to create an OpenGL ES context with any version")))
        } else {
            Ok(eagl_context)
        }
    }

    unsafe fn init_context(&mut self, window: &Window) {
        let dict_class = Class::get("NSDictionary").expect("Failed to get class `NSDictionary`");
        let number_class = Class::get("NSNumber").expect("Failed to get class `NSNumber`");
        let draw_props: id = msg_send![dict_class, alloc];
        let draw_props: id = msg_send![draw_props,
            initWithObjects:
                vec![
                    msg_send![number_class, numberWithBool:NO],
                    kEAGLColorFormatRGB565,
                ].as_ptr()
            forKeys:
                vec![
                    kEAGLDrawablePropertyRetainedBacking,
                    kEAGLDrawablePropertyColorFormat,
                ].as_ptr()
            count: 2
        ];
        let _ = self.make_current();

        let view = self.view;
        let scale_factor = window.get_hidpi_factor() as CGFloat;
        let _: () = msg_send![view, setContentScaleFactor:scale_factor];
        let layer: id = msg_send![view, layer];
        let _: () = msg_send![layer, setContentsScale:scale_factor];
        let _: () = msg_send![layer, setDrawableProperties:draw_props];

        let gl = gles::Gles2::load_with(|symbol| self.get_proc_address(symbol) as *const c_void);
        let mut color_render_buf: gles::types::GLuint = 0;
        let mut frame_buf: gles::types::GLuint = 0;
        gl.GenRenderbuffers(1, &mut color_render_buf);
        gl.BindRenderbuffer(gles::RENDERBUFFER, color_render_buf);

        let ok: BOOL = msg_send![self.eagl_context, renderbufferStorage:gles::RENDERBUFFER fromDrawable:layer];
        if ok != YES {
            panic!("EAGL: could not set renderbufferStorage");
        }

        gl.GenFramebuffers(1, &mut frame_buf);
        gl.BindFramebuffer(gles::FRAMEBUFFER, frame_buf);

        gl.FramebufferRenderbuffer(gles::FRAMEBUFFER, gles::COLOR_ATTACHMENT0, gles::RENDERBUFFER, color_render_buf);

        let status = gl.CheckFramebufferStatus(gles::FRAMEBUFFER);
        if gl.CheckFramebufferStatus(gles::FRAMEBUFFER) != gles::FRAMEBUFFER_COMPLETE {
            panic!("framebuffer status: {:?}", status);
        }
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<(), ContextError> {
        unsafe {
            let res: BOOL = msg_send![self.eagl_context, presentRenderbuffer:gles::RENDERBUFFER];
            if res == YES {
                Ok(())
            } else {
                Err(ContextError::IoError(
                    io::Error::new(io::ErrorKind::Other, "`EAGLContext presentRenderbuffer` failed")
                ))
            }
        }
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        let color_format = ColorFormat::for_view(self.view);
        PixelFormat {
            hardware_accelerated: true,
            color_bits: color_format.color_bits(),
            alpha_bits: color_format.alpha_bits(),
            depth_bits: depth_for_view(self.view),
            stencil_bits: stencil_for_view(self.view),
            stereoscopy: false,
            double_buffer: true,
            multisampling: multisampling_for_view(self.view),
            srgb: color_format.srgb(),
        }
    }

    #[inline]
    pub fn resize(&self, _width: u32, _height: u32) {
        // N/A
    }

    #[inline]
    pub unsafe fn make_current(&self) -> Result<(), ContextError> {
        let context_class = Class::get("EAGLContext").expect("Failed to get class `EAGLContext`");
        let res: BOOL = msg_send![context_class, setCurrentContext: self.eagl_context];
        if res == YES {
            Ok(())
        } else {
            Err(ContextError::IoError(
                io::Error::new(io::ErrorKind::Other, "`EAGLContext setCurrentContext` failed")
            ))
        }
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        // TODO: This can likely be implemented using `currentContext`/`getCurrentContext`
        true
    }

    #[inline]
    pub fn get_proc_address(&self, proc_name: &str) -> *const () {
        let proc_name_c = CString::new(proc_name).expect("proc name contained interior nul byte");
        let path = b"/System/Library/Frameworks/OpenGLES.framework/OpenGLES\0";
        let addr = unsafe {
            let lib = dlopen(path.as_ptr() as *const c_char, RTLD_LAZY | RTLD_GLOBAL);
            dlsym(lib, proc_name_c.as_ptr()) as *const _
        };
        //debug!("proc {} -> {:?}", proc_name, addr);
        addr
    }

    #[inline]
    pub fn get_api(&self) -> Api {
        Api::OpenGlEs
    }
}

fn create_view_class() {
    extern fn init_with_frame(this: &Object, _: Sel, frame: CGRect) -> id {
        unsafe {
            let view: id = msg_send![super(this, class!(GLKView)), initWithFrame:frame];

            let mask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
            let _: () = msg_send![view, setAutoresizingMask:mask];
            let _: () = msg_send![view, setAutoresizesSubviews:YES];

            let layer: id = msg_send![view, layer];
            let _ : () = msg_send![layer, setOpaque:YES];

            view
        }
    }

    extern fn layer_class(_: &Class, _: Sel) -> *const Class {
        unsafe { mem::transmute(Class::get("CAEAGLLayer").expect("Failed to get class `CAEAGLLayer`")) }
    }

    let superclass = Class::get("GLKView").expect("Failed to get class `GLKView`");
    let mut decl = ClassDecl::new("MainGLView", superclass).expect("Failed to declare class `MainGLView`");
    unsafe {
        decl.add_method(sel!(initWithFrame:), init_with_frame as extern fn(&Object, Sel, CGRect) -> id);
        decl.add_class_method(sel!(layerClass), layer_class as extern fn(&Class, Sel) -> *const Class);
        decl.register();
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        let _: () = unsafe { msg_send![self.eagl_context, release] };
    }
}

impl GlContextExt for Context {
    type Handle = *mut c_void;
    #[inline]
    unsafe fn raw_handle(&self) -> *mut c_void {
        self.eagl_context as *mut c_void
    }
}
