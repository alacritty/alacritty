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
use glutin;

/// Default title for the window
const DEFAULT_TITLE: &'static str = "Alacritty";

/// Resize handling for Mac and maybe other platforms
///
/// This delegates to a statically referenced closure for convenience. The
/// C-style callback doesn't receive a pointer or anything, so we are forced to
/// use static storage.
///
/// This will fail horribly if more than one window is created. Don't do that :)
fn window_resize_handler(width: u32, height: u32) {
    unsafe {
        RESIZE_CALLBACK.as_ref().map(|func| func(width, height));
    }
}

/// The resize callback invoked by `window_resize_handler`
static mut RESIZE_CALLBACK: Option<Box<Fn(u32, u32)>> = None;

/// Window errors
#[derive(Debug)]
pub enum Error {
    /// Error creating the window
    Creation(glutin::CreationError),

    /// Error manipulating the rendering context
    Context(glutin::ContextError),
}

/// Result of fallible operations concerning a Window.
type Result<T> = ::std::result::Result<T, Error>;

/// A window which can be used for displaying the terminal
///
/// Wraps the underlying windowing library to provide a stable API in Alacritty
pub struct Window {
    glutin_window: glutin::Window,
}

/// Threadsafe APIs for the window
pub struct Proxy(glutin::WindowProxy);

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
            height: Points(height_pts)
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

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::Creation(ref err) => Some(err),
            Error::Context(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Creation(ref _err) => "Error creating glutin Window",
            Error::Context(ref _err) => "Error operating on render context",
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::Creation(ref err) => {
                write!(f, "Error creating glutin::Window; {}", err)
            },
            Error::Context(ref err) => {
                write!(f, "Error operating on render context; {}", err)
            },
        }
    }
}

impl From<glutin::CreationError> for Error {
    fn from(val: glutin::CreationError) -> Error {
        Error::Creation(val)
    }
}

impl From<glutin::ContextError> for Error {
    fn from(val: glutin::ContextError) -> Error {
        Error::Context(val)
    }
}

impl Window {
    /// Create a new window
    ///
    /// This creates a window and fully initializes a window.
    pub fn new() -> Result<Window> {
        /// Create a glutin::Window
        let mut window = glutin::WindowBuilder::new()
            .with_vsync()
            .with_title(DEFAULT_TITLE)
            .build()?;

        /// Set the glutin window resize callback for *this* window. The
        /// function pointer must be a C-style callback. This sets such a
        /// callback which simply delegates to a statically referenced Rust
        /// closure.
        window.set_window_resize_callback(Some(window_resize_handler as fn(u32, u32)));

        /// Set OpenGL symbol loader
        gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);

        /// Make the window's context current so OpenGL operations can run
        unsafe {
            window.make_current()?;
        }

        Ok(Window {
            glutin_window: window,
        })
    }

    /// Get some properties about the device
    ///
    /// Some window properties are provided since subsystems like font
    /// rasterization depend on DPI and scale factor.
    pub fn device_properties(&self) -> DeviceProperties {
        DeviceProperties {
            scale_factor: self.glutin_window.hidpi_factor(),
        }
    }

    /// Set the window resize callback
    ///
    /// Pass a `move` closure which will be called with the new width and height
    /// when the window is resized. According to the glutin docs, this can be
    /// used to draw during resizing.
    ///
    /// This method takes self mutably to ensure there's no race condition
    /// setting the callback.
    pub fn set_resize_callback<F: Fn(u32, u32) + 'static>(&mut self, func: F) {
        unsafe {
            RESIZE_CALLBACK = Some(Box::new(func));
        }
    }

    pub fn inner_size_pixels(&self) -> Option<Size<Pixels<u32>>> {
        self.glutin_window
            .get_inner_size_pixels()
            .map(|(w, h)| Size { width: Pixels(w), height: Pixels(h) })
    }

    #[inline]
    pub fn hidpi_factor(&self) -> f32 {
        self.glutin_window.hidpi_factor()
    }

    #[inline]
    pub fn create_window_proxy(&self) -> Proxy {
        Proxy(self.glutin_window.create_window_proxy())
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<()> {
        self.glutin_window
            .swap_buffers()
            .map_err(From::from)
    }

    /// Block waiting for events
    ///
    /// FIXME should return our own type
    #[inline]
    pub fn wait_events(&self) -> glutin::WaitEventsIterator {
        self.glutin_window.wait_events()
    }

    /// Block waiting for events
    ///
    /// FIXME should return our own type
    #[inline]
    pub fn poll_events(&self) -> glutin::PollEventsIterator {
        self.glutin_window.poll_events()
    }
}

impl Proxy {
    /// Wakes up the event loop of the window
    ///
    /// This is useful for triggering a draw when the renderer would otherwise
    /// be waiting on user input.
    pub fn wakeup_event_loop(&self) {
        self.0.wakeup_event_loop();
    }
}

pub trait SetInnerSize<T> {
    fn set_inner_size<S: ToPoints>(&mut self, size: S);
}

impl SetInnerSize<Pixels<u32>> for Window {
    fn set_inner_size<T: ToPoints>(&mut self, size: T) {
        let size = size.to_points(self.hidpi_factor());
        self.glutin_window.set_inner_size(*size.width as _, *size.height as _);
    }
}
