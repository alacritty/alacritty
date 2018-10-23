// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use std::convert::From;
use std::fmt::{self, Display};
use std::ops::Deref;

use gl;
use glutin::GlContext;
#[cfg(windows)]
use winit::Icon;
#[cfg(windows)]
use image::ImageFormat;
use glutin::{
    self, ContextBuilder, ControlFlow, CursorState, Event, EventsLoop,
    MouseCursor as GlutinMouseCursor, WindowBuilder,
};

use MouseCursor;

use cli::Options;
use config::{Decorations, WindowConfig};

#[cfg(windows)]
static WINDOW_ICON: &'static [u8] = include_bytes!("../assets/windows/alacritty.ico");

/// Default text for the window's title bar, if not overriden.
///
/// In X11, this the default value for the `WM_NAME` property.
pub const DEFAULT_TITLE: &str = "Alacritty";

/// Default text for general window class, X11 specific.
///
/// In X11, this is the default value for the `WM_CLASS` property. The
/// second value of `WM_CLASS` is **never** changed to anything but
/// the default value.
///
/// ```ignore
/// $ xprop | grep WM_CLASS
/// WM_CLASS(STRING) = "Alacritty", "Alacritty"
/// ```
pub const DEFAULT_CLASS: &str = "Alacritty";

/// Window errors
#[derive(Debug)]
pub enum Error {
    /// Error creating the window
    ContextCreation(glutin::CreationError),

    /// Error manipulating the rendering context
    Context(glutin::ContextError),
}

/// Result of fallible operations concerning a Window.
type Result<T> = ::std::result::Result<T, Error>;

/// A window which can be used for displaying the terminal
///
/// Wraps the underlying windowing library to provide a stable API in Alacritty
pub struct Window {
    event_loop: EventsLoop,
    window: glutin::GlWindow,
    cursor_visible: bool,

    /// Whether or not the window is the focused window.
    pub is_focused: bool,
    pub is_fullscreen: bool,
}

/// Threadsafe APIs for the window
pub struct Proxy {
    inner: glutin::EventsLoopProxy,
}

/// Information about where the window is being displayed
///
/// Useful for subsystems like the font rasterized which depend on DPI and scale
/// factor.
pub struct DeviceProperties {
    /// Scale factor for pixels <-> points.
    ///
    /// This will be 1. on standard displays and may have a different value on
    /// hidpi displays.
    pub scale_factor: f32,
}

/// Size of the window
#[derive(Debug, Copy, Clone)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}

/// Strongly typed Pixels unit
#[derive(Debug, Copy, Clone)]
pub struct Pixels<T>(pub T);

/// Strongly typed Points unit
///
/// Points are like pixels but adjusted for DPI.
#[derive(Debug, Copy, Clone)]
pub struct Points<T>(pub T);

pub trait ToPoints {
    fn to_points(&self, scale: f32) -> Size<Points<u32>>;
}

impl ToPoints for Size<Points<u32>> {
    #[inline]
    fn to_points(&self, _scale: f32) -> Size<Points<u32>> {
        *self
    }
}

impl ToPoints for Size<Pixels<u32>> {
    fn to_points(&self, scale: f32) -> Size<Points<u32>> {
        let width_pts = (*self.width as f32 / scale) as u32;
        let height_pts = (*self.height as f32 / scale) as u32;

        Size {
            width: Points(width_pts),
            height: Points(height_pts),
        }
    }
}

impl<T: Display> Display for Size<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} × {}", self.width, self.height)
    }
}

macro_rules! deref_newtype {
    ($($src:ty),+) => {
        $(
        impl<T> Deref for $src {
            type Target = T;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
        )+
    }
}

deref_newtype! { Points<T>, Pixels<T> }

impl<T: Display> Display for Pixels<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}px", self.0)
    }
}

impl<T: Display> Display for Points<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}pts", self.0)
    }
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::ContextCreation(ref err) => Some(err),
            Error::Context(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::ContextCreation(ref _err) => "Error creating gl context",
            Error::Context(ref _err) => "Error operating on render context",
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::ContextCreation(ref err) => write!(f, "Error creating GL context; {}", err),
            Error::Context(ref err) => write!(f, "Error operating on render context; {}", err),
        }
    }
}

impl From<glutin::CreationError> for Error {
    fn from(val: glutin::CreationError) -> Error {
        Error::ContextCreation(val)
    }
}

impl From<glutin::ContextError> for Error {
    fn from(val: glutin::ContextError) -> Error {
        Error::Context(val)
    }
}

fn create_gl_window(
    window: WindowBuilder,
    event_loop: &EventsLoop,
    srgb: bool,
) -> ::std::result::Result<glutin::GlWindow, glutin::CreationError> {
    let context = ContextBuilder::new().with_srgb(srgb).with_vsync(true);
    ::glutin::GlWindow::new(window, context, event_loop)
}

impl Window {
    /// Create a new window
    ///
    /// This creates a window and fully initializes a window.
    pub fn new(options: &Options, window_config: &WindowConfig) -> Result<Window> {
        let event_loop = EventsLoop::new();
        let fullscreen_monitor = if window_config.fullscreen() {
            Some(event_loop.get_primary_monitor())
        } else {
            None
        };
        let title = options.title.as_ref().map_or(DEFAULT_TITLE, |t| t);
        let class = options.class.as_ref().map_or(DEFAULT_TITLE, |c| c);
        let window_builder = Window::get_platform_window(title, window_config, fullscreen_monitor);
        let window_builder = Window::platform_builder_ext(window_builder, &class);
        let window = create_gl_window(window_builder.clone(), &event_loop, false)
            .or_else(|_| create_gl_window(window_builder, &event_loop, true))?;
        window.show();

        // Text cursor
        window.set_cursor(GlutinMouseCursor::Text);

        // Make the context current so OpenGL operations can run
        unsafe {
            window.make_current()?;
        }

        // Set OpenGL symbol loader. This call MUST be after window.make_current on windows.
        gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);

        let window = Window {
            event_loop,
            window,
            cursor_visible: true,
            is_focused: false,
            is_fullscreen: false,
        };

        window.run_os_extensions();

        Ok(window)
    }

    /// Get some properties about the device
    ///
    /// Some window properties are provided since subsystems like font
    /// rasterization depend on DPI and scale factor.
    pub fn device_properties(&self) -> DeviceProperties {
        DeviceProperties {
            scale_factor: self.window.hidpi_factor(),
        }
    }

    pub fn inner_size_pixels(&self) -> Option<Size<Pixels<u32>>> {
        self.window.get_inner_size().map(|(w, h)| Size {
            width: Pixels(w),
            height: Pixels(h),
        })
    }

    #[inline]
    pub fn hidpi_factor(&self) -> f32 {
        self.window.hidpi_factor()
    }

    #[inline]
    pub fn create_window_proxy(&self) -> Proxy {
        Proxy {
            inner: self.event_loop.create_proxy(),
        }
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<()> {
        self.window.swap_buffers().map_err(From::from)
    }

    /// Poll for any available events
    #[inline]
    pub fn poll_events<F>(&mut self, func: F)
    where
        F: FnMut(Event),
    {
        self.event_loop.poll_events(func);
    }

    #[inline]
    pub fn resize(&self, width: u32, height: u32) {
        self.window.resize(width, height);
    }

    /// Block waiting for events
    #[inline]
    pub fn wait_events<F>(&mut self, func: F)
    where
        F: FnMut(Event) -> ControlFlow,
    {
        self.event_loop.run_forever(func);
    }

    /// Set the window title
    #[inline]
    pub fn set_title(&self, _title: &str) {
        // Because winpty doesn't know anything about OSC escapes this gets set to an empty
        // string on windows
        #[cfg(not(windows))]
        self.window.set_title(_title);
    }

    #[inline]
    pub fn set_mouse_cursor(&self, cursor: MouseCursor) {
        self.window.set_cursor(match cursor {
            MouseCursor::Arrow => GlutinMouseCursor::Arrow,
            MouseCursor::Text => GlutinMouseCursor::Text,
        });
    }

    /// Set cursor visible
    pub fn set_cursor_visible(&mut self, visible: bool) {
        if visible != self.cursor_visible {
            self.cursor_visible = visible;
            if let Err(err) = self.window.set_cursor_state(if visible {
                CursorState::Normal
            } else {
                CursorState::Hide
            }) {
                warn!("Failed to set cursor visibility: {}", err);
            }
        }
    }

    #[cfg(
        any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "openbsd"
        )
    )]
    fn platform_builder_ext(window_builder: WindowBuilder, wm_class: &str) -> WindowBuilder {
        use glutin::os::unix::WindowBuilderExt;
        window_builder.with_class(wm_class.to_owned(), "Alacritty".to_owned())
    }

    #[cfg(
        not(
            any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "dragonfly",
                target_os = "openbsd"
            )
        )
    )]
    fn platform_builder_ext(window_builder: WindowBuilder, _: &str) -> WindowBuilder {
        window_builder
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn get_platform_window(title: &str, window_config: &WindowConfig,
                               fullscreen_monitor: Option<glutin::MonitorId>)
                               -> WindowBuilder {
        let decorations = match window_config.decorations() {
            Decorations::None => false,
            _ => true,
        };

        WindowBuilder::new()
            .with_title(title)
            .with_visibility(false)
            .with_transparency(true)
            .with_fullscreen(fullscreen_monitor)
            .with_decorations(decorations)
    }


    #[cfg(windows)]
    pub fn get_platform_window(title: &str, window_config: &WindowConfig,
                               fullscreen_monitor: Option<glutin::MonitorId>)
                               -> WindowBuilder {
        let icon = Icon::from_bytes_with_format(WINDOW_ICON, ImageFormat::ICO).unwrap();

        let decorations = match window_config.decorations() {
            Decorations::None => false,
            _ => true,
        };

        WindowBuilder::new()
            .with_title(title)
            .with_visibility(cfg!(windows))
            .with_decorations(decorations)
            .with_transparency(true)
            .with_fullscreen(fullscreen_monitor)
            .with_window_icon(Some(icon))
    }

    #[cfg(target_os = "macos")]
    pub fn get_platform_window(title: &str, window_config: &WindowConfig,
                               fullscreen_monitor: Option<glutin::MonitorId>)
                               -> WindowBuilder {
        use glutin::os::macos::WindowBuilderExt;

        let window = WindowBuilder::new()
            .with_title(title)
            .with_visibility(false)
            .with_transparency(true)
            .with_fullscreen(fullscreen_monitor);

        match window_config.decorations() {
            Decorations::Full => window,
            Decorations::Transparent => window
                .with_title_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true),
            Decorations::Buttonless => window
                .with_title_hidden(true)
                .with_titlebar_buttons_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true),
            Decorations::None => window
                .with_titlebar_hidden(true),
        }
    }

    #[cfg(
        any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "openbsd"
        )
    )]
    pub fn set_urgent(&self, is_urgent: bool) {
        use glutin::os::unix::WindowExt;
        self.window.set_urgent(is_urgent);
    }

    #[cfg(
        not(
            any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "dragonfly",
                target_os = "openbsd"
            )
        )
    )]
    pub fn set_urgent(&self, _is_urgent: bool) {}

    pub fn set_ime_spot(&self, _x: i32, _y: i32) {
        // This is not implemented on windows as of winit 0.15.1
        #[cfg(not(windows))]
        self.window.set_ime_spot(_x, _y);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn get_window_id(&self) -> Option<usize> {
        use glutin::os::unix::WindowExt;

        match self.window.get_xlib_window() {
            Some(xlib_window) => Some(xlib_window as usize),
            None => None,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn get_window_id(&self) -> Option<usize> {
        None
    }

    /// Hide the window
    pub fn hide(&self) {
        self.window.hide();
    }

    pub fn toggle_fullscreen(&self) {
        let monitor_id = if self.is_fullscreen {
            None
        } else {
            Some(self.event_loop.get_primary_monitor())
        };

        self.window.set_fullscreen(monitor_id);
    }
}

pub trait OsExtensions {
    fn run_os_extensions(&self) {}
}

#[cfg(
    not(
        any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "openbsd"
        )
    )
)]
impl OsExtensions for Window {}

#[cfg(
    any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly", target_os = "openbsd")
)]
impl OsExtensions for Window {
    fn run_os_extensions(&self) {
        use glutin::os::unix::WindowExt;
        use libc::getpid;
        use std::ffi::CStr;
        use std::ptr;
        use x11_dl::xlib::{self, PropModeReplace, XA_CARDINAL};

        let xlib_display = self.window.get_xlib_display();
        let xlib_window = self.window.get_xlib_window();

        if let (Some(xlib_window), Some(xlib_display)) = (xlib_window, xlib_display) {
            let xlib = xlib::Xlib::open().expect("get xlib");

            // Set _NET_WM_PID to process pid
            unsafe {
                let _net_wm_pid = CStr::from_ptr(b"_NET_WM_PID\0".as_ptr() as *const _);
                let atom = (xlib.XInternAtom)(xlib_display as *mut _, _net_wm_pid.as_ptr(), 0);
                let pid = getpid();

                (xlib.XChangeProperty)(
                    xlib_display as _,
                    xlib_window as _,
                    atom,
                    XA_CARDINAL,
                    32,
                    PropModeReplace,
                    &pid as *const i32 as *const u8,
                    1,
                );
            }
            // Although this call doesn't actually pass any data, it does cause
            // WM_CLIENT_MACHINE to be set. WM_CLIENT_MACHINE MUST be set if _NET_WM_PID is set
            // (which we do above).
            unsafe {
                (xlib.XSetWMProperties)(
                    xlib_display as _,
                    xlib_window as _,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                );
            }
        }
    }
}

impl Proxy {
    /// Wakes up the event loop of the window
    ///
    /// This is useful for triggering a draw when the renderer would otherwise
    /// be waiting on user input.
    pub fn wakeup_event_loop(&self) {
        self.inner.wakeup().unwrap();
    }
}

pub trait SetInnerSize<T> {
    fn set_inner_size<S: ToPoints>(&mut self, size: &S);
}

impl SetInnerSize<Pixels<u32>> for Window {
    fn set_inner_size<T: ToPoints>(&mut self, size: &T) {
        let size = size.to_points(self.hidpi_factor());
        self.window
            .set_inner_size(*size.width as _, *size.height as _);
    }
}
