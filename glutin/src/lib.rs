//! The purpose of this library is to provide an OpenGL context on as many platforms as possible.
//!
//! # Building a GlWindow
//!
//! A `GlWindow` is composed of a `Window` and an OpenGL `Context`. Due to some
//! operating-system-specific quirks, glutin requires control over the order of creation of the
//! `Context` and `Window`. Here is an example of building a GlWindow:
//!
//! ```no_run
//! # extern crate glutin;
//! # fn main() {
//! let events_loop = glutin::EventsLoop::new();
//! let window = glutin::WindowBuilder::new()
//!     .with_title("Hello world!")
//!     .with_dimensions(glutin::dpi::LogicalSize::new(1024.0, 768.0));
//! let context = glutin::ContextBuilder::new();
//! let gl_window = glutin::GlWindow::new(window, context, &events_loop).unwrap();
//! # }
//! ```
//!
//! # Features
//!
//! This crate has one Cargo feature: `window`.
//!
//!  - `window` allows you to create regular windows and enables the `WindowBuilder` object.
//!
//! By default `window` is enabled.

#[cfg(target_os = "windows")]
#[macro_use]
extern crate lazy_static;

#[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]
#[macro_use]
extern crate shared_library;

extern crate libc;

extern crate winit;

#[cfg(target_os = "windows")]
extern crate winapi;
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[macro_use]
extern crate objc;
#[cfg(target_os = "macos")]
extern crate cgl;
#[cfg(target_os = "macos")]
extern crate cocoa;
#[cfg(target_os = "macos")]
extern crate core_foundation;
#[cfg(target_os = "macos")]
extern crate core_graphics;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly", target_os = "openbsd"))]
extern crate x11_dl;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly", target_os = "openbsd"))]
extern crate wayland_client;

pub use winit::{
    AvailableMonitorsIter,
    AxisId,
    ButtonId,
    ControlFlow,
    CreationError as WindowCreationError,
    DeviceEvent,
    DeviceId,
    dpi,
    ElementState,
    Event,
    EventsLoop,
    EventsLoopClosed,
    EventsLoopProxy,
    Icon,
    KeyboardInput,
    ModifiersState,
    MonitorId,
    MouseButton,
    MouseCursor,
    MouseScrollDelta,
    ScanCode,
    Touch,
    TouchPhase,
    VirtualKeyCode,
    Window,
    WindowAttributes,
    WindowBuilder,
    WindowEvent,
    WindowId,
};

use std::io;

mod api;
mod platform;

pub mod os;

/// A trait for types associated with a GL context.
pub trait GlContext
where
    Self: Sized,
{
    /// Sets the context as the current context.
    unsafe fn make_current(&self) -> Result<(), ContextError>;

    /// Returns true if this context is the current one in this thread.
    fn is_current(&self) -> bool;

    /// Returns the address of an OpenGL function.
    fn get_proc_address(&self, addr: &str) -> *const ();

    /// Returns the OpenGL API being used.
    fn get_api(&self) -> Api;
}

/// Represents an OpenGL context.
///
/// A `Context` is normally associated with a single Window, however `Context`s can be *shared*
/// between multiple windows.
///
/// # Example
///
/// ```no_run
/// # extern crate glutin;
/// # use glutin::GlContext;
/// # fn main() {
/// # let events_loop = glutin::EventsLoop::new();
/// # let window = glutin::WindowBuilder::new();
/// # let context = glutin::ContextBuilder::new();
/// # let some_gl_window = glutin::GlWindow::new(window, context, &events_loop).unwrap();
/// let context = glutin::ContextBuilder::new()
///     .with_vsync(true)
///     .with_multisampling(8)
///     .with_shared_lists(some_gl_window.context());
/// # }
/// ```
pub struct Context {
    context: platform::Context,
}

/// Object that allows you to build `Context`s.
pub struct ContextBuilder<'a> {
    /// The attributes to use to create the context.
    pub gl_attr: GlAttributes<&'a Context>,
    // Should be made public once it's stabilized.
    pf_reqs: PixelFormatRequirements,
}

/// Represents an OpenGL context and a Window with which it is associated.
///
/// # Example
///
/// ```no_run
/// # extern crate glutin;
/// # use glutin::GlContext;
/// # fn main() {
/// let mut events_loop = glutin::EventsLoop::new();
/// let window = glutin::WindowBuilder::new();
/// let context = glutin::ContextBuilder::new();
/// let gl_window = glutin::GlWindow::new(window, context, &events_loop).unwrap();
///
/// unsafe { gl_window.make_current().unwrap() };
///
/// loop {
///     events_loop.poll_events(|event| {
///         match event {
///             // process events here
///             _ => ()
///         }
///     });
///
///     // draw everything here
///
///     gl_window.swap_buffers();
///     std::thread::sleep(std::time::Duration::from_millis(17));
/// }
/// # }
/// ```
pub struct GlWindow {
    context: Context,
    window: Window,
}

impl<'a> ContextBuilder<'a> {
    /// Initializes a new `ContextBuilder` with default values.
    pub fn new() -> Self {
        ContextBuilder {
            pf_reqs: std::default::Default::default(),
            gl_attr: std::default::Default::default(),
        }
    }

    /// Sets how the backend should choose the OpenGL API and version.
    #[inline]
    pub fn with_gl(mut self, request: GlRequest) -> Self {
        self.gl_attr.version = request;
        self
    }

    /// Sets the desired OpenGL context profile.
    #[inline]
    pub fn with_gl_profile(mut self, profile: GlProfile) -> Self {
        self.gl_attr.profile = Some(profile);
        self
    }

    /// Sets the *debug* flag for the OpenGL context.
    ///
    /// The default value for this flag is `cfg!(debug_assertions)`, which means that it's enabled
    /// when you run `cargo build` and disabled when you run `cargo build --release`.
    #[inline]
    pub fn with_gl_debug_flag(mut self, flag: bool) -> Self {
        self.gl_attr.debug = flag;
        self
    }

    /// Sets the robustness of the OpenGL context. See the docs of `Robustness`.
    #[inline]
    pub fn with_gl_robustness(mut self, robustness: Robustness) -> Self {
        self.gl_attr.robustness = robustness;
        self
    }

    /// Requests that the window has vsync enabled.
    ///
    /// By default, vsync is not enabled.
    #[inline]
    pub fn with_vsync(mut self, vsync: bool) -> Self {
        self.gl_attr.vsync = vsync;
        self
    }

    /// Share the display lists with the given `Context`.
    #[inline]
    pub fn with_shared_lists(mut self, other: &'a Context) -> Self {
        self.gl_attr.sharing = Some(other);
        self
    }

    /// Sets the multisampling level to request. A value of `0` indicates that multisampling must
    /// not be enabled.
    ///
    /// # Panic
    ///
    /// Will panic if `samples` is not a power of two.
    #[inline]
    pub fn with_multisampling(mut self, samples: u16) -> Self {
        self.pf_reqs.multisampling = match samples {
            0 => None,
            _ => {
                assert!(samples.is_power_of_two());
                Some(samples)
            }
        };
        self
    }

    /// Sets the number of bits in the depth buffer.
    #[inline]
    pub fn with_depth_buffer(mut self, bits: u8) -> Self {
        self.pf_reqs.depth_bits = Some(bits);
        self
    }

    /// Sets the number of bits in the stencil buffer.
    #[inline]
    pub fn with_stencil_buffer(mut self, bits: u8) -> Self {
        self.pf_reqs.stencil_bits = Some(bits);
        self
    }

    /// Sets the number of bits in the color buffer.
    #[inline]
    pub fn with_pixel_format(mut self, color_bits: u8, alpha_bits: u8) -> Self {
        self.pf_reqs.color_bits = Some(color_bits);
        self.pf_reqs.alpha_bits = Some(alpha_bits);
        self
    }

    /// Request the backend to be stereoscopic.
    #[inline]
    pub fn with_stereoscopy(mut self) -> Self {
        self.pf_reqs.stereoscopy = true;
        self
    }

    /// Sets whether sRGB should be enabled on the window.
    ///
    /// The default value is `false`.
    #[inline]
    pub fn with_srgb(mut self, srgb_enabled: bool) -> Self {
        self.pf_reqs.srgb = srgb_enabled;
        self
    }

    /// Sets whether double buffering should be enabled.
    ///
    /// The default value is `None`.
    ///
    /// ## Platform-specific
    ///
    /// This option will be taken into account on the following platforms:
    ///
    ///   * MacOS
    ///   * Linux using GLX with X
    ///   * Windows using WGL
    ///
    #[inline]
    pub fn with_double_buffer(mut self, double_buffer: Option<bool>) -> Self {
        self.pf_reqs.double_buffer = double_buffer;
        self
    }

    /// Sets whether hardware acceleration is required.
    ///
    /// The default value is `Some(true)`
    ///
    /// ## Platform-specific
    ///
    /// This option will be taken into account on the following platforms:
    ///
    ///   * MacOS
    ///   * Linux using EGL with either X or Wayland
    ///   * Windows using EGL or WGL
    ///   * Android using EGL
    ///
    #[inline]
    pub fn with_hardware_acceleration(mut self, acceleration: Option<bool>) -> Self {
        self.pf_reqs.hardware_accelerated = acceleration;
        self
    }
}

impl GlWindow {
    /// Builds the given window along with the associated GL context, returning the pair as a
    /// `GlWindow`.
    ///
    /// The context made can be shared with:
    ///  - headless contexts made with the `shareable_with_windowed_contexts`
    ///  flag set to `true`; and
    ///  - contexts made when creating a `GlWindow`.
    ///
    /// You are not guaranteed to receive an error if you share a context with an
    /// other context which you're not permitted to share it with, as according
    /// to:
    ///  - the restrictions stated by us above; and
    ///  - the restrictions imposed on you by the platform your application runs
    ///  on. (Please refer to `README-SHARING.md`)
    ///
    /// Failing to follow all the context sharing restrictions imposed on you
    /// may result in unsafe behavior.
    ///
    /// This safe variant of `new_shared` will panic if you try to share it with
    /// an existing context.
    ///
    /// Error should be very rare and only occur in case of permission denied,
    /// incompatible system out of memory, etc.
    pub fn new(
        window_builder: WindowBuilder,
        context_builder: ContextBuilder,
        events_loop: &EventsLoop,
    ) -> Result<Self, CreationError>
    {
        let ContextBuilder { pf_reqs, gl_attr } = context_builder;
        let gl_attr = gl_attr.map_sharing(|_ctxt| panic!("Context sharing is not allowed when using `new()`. Please instead use `new_shared()`."));
        
        // Not all platforms support context sharing yet, when they do, their 
        // `new.*` functions should be marked unsafe.
        #[allow(unused_unsafe)] 
        unsafe {
            platform::Context::new(window_builder, events_loop, &pf_reqs, &gl_attr)
                .map(|(window, context)| GlWindow {
                    window,
                    context: Context { context },
                })
        }
    }

    /// Builds the given window along with the associated GL context, returning the pair as a
    /// `GlWindow`.
    ///
    /// The context made can be shared with:
    ///  - headless contexts made with the `shareable_with_windowed_contexts`
    ///  flag set to `true`; and
    ///  - contexts made when creating a `GlWindow`.
    ///
    /// You are not guaranteed to receive an error if you share a context with an
    /// other context which you're not permitted to share it with, as according
    /// to:
    ///  - the restrictions stated by us above; and
    ///  - the restrictions imposed on you by the platform your application runs
    ///  on. (Please refer to `README-SHARING.md`)
    ///
    /// Failing to follow all the context sharing restrictions imposed on you
    /// may result in unsafe behavior.
    ///
    /// Error should be very rare and only occur in case of permission denied,
    /// incompatible system out of memory, etc.
    pub unsafe fn new_shared(
        window_builder: WindowBuilder,
        context_builder: ContextBuilder,
        events_loop: &EventsLoop,
    ) -> Result<Self, CreationError>
    {
        let ContextBuilder { pf_reqs, gl_attr } = context_builder;
        let gl_attr = gl_attr.map_sharing(|ctxt| &ctxt.context);
        platform::Context::new(window_builder, events_loop, &pf_reqs, &gl_attr)
            .map(|(window, context)| GlWindow {
                window,
                context: Context { context },
            })
    }

    /// Borrow the inner `Window`.
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Borrow the inner GL `Context`.
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// Swaps the buffers in case of double or triple buffering.
    ///
    /// You should call this function every time you have finished rendering, or the image may not
    /// be displayed on the screen.
    ///
    /// **Warning**: if you enabled vsync, this function will block until the next time the screen
    /// is refreshed. However drivers can choose to override your vsync settings, which means that
    /// you can't know in advance whether `swap_buffers` will block or not.
    pub fn swap_buffers(&self) -> Result<(), ContextError> {
        self.context.context.swap_buffers()
    }

    /// Returns the pixel format of the main framebuffer of the context.
    pub fn get_pixel_format(&self) -> PixelFormat {
        self.context.context.get_pixel_format()
    }

    /// Resize the context.
    ///
    /// Some platforms (macOS, Wayland) require being manually updated when their window or
    /// surface is resized.
    ///
    /// The easiest way of doing this is to take every `Resized` window event that
    /// is received with a `LogicalSize` and convert it to a `PhysicalSize` and
    /// pass it into this function.
    pub fn resize(&self, size: dpi::PhysicalSize) {
        let (width, height) = size.into();
        self.context.context.resize(width, height);
    }
}

impl GlContext for Context {
    unsafe fn make_current(&self) -> Result<(), ContextError> {
        self.context.make_current()
    }

    fn is_current(&self) -> bool {
        self.context.is_current()
    }

    fn get_proc_address(&self, addr: &str) -> *const () {
        self.context.get_proc_address(addr)
    }

    fn get_api(&self) -> Api {
        self.context.get_api()
    }
}

impl Context {
    /// Builds the given GL context
    ///
    /// Contexts made with the `shareable_with_windowed_contexts` flag set to
    /// `true` can be shared with:
    ///  - contexts made with that flag set to `true`; and
    ///  - contexts made when creating a `GlWindow`.
    ///
    /// If the flag is set to `false` on the other hand, the context should only
    /// be shared with other contexts made with the flag set to `false`.
    ///
    /// Some platforms might not implement contexts which aren't shareable with
    /// windowed contexts. If so, those platforms will fallback to making a
    /// contexts which are shareable with windowed contexts.
    ///
    /// You are not guaranteed to receive an error if you share a context with an
    /// other context which you're not permitted to share it with, as according
    /// to:
    ///  - the restrictions stated by us above; and
    ///  - the restrictions imposed on you by the platform your application runs
    ///  on. (Please refer to `README-SHARING.md`)
    ///
    /// Failing to follow all the context sharing restrictions imposed on you
    /// may result in unsafe behavior.
    ///
    /// This safe variant of `new_shared` will panic if you try to share it with
    /// an existing context.
    ///
    /// Error should be very rare and only occur in case of permission denied,
    /// incompatible system, out of memory, etc.
    pub fn new(
        el: &winit::EventsLoop,
        context_builder: ContextBuilder,
        shareable_with_windowed_contexts: bool,
    ) -> Result<Self, CreationError>
    {
        let ContextBuilder { pf_reqs, gl_attr } = context_builder;
        let gl_attr = gl_attr.map_sharing(|_ctxt| panic!("Context sharing is not allowed when using `new()`. Please instead use `new_shared()`."));
        
        // Not all platforms support context sharing yet, when they do, their 
        // `new.*` functions should be marked unsafe.
        #[allow(unused_unsafe)] 
        unsafe {
            platform::Context::new_context(el, &pf_reqs, &gl_attr, shareable_with_windowed_contexts)
                .map(|context| Context { context })
        }
    }

    /// Builds the given GL context
    ///
    /// Contexts made with the `shareable_with_windowed_contexts` flag set to
    /// `true` can be shared with:
    ///  - contexts made with that flag set to `true`; and
    ///  - contexts made when creating a `GlWindow`.
    ///
    /// If the flag is set to `false` on the other hand, the context should only
    /// be shared with other contexts made with the flag set to `false`.
    ///
    /// Some platforms might not implement contexts which aren't shareable with
    /// windowed contexts. If so, those platforms will fallback to making a
    /// contexts which are shareable with windowed contexts.
    ///
    /// You are not guaranteed to receive an error if you share a context with an
    /// other context which you're not permitted to share it with, as according
    /// to:
    ///  - the restrictions stated by us above; and
    ///  - the restrictions imposed on you by the platform your application runs
    ///  on. (Please refer to `README-SHARING.md`)
    ///
    /// Failing to follow all the context sharing restrictions imposed on you
    /// may result in unsafe behavior.
    ///
    /// Error should be very rare and only occur in case of permission denied,
    /// incompatible system, out of memory, etc.
    pub unsafe fn new_shared(
        el: &winit::EventsLoop,
        context_builder: ContextBuilder,
        shareable_with_windowed_contexts: bool,
    ) -> Result<Self, CreationError>
    {
        let ContextBuilder { pf_reqs, gl_attr } = context_builder;
        let gl_attr = gl_attr.map_sharing(|ctxt| &ctxt.context);
        platform::Context::new_context(el, &pf_reqs, &gl_attr, shareable_with_windowed_contexts)
            .map(|context| Context { context })
    }
}

impl GlContext for GlWindow {
    unsafe fn make_current(&self) -> Result<(), ContextError> {
        self.context.make_current()
    }

    fn is_current(&self) -> bool {
        self.context.is_current()
    }

    fn get_proc_address(&self, addr: &str) -> *const () {
        self.context.get_proc_address(addr)
    }

    fn get_api(&self) -> Api {
        self.context.get_api()
    }
}

impl std::ops::Deref for GlWindow {
    type Target = Window;
    fn deref(&self) -> &Self::Target {
        &self.window
    }
}

/// Error that can happen while creating a window or a headless renderer.
#[derive(Debug)]
pub enum CreationError {
    OsError(String),
    /// TODO: remove this error
    NotSupported(&'static str),
    NoBackendAvailable(Box<std::error::Error + Send>),
    RobustnessNotSupported,
    OpenGlVersionNotSupported,
    NoAvailablePixelFormat,
    PlatformSpecific(String),
    Window(WindowCreationError),
    /// We received two errors, instead of one.
    CreationErrorPair(Box<CreationError>, Box<CreationError>),
}

impl CreationError {
    fn to_string(&self) -> &str {
        match *self {
            CreationError::OsError(ref text) => &text,
            CreationError::NotSupported(text) => &text,
            CreationError::NoBackendAvailable(_) => "No backend is available",
            CreationError::RobustnessNotSupported => "You requested robustness, but it is \
                                                      not supported.",
            CreationError::OpenGlVersionNotSupported => "The requested OpenGL version is not \
                                                         supported.",
            CreationError::NoAvailablePixelFormat => "Couldn't find any pixel format that matches \
                                                      the criteria.",
            CreationError::PlatformSpecific(ref text) => &text,
            CreationError::Window(ref err) => std::error::Error::description(err),
            CreationError::CreationErrorPair(ref _err1, ref _err2) => "Received two errors."
        }
    }
}

impl std::fmt::Display for CreationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        formatter.write_str(self.to_string())?;

        if let CreationError::CreationErrorPair(ref e1, ref e2) = *self {
            write!(formatter, " Error 1: \"")?;
            e1.fmt(formatter)?;
            write!(formatter, "\"")?;
            write!(formatter, " Error 2: \"")?;
            e2.fmt(formatter)?;
            write!(formatter, "\"")?;
        }

        if let &CreationError::NotSupported(msg) = self {
            write!(formatter, ": {}", msg)?;
        }
        if let Some(err) = std::error::Error::cause(self) {
            write!(formatter, ": {}", err)?;
        }
        Ok(())
    }
}

impl std::error::Error for CreationError {
    fn description(&self) -> &str {
        self.to_string()
    }

    fn cause(&self) -> Option<&std::error::Error> {
        match *self {
            CreationError::NoBackendAvailable(ref err) => Some(&**err),
            CreationError::Window(ref err) => Some(err),
            _ => None
        }
    }
}

impl From<WindowCreationError> for CreationError {
    fn from(err: WindowCreationError) -> Self {
        CreationError::Window(err)
    }
}

/// Error that can happen when manipulating an OpenGL context.
#[derive(Debug)]
pub enum ContextError {
    /// General platform error.
    OsError(String),
    IoError(io::Error),
    ContextLost,
}

impl ContextError {
    fn to_string(&self) -> &str {
        use std::error::Error;
        match *self {
            ContextError::OsError(ref string) => string,
            ContextError::IoError(ref err) => err.description(),
            ContextError::ContextLost => "Context lost"
        }
    }
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        formatter.write_str(self.to_string())
    }
}

impl std::error::Error for ContextError {
    fn description(&self) -> &str {
        self.to_string()
    }
}

/// All APIs related to OpenGL that you can possibly get while using glutin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    /// The classical OpenGL. Available on Windows, Linux, OS/X.
    OpenGl,
    /// OpenGL embedded system. Available on Linux, Android.
    OpenGlEs,
    /// OpenGL for the web. Very similar to OpenGL ES.
    WebGl,
}

/// Describes the requested OpenGL context profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlProfile {
    /// Include all the immediate more functions and definitions.
    Compatibility,
    /// Include all the future-compatible functions and definitions.
    Core,
}

/// Describes the OpenGL API and version that are being requested when a context is created.
#[derive(Debug, Copy, Clone)]
pub enum GlRequest {
    /// Request the latest version of the "best" API of this platform.
    ///
    /// On desktop, will try OpenGL.
    Latest,

    /// Request a specific version of a specific API.
    ///
    /// Example: `GlRequest::Specific(Api::OpenGl, (3, 3))`.
    Specific(Api, (u8, u8)),

    /// If OpenGL is available, create an OpenGL context with the specified `opengl_version`.
    /// Else if OpenGL ES or WebGL is available, create a context with the
    /// specified `opengles_version`.
    GlThenGles {
        /// The version to use for OpenGL.
        opengl_version: (u8, u8),
        /// The version to use for OpenGL ES.
        opengles_version: (u8, u8),
    },
}

impl GlRequest {
    /// Extract the desktop GL version, if any.
    pub fn to_gl_version(&self) -> Option<(u8, u8)> {
        match self {
            &GlRequest::Specific(Api::OpenGl, version) => Some(version),
            &GlRequest::GlThenGles { opengl_version: version, .. } => Some(version),
            _ => None,
        }
    }
}

/// The minimum core profile GL context. Useful for getting the minimum
/// required GL version while still running on OSX, which often forbids
/// the compatibility profile features.
pub static GL_CORE: GlRequest = GlRequest::Specific(Api::OpenGl, (3, 2));

/// Specifies the tolerance of the OpenGL context to faults. If you accept raw OpenGL commands
/// and/or raw shader code from an untrusted source, you should definitely care about this.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Robustness {
    /// Not everything is checked. Your application can crash if you do something wrong with your
    /// shaders.
    NotRobust,

    /// The driver doesn't check anything. This option is very dangerous. Please know what you're
    /// doing before using it. See the `GL_KHR_no_error` extension.
    ///
    /// Since this option is purely an optimization, no error will be returned if the backend
    /// doesn't support it. Instead it will automatically fall back to `NotRobust`.
    NoError,

    /// Everything is checked to avoid any crash. The driver will attempt to avoid any problem,
    /// but if a problem occurs the behavior is implementation-defined. You are just guaranteed not
    /// to get a crash.
    RobustNoResetNotification,

    /// Same as `RobustNoResetNotification` but the context creation doesn't fail if it's not
    /// supported.
    TryRobustNoResetNotification,

    /// Everything is checked to avoid any crash. If a problem occurs, the context will enter a
    /// "context lost" state. It must then be recreated. For the moment, glutin doesn't provide a
    /// way to recreate a context with the same window :-/
    RobustLoseContextOnReset,

    /// Same as `RobustLoseContextOnReset` but the context creation doesn't fail if it's not
    /// supported.
    TryRobustLoseContextOnReset,
}

/// The behavior of the driver when you change the current context.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ReleaseBehavior {
    /// Doesn't do anything. Most notably doesn't flush.
    None,

    /// Flushes the context that was previously current as if `glFlush` was called.
    Flush,
}

/// Describes a possible format. Unused.
#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub struct PixelFormat {
    pub hardware_accelerated: bool,
    pub color_bits: u8,
    pub alpha_bits: u8,
    pub depth_bits: u8,
    pub stencil_bits: u8,
    pub stereoscopy: bool,
    pub double_buffer: bool,
    pub multisampling: Option<u16>,
    pub srgb: bool,
}

/// Describes how the backend should choose a pixel format.
// TODO: swap method? (swap, copy)
#[derive(Clone, Debug)]
pub struct PixelFormatRequirements {
    /// If true, only hardware-accelerated formats will be considered. If false, only software
    /// renderers. `None` means "don't care". Default is `Some(true)`.
    pub hardware_accelerated: Option<bool>,

    /// Minimum number of bits for the color buffer, excluding alpha. `None` means "don't care".
    /// The default is `Some(24)`.
    pub color_bits: Option<u8>,

    /// If true, the color buffer must be in a floating point format. Default is `false`.
    ///
    /// Using floating points allows you to write values outside of the `[0.0, 1.0]` range.
    pub float_color_buffer: bool,

    /// Minimum number of bits for the alpha in the color buffer. `None` means "don't care".
    /// The default is `Some(8)`.
    pub alpha_bits: Option<u8>,

    /// Minimum number of bits for the depth buffer. `None` means "don't care".
    /// The default value is `Some(24)`.
    pub depth_bits: Option<u8>,

    /// Minimum number of bits for the depth buffer. `None` means "don't care".
    /// The default value is `Some(8)`.
    pub stencil_bits: Option<u8>,

    /// If true, only double-buffered formats will be considered. If false, only single-buffer
    /// formats. `None` means "don't care". The default is `Some(true)`.
    pub double_buffer: Option<bool>,

    /// Contains the minimum number of samples per pixel in the color, depth and stencil buffers.
    /// `None` means "don't care". Default is `None`.
    /// A value of `Some(0)` indicates that multisampling must not be enabled.
    pub multisampling: Option<u16>,

    /// If true, only stereoscopic formats will be considered. If false, only non-stereoscopic
    /// formats. The default is `false`.
    pub stereoscopy: bool,

    /// If true, only sRGB-capable formats will be considered. If false, don't care.
    /// The default is `false`.
    pub srgb: bool,

    /// The behavior when changing the current context. Default is `Flush`.
    pub release_behavior: ReleaseBehavior,
}

impl Default for PixelFormatRequirements {
    #[inline]
    fn default() -> PixelFormatRequirements {
        PixelFormatRequirements {
            hardware_accelerated: Some(true),
            color_bits: Some(24),
            float_color_buffer: false,
            alpha_bits: Some(8),
            depth_bits: Some(24),
            stencil_bits: Some(8),
            double_buffer: None,
            multisampling: None,
            stereoscopy: false,
            srgb: false,
            release_behavior: ReleaseBehavior::Flush,
        }
    }
}

/// Attributes to use when creating an OpenGL context.
#[derive(Clone)]
pub struct GlAttributes<S> {
    /// An existing context to share the new the context with.
    ///
    /// The default is `None`.
    pub sharing: Option<S>,

    /// Version to try create. See `GlRequest` for more infos.
    ///
    /// The default is `Latest`.
    pub version: GlRequest,

    /// OpenGL profile to use.
    ///
    /// The default is `None`.
    pub profile: Option<GlProfile>,

    /// Whether to enable the `debug` flag of the context.
    ///
    /// Debug contexts are usually slower but give better error reporting.
    ///
    /// The default is `true` in debug mode and `false` in release mode.
    pub debug: bool,

    /// How the OpenGL context should detect errors.
    ///
    /// The default is `NotRobust` because this is what is typically expected when you create an
    /// OpenGL context. However for safety you should consider `TryRobustLoseContextOnReset`.
    pub robustness: Robustness,

    /// Whether to use vsync. If vsync is enabled, calling `swap_buffers` will block until the
    /// screen refreshes. This is typically used to prevent screen tearing.
    ///
    /// The default is `false`.
    pub vsync: bool,
}

impl<S> GlAttributes<S> {
    /// Turns the `sharing` parameter into another type by calling a closure.
    #[inline]
    pub fn map_sharing<F, T>(self, f: F) -> GlAttributes<T> where F: FnOnce(S) -> T {
        GlAttributes {
            sharing: self.sharing.map(f),
            version: self.version,
            profile: self.profile,
            debug: self.debug,
            robustness: self.robustness,
            vsync: self.vsync,
        }
    }
}

impl<S> Default for GlAttributes<S> {
    #[inline]
    fn default() -> GlAttributes<S> {
        GlAttributes {
            sharing: None,
            version: GlRequest::Latest,
            profile: None,
            debug: cfg!(debug_assertions),
            robustness: Robustness::NotRobust,
            vsync: false,
        }
    }
}
