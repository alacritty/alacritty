#[cfg(not(any(target_os = "macos", windows)))]
use winit::platform::startup_notify::{
    self, EventLoopExtStartupNotify, WindowBuilderExtStartupNotify,
};

#[cfg(all(not(feature = "x11"), not(any(target_os = "macos", windows))))]
use winit::platform::wayland::WindowBuilderExtWayland;

#[rustfmt::skip]
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use {
    std::io::Cursor,
    winit::platform::x11::{WindowBuilderExtX11, EventLoopWindowTargetExtX11},
    winit::window::raw_window_handle::{HasRawDisplayHandle, RawDisplayHandle},
    glutin::platform::x11::X11VisualInfo,
    x11_dl::xlib::{Display as XDisplay, PropModeReplace, XErrorEvent, Xlib},
    winit::window::Icon,
    png::Decoder,
};

use std::fmt::{self, Display, Formatter};

#[cfg(target_os = "macos")]
use {
    cocoa::appkit::NSColorSpace,
    cocoa::base::{id, nil, NO, YES},
    objc::{msg_send, sel, sel_impl},
    winit::platform::macos::{OptionAsAlt, WindowBuilderExtMacOS, WindowExtMacOS},
};

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::EventLoopWindowTarget;
use winit::monitor::MonitorHandle;
#[cfg(windows)]
use winit::platform::windows::IconExtWindows;
use winit::window::raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::window::{
    CursorIcon, Fullscreen, ImePurpose, UserAttentionType, Window as WinitWindow, WindowBuilder,
    WindowId,
};

use alacritty_terminal::index::Point;

use crate::config::window::{Decorations, Identity, WindowConfig};
use crate::config::UiConfig;
use crate::display::SizeInfo;

/// Window icon for `_NET_WM_ICON` property.
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
static WINDOW_ICON: &[u8] = include_bytes!("../../extra/logo/compat/alacritty-term.png");

/// This should match the definition of IDI_ICON from `alacritty.rc`.
#[cfg(windows)]
const IDI_ICON: u16 = 0x101;

/// Window errors.
#[derive(Debug)]
pub enum Error {
    /// Error creating the window.
    WindowCreation(winit::error::OsError),

    /// Error dealing with fonts.
    Font(crossfont::Error),
}

/// Result of fallible operations concerning a Window.
type Result<T> = std::result::Result<T, Error>;

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::WindowCreation(err) => err.source(),
            Error::Font(err) => err.source(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::WindowCreation(err) => write!(f, "Error creating GL context; {}", err),
            Error::Font(err) => err.fmt(f),
        }
    }
}

impl From<winit::error::OsError> for Error {
    fn from(val: winit::error::OsError) -> Self {
        Error::WindowCreation(val)
    }
}

impl From<crossfont::Error> for Error {
    fn from(val: crossfont::Error) -> Self {
        Error::Font(val)
    }
}

/// A window which can be used for displaying the terminal.
///
/// Wraps the underlying windowing library to provide a stable API in Alacritty.
pub struct Window {
    /// Flag tracking that we have a frame we can draw.
    pub has_frame: bool,

    /// Cached scale factor for quickly scaling pixel sizes.
    pub scale_factor: f64,

    /// Flag indicating whether redraw was requested.
    pub requested_redraw: bool,

    window: WinitWindow,

    /// Current window title.
    title: String,

    current_mouse_cursor: CursorIcon,
    mouse_visible: bool,
}

impl Window {
    /// Create a new window.
    ///
    /// This creates a window and fully initializes a window.
    pub fn new<E>(
        event_loop: &EventLoopWindowTarget<E>,
        config: &UiConfig,
        identity: &Identity,
        #[rustfmt::skip]
        #[cfg(target_os = "macos")]
        tabbing_id: &Option<String>,
        #[rustfmt::skip]
        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        x11_visual: Option<X11VisualInfo>,
    ) -> Result<Window> {
        let identity = identity.clone();
        let mut window_builder = Window::get_platform_window(
            &identity,
            &config.window,
            #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
            x11_visual,
            #[cfg(target_os = "macos")]
            tabbing_id,
        );

        if let Some(position) = config.window.position {
            window_builder = window_builder
                .with_position(PhysicalPosition::<i32>::from((position.x, position.y)));
        }

        #[cfg(not(any(target_os = "macos", windows)))]
        if let Some(token) = event_loop.read_token_from_env() {
            log::debug!("Activating window with token: {token:?}");
            window_builder = window_builder.with_activation_token(token);

            // Remove the token from the env.
            unsafe {
                startup_notify::reset_activation_token_env();
            }
        }

        let window = window_builder
            .with_title(&identity.title)
            .with_theme(config.window.decorations_theme_variant)
            .with_visible(false)
            .with_transparent(true)
            .with_maximized(config.window.maximized())
            .with_fullscreen(config.window.fullscreen())
            .build(event_loop)?;

        // Text cursor.
        let current_mouse_cursor = CursorIcon::Text;
        window.set_cursor_icon(current_mouse_cursor);

        // Enable IME.
        window.set_ime_allowed(true);
        window.set_ime_purpose(ImePurpose::Terminal);

        // Set initial transparency hint.
        window.set_transparent(config.window_opacity() < 1.);

        #[cfg(target_os = "macos")]
        use_srgb_color_space(&window);

        // On X11, embed the window inside another if the parent ID has been set.
        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        if let Some(parent_window_id) = event_loop.is_x11().then_some(config.window.embed).flatten()
        {
            x_embed_window(&window, parent_window_id);
        }

        let scale_factor = window.scale_factor();
        log::info!("Window scale factor: {}", scale_factor);

        Ok(Self {
            current_mouse_cursor,
            mouse_visible: true,
            requested_redraw: false,
            window,
            title: identity.title,
            has_frame: true,
            scale_factor,
        })
    }

    #[inline]
    pub fn raw_window_handle(&self) -> RawWindowHandle {
        self.window.raw_window_handle()
    }

    #[inline]
    pub fn request_inner_size(&self, size: PhysicalSize<u32>) {
        let _ = self.window.request_inner_size(size);
    }

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.window.inner_size()
    }

    #[inline]
    pub fn set_visible(&self, visibility: bool) {
        self.window.set_visible(visibility);
    }

    /// Set the window title.
    #[inline]
    pub fn set_title(&mut self, title: String) {
        self.title = title;
        self.window.set_title(&self.title);
    }

    /// Get the window title.
    #[inline]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[inline]
    pub fn request_redraw(&mut self) {
        if !self.requested_redraw {
            self.requested_redraw = true;
            self.window.request_redraw();
        }
    }

    #[inline]
    pub fn set_mouse_cursor(&mut self, cursor: CursorIcon) {
        if cursor != self.current_mouse_cursor {
            self.current_mouse_cursor = cursor;
            self.window.set_cursor_icon(cursor);
        }
    }

    /// Set mouse cursor visible.
    pub fn set_mouse_visible(&mut self, visible: bool) {
        if visible != self.mouse_visible {
            self.mouse_visible = visible;
            self.window.set_cursor_visible(visible);
        }
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn get_platform_window(
        identity: &Identity,
        window_config: &WindowConfig,
        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))] x11_visual: Option<
            X11VisualInfo,
        >,
    ) -> WindowBuilder {
        #[cfg(feature = "x11")]
        let icon = {
            let mut decoder = Decoder::new(Cursor::new(WINDOW_ICON));
            decoder.set_transformations(png::Transformations::normalize_to_color8());
            let mut reader = decoder.read_info().expect("invalid embedded icon");
            let mut buf = vec![0; reader.output_buffer_size()];
            let _ = reader.next_frame(&mut buf);
            Icon::from_rgba(buf, reader.info().width, reader.info().height)
                .expect("invalid embedded icon format")
        };

        let builder = WindowBuilder::new()
            .with_name(&identity.class.general, &identity.class.instance)
            .with_decorations(window_config.decorations != Decorations::None);

        #[cfg(feature = "x11")]
        let builder = builder.with_window_icon(Some(icon));

        #[cfg(feature = "x11")]
        let builder = match x11_visual {
            Some(visual) => builder.with_x11_visual(visual.visual_id() as u32),
            None => builder,
        };

        builder
    }

    #[cfg(windows)]
    pub fn get_platform_window(_: &Identity, window_config: &WindowConfig) -> WindowBuilder {
        let icon = winit::window::Icon::from_resource(IDI_ICON, None);

        WindowBuilder::new()
            .with_decorations(window_config.decorations != Decorations::None)
            .with_window_icon(icon.ok())
    }

    #[cfg(target_os = "macos")]
    pub fn get_platform_window(
        _: &Identity,
        window_config: &WindowConfig,
        tabbing_id: &Option<String>,
    ) -> WindowBuilder {
        let mut window = WindowBuilder::new().with_option_as_alt(window_config.option_as_alt());

        if let Some(tabbing_id) = tabbing_id {
            window = window.with_tabbing_identifier(tabbing_id);
        }

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

    pub fn set_urgent(&self, is_urgent: bool) {
        let attention = if is_urgent { Some(UserAttentionType::Critical) } else { None };

        self.window.request_user_attention(attention);
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn set_transparent(&self, transparent: bool) {
        self.window.set_transparent(transparent);
    }

    pub fn set_maximized(&self, maximized: bool) {
        self.window.set_maximized(maximized);
    }

    pub fn set_minimized(&self, minimized: bool) {
        self.window.set_minimized(minimized);
    }

    pub fn set_resize_increments(&self, increments: PhysicalSize<f32>) {
        self.window.set_resize_increments(Some(increments));
    }

    /// Toggle the window's fullscreen state.
    pub fn toggle_fullscreen(&self) {
        self.set_fullscreen(self.window.fullscreen().is_none());
    }

    /// Toggle the window's maximized state.
    pub fn toggle_maximized(&self) {
        self.set_maximized(!self.window.is_maximized());
    }

    /// Inform windowing system about presenting to the window.
    ///
    /// Should be called right before presenting to the window with e.g. `eglSwapBuffers`.
    pub fn pre_present_notify(&self) {
        self.window.pre_present_notify();
    }

    #[cfg(target_os = "macos")]
    pub fn toggle_simple_fullscreen(&self) {
        self.set_simple_fullscreen(!self.window.simple_fullscreen());
    }

    #[cfg(target_os = "macos")]
    pub fn set_option_as_alt(&self, option_as_alt: OptionAsAlt) {
        self.window.set_option_as_alt(option_as_alt);
    }

    pub fn set_fullscreen(&self, fullscreen: bool) {
        if fullscreen {
            self.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
        } else {
            self.window.set_fullscreen(None);
        }
    }

    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        self.window.current_monitor()
    }

    #[cfg(target_os = "macos")]
    pub fn set_simple_fullscreen(&self, simple_fullscreen: bool) {
        self.window.set_simple_fullscreen(simple_fullscreen);
    }

    pub fn set_ime_allowed(&self, allowed: bool) {
        self.window.set_ime_allowed(allowed);
    }

    /// Adjust the IME editor position according to the new location of the cursor.
    pub fn update_ime_position(&self, point: Point<usize>, size: &SizeInfo) {
        let nspot_x = f64::from(size.padding_x() + point.column.0 as f32 * size.cell_width());
        let nspot_y = f64::from(size.padding_y() + (point.line + 1) as f32 * size.cell_height());

        // Exclude the rest of the line since we edit from left to right.
        let width = size.width as f64 - nspot_x;
        let height = size.cell_height as f64;

        self.window.set_ime_cursor_area(
            PhysicalPosition::new(nspot_x, nspot_y),
            PhysicalSize::new(width, height),
        );
    }

    /// Disable macOS window shadows.
    ///
    /// This prevents rendering artifacts from showing up when the window is transparent.
    #[cfg(target_os = "macos")]
    pub fn set_has_shadow(&self, has_shadows: bool) {
        let raw_window = match self.raw_window_handle() {
            RawWindowHandle::AppKit(handle) => handle.ns_window as id,
            _ => return,
        };

        let value = if has_shadows { YES } else { NO };
        unsafe {
            let _: id = msg_send![raw_window, setHasShadow: value];
        }
    }

    /// Select tab at the given `index`.
    #[cfg(target_os = "macos")]
    pub fn select_tab_at_index(&self, index: usize) {
        self.window.select_tab_at_index(index);
    }

    /// Select the last tab.
    #[cfg(target_os = "macos")]
    pub fn select_last_tab(&self) {
        self.window.select_tab_at_index(self.window.num_tabs() - 1);
    }

    /// Select next tab.
    #[cfg(target_os = "macos")]
    pub fn select_next_tab(&self) {
        self.window.select_next_tab();
    }

    /// Select previous tab.
    #[cfg(target_os = "macos")]
    pub fn select_previous_tab(&self) {
        self.window.select_previous_tab();
    }

    #[cfg(target_os = "macos")]
    pub fn tabbing_id(&self) -> String {
        self.window.tabbing_identifier()
    }
}

#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
fn x_embed_window(window: &WinitWindow, parent_id: std::os::raw::c_ulong) {
    let xlib_display = window.raw_display_handle();
    let xlib_window = window.raw_window_handle();
    let (xlib_display, xlib_window) = match (xlib_display, xlib_window) {
        (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(window)) => {
            (display.display, window.window)
        },
        _ => return,
    };

    let xlib = Xlib::open().expect("get xlib");

    unsafe {
        let atom = (xlib.XInternAtom)(xlib_display as *mut _, "_XEMBED".as_ptr() as *const _, 0);
        (xlib.XChangeProperty)(
            xlib_display as _,
            xlib_window,
            atom,
            atom,
            32,
            PropModeReplace,
            [0, 1].as_ptr(),
            2,
        );

        // Register new error handler.
        let old_handler = (xlib.XSetErrorHandler)(Some(xembed_error_handler));

        // Check for the existence of the target before attempting reparenting.
        (xlib.XReparentWindow)(xlib_display as _, xlib_window as _, parent_id, 0, 0);

        // Drain errors and restore original error handler.
        (xlib.XSync)(xlib_display as _, 0);
        (xlib.XSetErrorHandler)(old_handler);
    }
}

#[cfg(target_os = "macos")]
fn use_srgb_color_space(window: &WinitWindow) {
    let raw_window = match window.raw_window_handle() {
        RawWindowHandle::AppKit(handle) => handle.ns_window as id,
        _ => return,
    };

    unsafe {
        let _: () = msg_send![raw_window, setColorSpace: NSColorSpace::sRGBColorSpace(nil)];
    }
}

#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
unsafe extern "C" fn xembed_error_handler(_: *mut XDisplay, _: *mut XErrorEvent) -> i32 {
    log::error!("Could not embed into specified window.");
    std::process::exit(1);
}
