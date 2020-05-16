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
#[cfg(not(any(target_os = "macos", windows)))]
use std::ffi::c_void;
use std::fmt::{self, Display, Formatter};
#[cfg(not(any(target_os = "macos", windows)))]
use std::os::raw::c_ulong;

use glutin::dpi::{PhysicalPosition, PhysicalSize};
use glutin::event_loop::EventLoop;
#[cfg(target_os = "macos")]
use glutin::platform::macos::{RequestUserAttentionType, WindowBuilderExtMacOS, WindowExtMacOS};
#[cfg(not(any(target_os = "macos", windows)))]
use glutin::platform::unix::{EventLoopWindowTargetExtUnix, WindowBuilderExtUnix, WindowExtUnix};
#[cfg(not(target_os = "macos"))]
use glutin::window::Icon;
use glutin::window::{CursorIcon, Fullscreen, Window as GlutinWindow, WindowBuilder, WindowId};
use glutin::{self, ContextBuilder, PossiblyCurrent, WindowedContext};
#[cfg(not(target_os = "macos"))]
use image::ImageFormat;
#[cfg(not(any(target_os = "macos", windows)))]
use x11_dl::xlib::{Display as XDisplay, PropModeReplace, XErrorEvent, Xlib};

use alacritty_terminal::config::{Decorations, StartupMode, WindowConfig};
use alacritty_terminal::event::Event;
#[cfg(not(windows))]
use alacritty_terminal::term::{SizeInfo, Term};

use crate::config::Config;
use crate::gl;

// It's required to be in this directory due to the `windows.rc` file
#[cfg(not(target_os = "macos"))]
static WINDOW_ICON: &[u8] = include_bytes!("../../extra/windows/alacritty.ico");

/// Window errors
#[derive(Debug)]
pub enum Error {
    /// Error creating the window
    ContextCreation(glutin::CreationError),

    /// Error dealing with fonts
    Font(font::Error),

    /// Error manipulating the rendering context
    Context(glutin::ContextError),
}

/// Result of fallible operations concerning a Window.
type Result<T> = std::result::Result<T, Error>;

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::ContextCreation(err) => err.source(),
            Error::Context(err) => err.source(),
            Error::Font(err) => err.source(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::ContextCreation(err) => write!(f, "Error creating GL context; {}", err),
            Error::Context(err) => write!(f, "Error operating on render context; {}", err),
            Error::Font(err) => err.fmt(f),
        }
    }
}

impl From<glutin::CreationError> for Error {
    fn from(val: glutin::CreationError) -> Self {
        Error::ContextCreation(val)
    }
}

impl From<glutin::ContextError> for Error {
    fn from(val: glutin::ContextError) -> Self {
        Error::Context(val)
    }
}

impl From<font::Error> for Error {
    fn from(val: font::Error) -> Self {
        Error::Font(val)
    }
}

fn create_gl_window(
    mut window: WindowBuilder,
    event_loop: &EventLoop<Event>,
    srgb: bool,
    dimensions: Option<PhysicalSize<u32>>,
) -> Result<WindowedContext<PossiblyCurrent>> {
    if let Some(dimensions) = dimensions {
        window = window.with_inner_size(dimensions);
    }

    let windowed_context = ContextBuilder::new()
        .with_srgb(srgb)
        .with_vsync(true)
        .with_hardware_acceleration(None)
        .build_windowed(window, event_loop)?;

    // Make the context current so OpenGL operations can run
    let windowed_context = unsafe { windowed_context.make_current().map_err(|(_, err)| err)? };

    Ok(windowed_context)
}

/// A window which can be used for displaying the terminal
///
/// Wraps the underlying windowing library to provide a stable API in Alacritty
pub struct Window {
    windowed_context: WindowedContext<PossiblyCurrent>,
    current_mouse_cursor: CursorIcon,
    mouse_visible: bool,
}

impl Window {
    /// Create a new window
    ///
    /// This creates a window and fully initializes a window.
    pub fn new(
        event_loop: &EventLoop<Event>,
        config: &Config,
        size: Option<PhysicalSize<u32>>,
    ) -> Result<Window> {
        let window_builder = Window::get_platform_window(&config.window.title, &config.window);
        let windowed_context =
            create_gl_window(window_builder.clone(), &event_loop, false, size)
                .or_else(|_| create_gl_window(window_builder, &event_loop, true, size))?;

        // Text cursor
        let current_mouse_cursor = CursorIcon::Text;
        windowed_context.window().set_cursor_icon(current_mouse_cursor);

        // Set OpenGL symbol loader. This call MUST be after window.make_current on windows.
        gl::load_with(|symbol| windowed_context.get_proc_address(symbol) as *const _);

        // On X11, embed the window inside another if the parent ID has been set
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            if event_loop.is_x11() {
                if let Some(parent_window_id) = config.window.embed {
                    x_embed_window(windowed_context.window(), parent_window_id);
                }
            }
        }

        Ok(Self { current_mouse_cursor, mouse_visible: true, windowed_context })
    }

    pub fn set_inner_size(&mut self, size: PhysicalSize<u32>) {
        self.window().set_inner_size(size);
    }

    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.window().inner_size()
    }

    pub fn scale_factor(&self) -> f64 {
        self.window().scale_factor()
    }

    #[inline]
    pub fn set_visible(&self, visibility: bool) {
        self.window().set_visible(visibility);
    }

    /// Set the window title
    #[inline]
    pub fn set_title(&self, title: &str) {
        self.window().set_title(title);
    }

    #[inline]
    pub fn set_mouse_cursor(&mut self, cursor: CursorIcon) {
        if cursor != self.current_mouse_cursor {
            self.current_mouse_cursor = cursor;
            self.window().set_cursor_icon(cursor);
        }
    }

    /// Set mouse cursor visible
    pub fn set_mouse_visible(&mut self, visible: bool) {
        if visible != self.mouse_visible {
            self.mouse_visible = visible;
            self.window().set_cursor_visible(visible);
        }
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn get_platform_window(title: &str, window_config: &WindowConfig) -> WindowBuilder {
        let decorations = match window_config.decorations {
            Decorations::None => false,
            _ => true,
        };

        let image = image::load_from_memory_with_format(WINDOW_ICON, ImageFormat::ICO)
            .expect("loading icon")
            .to_rgba();
        let (width, height) = image.dimensions();
        let icon = Icon::from_rgba(image.into_raw(), width, height);

        let class = &window_config.class;

        let mut builder = WindowBuilder::new()
            .with_title(title)
            .with_visible(false)
            .with_transparent(true)
            .with_decorations(decorations)
            .with_maximized(window_config.startup_mode() == StartupMode::Maximized)
            .with_window_icon(icon.ok())
            // X11
            .with_class(class.instance.clone(), class.general.clone())
            // Wayland
            .with_app_id(class.instance.clone());

        if let Some(ref val) = window_config.gtk_theme_variant {
            builder = builder.with_gtk_theme_variant(val.clone())
        }

        builder
    }

    #[cfg(windows)]
    pub fn get_platform_window(title: &str, window_config: &WindowConfig) -> WindowBuilder {
        let decorations = match window_config.decorations {
            Decorations::None => false,
            _ => true,
        };

        let image = image::load_from_memory_with_format(WINDOW_ICON, ImageFormat::ICO)
            .expect("loading icon")
            .to_rgba();
        let (width, height) = image.dimensions();
        let icon = Icon::from_rgba(image.into_raw(), width, height);

        WindowBuilder::new()
            .with_title(title)
            .with_visible(true)
            .with_decorations(decorations)
            .with_transparent(true)
            .with_maximized(window_config.startup_mode() == StartupMode::Maximized)
            .with_window_icon(icon.ok())
    }

    #[cfg(target_os = "macos")]
    pub fn get_platform_window(title: &str, window_config: &WindowConfig) -> WindowBuilder {
        let window = WindowBuilder::new()
            .with_title(title)
            .with_visible(false)
            .with_transparent(true)
            .with_maximized(window_config.startup_mode() == StartupMode::Maximized);

        match window_config.decorations {
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

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn set_urgent(&self, is_urgent: bool) {
        self.window().set_urgent(is_urgent);
    }

    #[cfg(target_os = "macos")]
    pub fn set_urgent(&self, is_urgent: bool) {
        if !is_urgent {
            return;
        }

        self.window().request_user_attention(RequestUserAttentionType::Critical);
    }

    #[cfg(windows)]
    pub fn set_urgent(&self, _is_urgent: bool) {}

    pub fn set_outer_position(&self, pos: PhysicalPosition<u32>) {
        self.window().set_outer_position(pos);
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn x11_window_id(&self) -> Option<usize> {
        self.window().xlib_window().map(|xlib_window| xlib_window as usize)
    }

    pub fn window_id(&self) -> WindowId {
        self.window().id()
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn set_maximized(&self, maximized: bool) {
        self.window().set_maximized(maximized);
    }

    pub fn set_minimized(&self, minimized: bool) {
        self.window().set_minimized(minimized);
    }

    /// Toggle the window's fullscreen state
    pub fn toggle_fullscreen(&mut self) {
        self.set_fullscreen(self.window().fullscreen().is_none());
    }

    #[cfg(target_os = "macos")]
    pub fn toggle_simple_fullscreen(&mut self) {
        self.set_simple_fullscreen(!self.window().simple_fullscreen());
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        #[cfg(macos)]
        {
            if self.window().simple_fullscreen() {
                return;
            }
        }

        if fullscreen {
            let current_monitor = self.window().current_monitor();
            self.window().set_fullscreen(Some(Fullscreen::Borderless(current_monitor)));
        } else {
            self.window().set_fullscreen(None);
        }
    }

    #[cfg(target_os = "macos")]
    pub fn set_simple_fullscreen(&mut self, simple_fullscreen: bool) {
        if self.window().fullscreen().is_some() {
            return;
        }

        self.window().set_simple_fullscreen(simple_fullscreen);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn wayland_display(&self) -> Option<*mut c_void> {
        self.window().wayland_display()
    }

    /// Adjust the IME editor position according to the new location of the cursor
    #[cfg(not(windows))]
    pub fn update_ime_position<T>(&mut self, terminal: &Term<T>, size_info: &SizeInfo) {
        let point = terminal.cursor().point;
        let SizeInfo { cell_width, cell_height, padding_x, padding_y, .. } = size_info;

        let nspot_x = f64::from(padding_x + point.col.0 as f32 * cell_width);
        let nspot_y = f64::from(padding_y + (point.line.0 + 1) as f32 * cell_height);

        self.window().set_ime_position(PhysicalPosition::new(nspot_x, nspot_y));
    }

    pub fn swap_buffers(&self) {
        self.windowed_context.swap_buffers().expect("swap buffers");
    }

    pub fn resize(&self, size: PhysicalSize<u32>) {
        self.windowed_context.resize(size);
    }

    fn window(&self) -> &GlutinWindow {
        self.windowed_context.window()
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn x_embed_window(window: &GlutinWindow, parent_id: c_ulong) {
    let (xlib_display, xlib_window) = match (window.xlib_display(), window.xlib_window()) {
        (Some(display), Some(window)) => (display, window),
        _ => return,
    };

    let xlib = Xlib::open().expect("get xlib");

    unsafe {
        let atom = (xlib.XInternAtom)(xlib_display as *mut _, "_XEMBED".as_ptr() as *const _, 0);
        (xlib.XChangeProperty)(
            xlib_display as _,
            xlib_window as _,
            atom,
            atom,
            32,
            PropModeReplace,
            [0, 1].as_ptr(),
            2,
        );

        // Register new error handler
        let old_handler = (xlib.XSetErrorHandler)(Some(xembed_error_handler));

        // Check for the existence of the target before attempting reparenting
        (xlib.XReparentWindow)(xlib_display as _, xlib_window as _, parent_id, 0, 0);

        // Drain errors and restore original error handler
        (xlib.XSync)(xlib_display as _, 0);
        (xlib.XSetErrorHandler)(old_handler);
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
unsafe extern "C" fn xembed_error_handler(_: *mut XDisplay, _: *mut XErrorEvent) -> i32 {
    println!("Could not embed into specified window.");
    std::process::exit(1);
}
