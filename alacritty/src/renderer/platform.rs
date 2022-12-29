//! The graphics platform that is used by the renderer.

use std::num::NonZeroU32;

use glutin::config::{ColorBufferType, Config, ConfigTemplateBuilder, GetGlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentContext, Version,
};
use glutin::display::{Display, DisplayApiPreference, GetGlDisplay};
use glutin::error::Result as GlutinResult;
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, WindowSurface};

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use winit::dpi::PhysicalSize;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use winit::platform::unix;

/// Create the GL display.
pub fn create_gl_display(
    raw_display_handle: RawDisplayHandle,
    _raw_window_handle: Option<RawWindowHandle>,
) -> GlutinResult<Display> {
    #[cfg(target_os = "macos")]
    let preference = DisplayApiPreference::Cgl;

    #[cfg(windows)]
    let preference = DisplayApiPreference::Wgl(Some(_raw_window_handle.unwrap()));

    #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
    let preference = DisplayApiPreference::GlxThenEgl(Box::new(unix::register_xlib_error_hook));

    #[cfg(all(not(feature = "x11"), not(any(target_os = "macos", windows))))]
    let preference = DisplayApiPreference::Egl;

    let display = unsafe { Display::new(raw_display_handle, preference)? };
    log::info!("Using {}", { display.version_string() });
    Ok(display)
}

pub fn pick_gl_config(
    gl_display: &Display,
    raw_window_handle: Option<RawWindowHandle>,
) -> Result<Config, String> {
    let mut default_config = ConfigTemplateBuilder::new()
        .with_depth_size(0)
        .with_stencil_size(0)
        .with_transparency(true);

    if let Some(raw_window_handle) = raw_window_handle {
        default_config = default_config.compatible_with_native_window(raw_window_handle);
    }

    let config_10bit = default_config
        .clone()
        .with_buffer_type(ColorBufferType::Rgb { r_size: 10, g_size: 10, b_size: 10 })
        .with_alpha_size(2);

    let configs = [
        default_config.clone(),
        config_10bit.clone(),
        default_config.with_transparency(false),
        config_10bit.with_transparency(false),
    ];

    for config in configs {
        let gl_config = unsafe {
            gl_display.find_configs(config.build()).ok().and_then(|mut configs| configs.next())
        };

        if let Some(gl_config) = gl_config {
            return Ok(gl_config);
        }
    }

    Err(String::from("failed to find suitable GL configuration."))
}

pub fn create_gl_context(
    gl_display: &Display,
    gl_config: &Config,
    raw_window_handle: Option<RawWindowHandle>,
) -> GlutinResult<NotCurrentContext> {
    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(raw_window_handle);

    unsafe {
        if let Ok(gl_context) = gl_display.create_context(gl_config, &context_attributes) {
            Ok(gl_context)
        } else {
            let context_attributes = ContextAttributesBuilder::new()
                .with_profile(GlProfile::Compatibility)
                .with_context_api(ContextApi::OpenGl(Some(Version::new(2, 1))))
                .build(raw_window_handle);
            gl_display.create_context(gl_config, &context_attributes)
        }
    }
}

pub fn create_gl_surface(
    gl_context: &NotCurrentContext,
    size: PhysicalSize<u32>,
    raw_window_handle: RawWindowHandle,
) -> GlutinResult<Surface<WindowSurface>> {
    // Get the display and the config used to create that context.
    let gl_display = gl_context.display();
    let gl_config = gl_context.config();

    let surface_attributes =
        SurfaceAttributesBuilder::<WindowSurface>::new().with_srgb(Some(false)).build(
            raw_window_handle,
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );

    // Create the GL surface to draw into.
    unsafe { gl_display.create_window_surface(&gl_config, &surface_attributes) }
}
