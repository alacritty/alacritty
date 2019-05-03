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
use std::ffi::c_void;
use std::fmt::Display;

use crate::gl;
use glutin::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};
#[cfg(target_os = "macos")]
use glutin::os::macos::WindowExt;
#[cfg(not(any(target_os = "macos", windows)))]
use glutin::os::unix::{EventsLoopExt, WindowExt};
#[cfg(not(target_os = "macos"))]
use glutin::Icon;
use glutin::{
    self, ContextBuilder, ControlFlow, Event, EventsLoop, MouseCursor, PossiblyCurrent,
    WindowBuilder,
};
#[cfg(not(target_os = "macos"))]
use image::ImageFormat;

use crate::config::{Options, Decorations, StartupMode, WindowConfig};

// It's required to be in this directory due to the `windows.rc` file
#[cfg(not(target_os = "macos"))]
static WINDOW_ICON: &'static [u8] = include_bytes!("../../extra/windows/alacritty.ico");

/// Default Alacritty name, used for window title and class.
pub const DEFAULT_NAME: &str = "Alacritty";

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
    windowed_context: glutin::WindowedContext<PossiblyCurrent>,
    mouse_visible: bool,

    /// Whether or not the window is the focused window.
    pub is_focused: bool,
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
    pub scale_factor: f64,
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&dyn (::std::error::Error)> {
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
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
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
    mut window: WindowBuilder,
    event_loop: &EventsLoop,
    srgb: bool,
    dimensions: Option<LogicalSize>,
) -> Result<glutin::WindowedContext<PossiblyCurrent>> {
    if let Some(dimensions) = dimensions {
        window = window.with_dimensions(dimensions);
    }

    let windowed_context = ContextBuilder::new()
        .with_srgb(srgb)
        .with_vsync(true)
        .with_hardware_acceleration(None)
        .build_windowed(window, event_loop)?;

    // Make the context current so OpenGL operations can run
    let windowed_context = unsafe { windowed_context.make_current().map_err(|(_, e)| e)? };

    Ok(windowed_context)
}

impl Window {
    /// Create a new window
    ///
    /// This creates a window and fully initializes a window.
    pub fn new(
        event_loop: EventsLoop,
        options: &Options,
        window_config: &WindowConfig,
        dimensions: Option<LogicalSize>,
    ) -> Result<Window> {
        let title = options.title.as_ref().map_or(DEFAULT_NAME, |t| t);
        let class = options.class.as_ref().map_or(DEFAULT_NAME, |c| c);
        let window_builder = Window::get_platform_window(title, class, window_config);
        let windowed_context =
            create_gl_window(window_builder.clone(), &event_loop, false, dimensions)
                .or_else(|_| create_gl_window(window_builder, &event_loop, true, dimensions))?;
        let window = windowed_context.window();
        window.show();

        // Maximize window after mapping in X11
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            if event_loop.is_x11() && window_config.startup_mode() == StartupMode::Maximized {
                window.set_maximized(true);
            }
        }

        // Set window position
        //
        // TODO: replace `set_position` with `with_position` once available
        // Upstream issue: https://github.com/tomaka/winit/issues/806
        let position = options.position().or_else(|| window_config.position());
        if let Some(position) = position {
            let physical = PhysicalPosition::from((position.x, position.y));
            let logical = physical.to_logical(window.get_hidpi_factor());
            window.set_position(logical);
        }

        if let StartupMode::Fullscreen = window_config.startup_mode() {
            let current_monitor = window.get_current_monitor();
            window.set_fullscreen(Some(current_monitor));
        }

        #[cfg(target_os = "macos")]
        {
            if let StartupMode::SimpleFullscreen = window_config.startup_mode() {
                use glutin::os::macos::WindowExt;
                window.set_simple_fullscreen(true);
            }
        }

        // Text cursor
        window.set_cursor(MouseCursor::Text);

        // Set OpenGL symbol loader. This call MUST be after window.make_current on windows.
        gl::load_with(|symbol| windowed_context.get_proc_address(symbol) as *const _);

        let window =
            Window { event_loop, windowed_context, mouse_visible: true, is_focused: false };

        window.run_os_extensions();

        Ok(window)
    }

    /// Get some properties about the device
    ///
    /// Some window properties are provided since subsystems like font
    /// rasterization depend on DPI and scale factor.
    pub fn device_properties(&self) -> DeviceProperties {
        DeviceProperties { scale_factor: self.window().get_hidpi_factor() }
    }

    pub fn inner_size_pixels(&self) -> Option<LogicalSize> {
        self.window().get_inner_size()
    }

    pub fn set_inner_size(&mut self, size: LogicalSize) {
        self.window().set_inner_size(size);
    }

    #[inline]
    pub fn hidpi_factor(&self) -> f64 {
        self.window().get_hidpi_factor()
    }

    #[inline]
    pub fn create_window_proxy(&self) -> Proxy {
        Proxy { inner: self.event_loop.create_proxy() }
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<()> {
        self.windowed_context.swap_buffers().map_err(From::from)
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
    pub fn resize(&self, size: PhysicalSize) {
        self.windowed_context.resize(size);
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
    pub fn set_title(&self, title: &str) {
        self.window().set_title(title);
    }

    #[inline]
    pub fn set_mouse_cursor(&self, cursor: MouseCursor) {
        self.window().set_cursor(cursor);
    }

    /// Set mouse cursor visible
    pub fn set_mouse_visible(&mut self, visible: bool) {
        if visible != self.mouse_visible {
            self.mouse_visible = visible;
            self.window().hide_cursor(!visible);
        }
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn get_platform_window(
        title: &str,
        class: &str,
        window_config: &WindowConfig,
    ) -> WindowBuilder {
        use glutin::os::unix::WindowBuilderExt;

        let decorations = match window_config.decorations() {
            Decorations::None => false,
            _ => true,
        };

        let icon = Icon::from_bytes_with_format(WINDOW_ICON, ImageFormat::ICO);

        WindowBuilder::new()
            .with_title(title)
            .with_visibility(false)
            .with_transparency(true)
            .with_decorations(decorations)
            .with_maximized(window_config.startup_mode() == StartupMode::Maximized)
            .with_window_icon(icon.ok())
            // X11
            .with_class(class.into(), DEFAULT_NAME.into())
            // Wayland
            .with_app_id(class.into())
    }

    #[cfg(windows)]
    pub fn get_platform_window(
        title: &str,
        _class: &str,
        window_config: &WindowConfig,
    ) -> WindowBuilder {
        let decorations = match window_config.decorations() {
            Decorations::None => false,
            _ => true,
        };

        let icon = Icon::from_bytes_with_format(WINDOW_ICON, ImageFormat::ICO);

        WindowBuilder::new()
            .with_title(title)
            .with_visibility(cfg!(windows))
            .with_decorations(decorations)
            .with_transparency(true)
            .with_maximized(window_config.startup_mode() == StartupMode::Maximized)
            .with_window_icon(icon.ok())
    }

    #[cfg(target_os = "macos")]
    pub fn get_platform_window(
        title: &str,
        _class: &str,
        window_config: &WindowConfig,
    ) -> WindowBuilder {
        use glutin::os::macos::WindowBuilderExt;

        let window = WindowBuilder::new()
            .with_title(title)
            .with_visibility(false)
            .with_transparency(true)
            .with_maximized(window_config.startup_mode() == StartupMode::Maximized);

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
            Decorations::None => window.with_titlebar_hidden(true),
        }
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd"
    ))]
    pub fn set_urgent(&self, is_urgent: bool) {
        self.window().set_urgent(is_urgent);
    }

    #[cfg(target_os = "macos")]
    pub fn set_urgent(&self, is_urgent: bool) {
        self.window().request_user_attention(is_urgent);
    }

    #[cfg(windows)]
    pub fn set_urgent(&self, _is_urgent: bool) {}

    pub fn set_ime_spot(&self, pos: LogicalPosition) {
        self.window().set_ime_spot(pos);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn get_window_id(&self) -> Option<usize> {
        match self.window().get_xlib_window() {
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
        self.window().hide();
    }

    /// Fullscreens the window on the current monitor.
    pub fn set_fullscreen(&self, fullscreen: bool) {
        let glutin_window = self.window();
        if fullscreen {
            let current_monitor = glutin_window.get_current_monitor();
            glutin_window.set_fullscreen(Some(current_monitor));
        } else {
            glutin_window.set_fullscreen(None);
        }
    }

    #[cfg(target_os = "macos")]
    pub fn set_simple_fullscreen(&self, fullscreen: bool) {
        use glutin::os::macos::WindowExt;
        self.window().set_simple_fullscreen(fullscreen);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn get_wayland_display(&self) -> Option<*mut c_void> {
        self.window().get_wayland_display()
    }

    fn window(&self) -> &glutin::Window {
        self.windowed_context.window()
    }
}

pub trait OsExtensions {
    fn run_os_extensions(&self) {}
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd"
)))]
impl OsExtensions for Window {}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd"
))]
impl OsExtensions for Window {
    fn run_os_extensions(&self) {
        use libc::getpid;
        use std::ffi::CStr;
        use std::ptr;
        use x11_dl::xlib::{self, PropModeReplace, XA_CARDINAL};

        let xlib_display = self.window().get_xlib_display();
        let xlib_window = self.window().get_xlib_window();

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
