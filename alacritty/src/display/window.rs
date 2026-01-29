use std::fmt::{self, Display, Formatter};

use bitflags::bitflags;
#[rustfmt::skip]
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use {
    std::io::Cursor,
    glutin::platform::x11::X11VisualInfo,
    winit::icon::RgbaIcon,
    winit::platform::x11::WindowAttributesX11,
};
use winit::cursor::CursorIcon;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::{Fullscreen, MonitorHandle};
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use winit::platform::wayland::WindowAttributesWayland;
#[cfg(windows)]
use winit::platform::windows::{WinIcon, WindowAttributesWindows};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{
    ImePurpose, Theme, UserAttentionType, Window as WinitWindow, WindowAttributes, WindowId,
};
#[cfg(target_os = "macos")]
use {
    objc2::MainThreadMarker,
    objc2_app_kit::{NSColorSpace, NSView},
    winit::platform::macos::{OptionAsAlt, WindowAttributesMacOS, WindowExtMacOS},
};
#[cfg(not(any(target_os = "macos", windows)))]
use {
    winit::platform::startup_notify::{self, EventLoopExtStartupNotify},
    winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
    winit::window::ActivationToken,
};

use alacritty_terminal::index::Point;

use crate::cli::WindowOptions;
use crate::config::UiConfig;
use crate::config::window::{Decorations, Identity, WindowConfig};
use crate::display::SizeInfo;

/// Window icon for `_NET_WM_ICON` property.
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
const WINDOW_ICON: &[u8] = include_bytes!("../../extra/logo/compat/alacritty-term.png");

/// This should match the definition of IDI_ICON from `alacritty.rc`.
#[cfg(windows)]
const IDI_ICON: u16 = 0x101;

/// Window errors.
#[derive(Debug)]
pub enum Error {
    /// Error creating the window.
    WindowCreation(winit::error::RequestError),

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
            Error::WindowCreation(err) => write!(f, "Error creating GL context; {err}"),
            Error::Font(err) => err.fmt(f),
        }
    }
}

impl From<winit::error::RequestError> for Error {
    fn from(val: winit::error::RequestError) -> Self {
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

    /// Hold the window when terminal exits.
    pub hold: bool,

    window: Box<dyn WinitWindow>,

    /// Current window title.
    title: String,

    is_x11: bool,
    current_mouse_cursor: CursorIcon,
    mouse_visible: bool,
    ime_inhibitor: ImeInhibitor,
}

impl Window {
    /// Create a new window.
    ///
    /// This creates a window and fully initializes a window.
    pub fn new(
        event_loop: &dyn ActiveEventLoop,
        config: &UiConfig,
        identity: &Identity,
        options: &mut WindowOptions,
        #[rustfmt::skip]
        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        x11_visual: Option<X11VisualInfo>,
    ) -> Result<Window> {
        let mut window_attributes = Window::get_platform_window_attributes(
            #[cfg(not(any(target_os = "macos", windows)))]
            event_loop,
            identity,
            &config.window,
            #[cfg(not(any(target_os = "macos", windows)))]
            options.activation_token.take(),
            #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
            x11_visual,
            #[cfg(target_os = "macos")]
            &options.window_tabbing_id.take(),
        );

        if let Some(position) = config.window.position {
            window_attributes = window_attributes
                .with_position(PhysicalPosition::<i32>::from((position.x, position.y)));
        }

        window_attributes = window_attributes
            .with_title(&identity.title)
            .with_theme(config.window.theme())
            .with_visible(false)
            .with_transparent(true)
            .with_blur(config.window.blur)
            .with_maximized(config.window.maximized())
            .with_fullscreen(config.window.fullscreen())
            .with_window_level(config.window.level.into());

        let window = event_loop.create_window(window_attributes)?;

        // Text cursor.
        let current_mouse_cursor = CursorIcon::Text;
        window.set_cursor(current_mouse_cursor.into());

        // Enable IME.
        #[allow(deprecated)]
        window.set_ime_allowed(true);
        #[allow(deprecated)]
        window.set_ime_purpose(ImePurpose::Terminal);

        // Set initial transparency hint.
        window.set_transparent(config.window_opacity() < 1.);

        #[cfg(target_os = "macos")]
        use_srgb_color_space(window.as_ref());

        let scale_factor = window.scale_factor();
        log::info!("Window scale factor: {scale_factor}");
        let is_x11 = matches!(window.window_handle().unwrap().as_raw(), RawWindowHandle::Xlib(_));

        Ok(Self {
            hold: options.terminal_options.hold,
            requested_redraw: false,
            title: identity.title.clone(),
            current_mouse_cursor,
            mouse_visible: true,
            has_frame: true,
            scale_factor,
            window,
            is_x11,
            ime_inhibitor: Default::default(),
        })
    }

    #[inline]
    pub fn raw_window_handle(&self) -> RawWindowHandle {
        self.window.window_handle().unwrap().as_raw()
    }

    #[inline]
    pub fn request_inner_size(&self, size: PhysicalSize<u32>) {
        let _ = self.window.request_surface_size(size.into());
    }

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.window.surface_size()
    }

    #[inline]
    pub fn set_visible(&self, visibility: bool) {
        self.window.set_visible(visibility);
    }

    #[cfg(target_os = "macos")]
    #[inline]
    pub fn focus_window(&self) {
        self.window.focus_window();
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
            self.window.set_cursor(cursor.into());
        }
    }

    /// Set mouse cursor visible.
    pub fn set_mouse_visible(&mut self, visible: bool) {
        if visible != self.mouse_visible {
            self.mouse_visible = visible;
            self.window.set_cursor_visible(visible);
        }
    }

    #[inline]
    pub fn mouse_visible(&self) -> bool {
        self.mouse_visible
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn get_platform_window_attributes(
        event_loop: &dyn ActiveEventLoop,
        identity: &Identity,
        window_config: &WindowConfig,
        activation_token: Option<String>,
        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))] x11_visual: Option<
            X11VisualInfo,
        >,
    ) -> WindowAttributes {
        let activation_token = activation_token
            .map(ActivationToken::from_raw)
            .or_else(|| event_loop.read_token_from_env())
            .map(|activation_token| {
                log::debug!("Activating window with token: {activation_token:?}");
                // Remove the token from the env.
                startup_notify::reset_activation_token_env();
                activation_token
            });

        match event_loop.display_handle().unwrap().as_raw() {
            #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
            RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => {
                Self::get_x11_window_attributes(
                    identity,
                    window_config,
                    activation_token,
                    x11_visual,
                )
            },
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            RawDisplayHandle::Wayland(_) => {
                Self::get_wayland_window_attributes(identity, window_config, activation_token)
            },
            _ => unreachable!(),
        }
    }

    #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
    fn get_wayland_window_attributes(
        identity: &Identity,
        window_config: &WindowConfig,
        activation_token: Option<ActivationToken>,
    ) -> WindowAttributes {
        let mut attrs = WindowAttributesWayland::default()
            .with_name(&identity.class.general, &identity.class.instance);

        if let Some(activation_token) = activation_token {
            attrs = attrs.with_activation_token(activation_token);
        }

        WindowAttributes::default()
            .with_decorations(window_config.decorations != Decorations::None)
            .with_platform_attributes(Box::new(attrs))
    }

    #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
    fn get_x11_window_attributes(
        identity: &Identity,
        window_config: &WindowConfig,
        activation_token: Option<ActivationToken>,
        x11_visual: Option<X11VisualInfo>,
    ) -> WindowAttributes {
        let mut decoder = png::Decoder::new(Cursor::new(WINDOW_ICON));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().expect("invalid embedded icon");
        let mut buf = vec![0; reader.output_buffer_size()];
        let _ = reader.next_frame(&mut buf);
        let reader_info = reader.info();
        let icon = RgbaIcon::new(buf, reader_info.width, reader_info.height)
            .expect("invalid embedded icon format")
            .into();

        let mut attrs = WindowAttributesX11::default()
            .with_x11_visual(x11_visual.unwrap().visual_id() as _)
            .with_name(&identity.class.general, &identity.class.instance);

        if let Some(embed) = window_config.embed {
            attrs = attrs.with_embed_parent_window(embed);
        }

        if let Some(activation_token) = activation_token {
            attrs = attrs.with_activation_token(activation_token);
        }

        WindowAttributes::default()
            .with_decorations(window_config.decorations != Decorations::None)
            .with_window_icon(Some(icon))
            .with_platform_attributes(Box::new(attrs))
    }

    #[cfg(windows)]
    pub fn get_platform_window_attributes(
        _: &Identity,
        window_config: &WindowConfig,
    ) -> WindowAttributes {
        let icon = WinIcon::from_resource(IDI_ICON, None).ok().map(Into::into);
        let attrs = WindowAttributesWindows::default().with_taskbar_icon(icon.clone());

        WindowAttributes::default()
            .with_decorations(window_config.decorations != Decorations::None)
            .with_window_icon(icon)
            .with_platform_attributes(Box::new(attrs))
    }

    #[cfg(target_os = "macos")]
    pub fn get_platform_window_attributes(
        _: &Identity,
        window_config: &WindowConfig,
        tabbing_id: &Option<String>,
    ) -> WindowAttributes {
        let mut attrs =
            WindowAttributesMacOS::default().with_option_as_alt(window_config.option_as_alt());

        if let Some(tabbing_id) = tabbing_id {
            attrs = attrs.with_tabbing_identifier(tabbing_id);
        }

        attrs = match window_config.decorations {
            Decorations::Full => attrs,
            Decorations::Transparent => attrs
                .with_title_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true),
            Decorations::Buttonless => attrs
                .with_title_hidden(true)
                .with_titlebar_buttons_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true),
            Decorations::None => attrs.with_titlebar_hidden(true),
        };

        WindowAttributes::default().with_platform_attributes(Box::new(attrs))
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

    pub fn set_blur(&self, blur: bool) {
        self.window.set_blur(blur);
    }

    pub fn set_maximized(&self, maximized: bool) {
        self.window.set_maximized(maximized);
    }

    pub fn set_minimized(&self, minimized: bool) {
        self.window.set_minimized(minimized);
    }

    pub fn set_resize_increments(&self, increments: PhysicalSize<f32>) {
        self.window.set_surface_resize_increments(Some(increments.into()));
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

    pub fn set_theme(&self, theme: Option<Theme>) {
        self.window.set_theme(theme);
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

    /// Set IME inhibitor state and disable IME while any are present.
    ///
    /// IME is re-enabled once all inhibitors are unset.
    pub fn set_ime_inhibitor(&mut self, inhibitor: ImeInhibitor, inhibit: bool) {
        if self.ime_inhibitor.contains(inhibitor) != inhibit {
            self.ime_inhibitor.set(inhibitor, inhibit);
            #[allow(deprecated)]
            self.window.set_ime_allowed(self.ime_inhibitor.is_empty());
        }
    }

    /// Adjust the IME editor position according to the new location of the cursor.
    pub fn update_ime_position(&self, point: Point<usize>, size: &SizeInfo) {
        // NOTE: X11 doesn't support cursor area, so we need to offset manually to not obscure
        // the text.
        let offset = if self.is_x11 { 1 } else { 0 };
        let nspot_x = f64::from(size.padding_x() + point.column.0 as f32 * size.cell_width());
        let nspot_y =
            f64::from(size.padding_y() + (point.line + offset) as f32 * size.cell_height());

        // NOTE: some compositors don't like excluding too much and try to render popup at the
        // bottom right corner of the provided area, so exclude just the full-width char to not
        // obscure the cursor and not render popup at the end of the window.
        let width = size.cell_width() as f64 * 2.;
        let height = size.cell_height as f64;

        #[allow(deprecated)]
        self.window.set_ime_cursor_area(
            PhysicalPosition::new(nspot_x, nspot_y).into(),
            PhysicalSize::new(width, height).into(),
        );
    }

    /// Disable macOS window shadows.
    ///
    /// This prevents rendering artifacts from showing up when the window is transparent.
    #[cfg(target_os = "macos")]
    pub fn set_has_shadow(&self, has_shadows: bool) {
        let view = match self.raw_window_handle() {
            RawWindowHandle::AppKit(handle) => {
                assert!(MainThreadMarker::new().is_some());
                unsafe { handle.ns_view.cast::<NSView>().as_ref() }
            },
            _ => return,
        };

        view.window().unwrap().setHasShadow(has_shadows);
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

bitflags! {
    /// IME inhibition sources.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ImeInhibitor: u8 {
        const FOCUS = 1;
        const TOUCH = 1 << 1;
        const VI    = 1 << 2;
    }
}

#[cfg(target_os = "macos")]
fn use_srgb_color_space(window: &dyn WinitWindow) {
    let view = match window.window_handle().unwrap().as_raw() {
        RawWindowHandle::AppKit(handle) => {
            assert!(MainThreadMarker::new().is_some());
            unsafe { handle.ns_view.cast::<NSView>().as_ref() }
        },
        _ => return,
    };

    view.window().unwrap().setColorSpace(Some(&NSColorSpace::sRGBColorSpace()));
}
