use std::os::raw::c_ulong;

use glutin::window::Fullscreen;
use serde::Deserialize;

use alacritty_config_derive::ConfigDeserialize;
use alacritty_terminal::index::{Column, Line};

use crate::config::ui_config::Delta;

/// Default Alacritty name, used for window title and class.
pub const DEFAULT_NAME: &str = "Alacritty";

#[derive(ConfigDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    /// Initial position.
    pub position: Option<Delta<i32>>,

    /// Draw the window with title bar / borders.
    pub decorations: Decorations,

    /// Startup mode.
    pub startup_mode: StartupMode,

    /// XEmbed parent.
    #[config(skip)]
    pub embed: Option<c_ulong>,

    /// GTK theme variant.
    pub gtk_theme_variant: Option<String>,

    /// Spread out additional padding evenly.
    pub dynamic_padding: bool,

    /// Use dynamic title.
    pub dynamic_title: bool,

    /// Window title.
    pub title: String,

    /// Window class.
    class: Class,

    /// Pixel padding.
    padding: Delta<u8>,

    /// Initial dimensions.
    dimensions: Dimensions,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            dynamic_title: true,
            title: DEFAULT_NAME.into(),
            position: Default::default(),
            decorations: Default::default(),
            startup_mode: Default::default(),
            embed: Default::default(),
            gtk_theme_variant: Default::default(),
            dynamic_padding: Default::default(),
            class: Default::default(),
            padding: Default::default(),
            dimensions: Default::default(),
        }
    }
}

impl WindowConfig {
    #[inline]
    pub fn dimensions(&self) -> Option<Dimensions> {
        if self.dimensions.columns.0 != 0
            && self.dimensions.lines.0 != 0
            && self.startup_mode != StartupMode::Maximized
        {
            Some(self.dimensions)
        } else {
            None
        }
    }

    #[inline]
    pub fn padding(&self, dpr: f64) -> (f32, f32) {
        let padding_x = (f32::from(self.padding.x) * dpr as f32).floor();
        let padding_y = (f32::from(self.padding.y) * dpr as f32).floor();
        (padding_x, padding_y)
    }

    #[inline]
    pub fn fullscreen(&self) -> Option<Fullscreen> {
        if self.startup_mode == StartupMode::Fullscreen {
            Some(Fullscreen::Borderless(None))
        } else {
            None
        }
    }

    #[inline]
    pub fn maximized(&self) -> bool {
        self.startup_mode == StartupMode::Maximized
    }

    #[inline]
    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn instance(&self) -> &str {
        match &self.class {
            Class::Just(instance) | Class::WithGeneral { instance, .. } => instance.as_str(),
        }
    }

    #[inline]
    pub fn set_instance(&mut self, instance: String) {
        match &mut self.class {
            Class::Just(i) | Class::WithGeneral { instance: i, .. } => *i = instance,
        }
    }

    #[inline]
    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn general(&self) -> &str {
        match &self.class {
            Class::Just(_) => DEFAULT_NAME,
            Class::WithGeneral { general, .. } => general.as_str(),
        }
    }

    #[inline]
    pub fn set_general(&mut self, general: String) {
        match &mut self.class {
            Class::Just(instance) => {
                let instance = instance.clone();
                self.class = Class::WithGeneral { instance, general };
            },
            Class::WithGeneral { general: g, .. } => *g = general,
        }
    }
}

#[derive(ConfigDeserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub enum StartupMode {
    Windowed,
    Maximized,
    Fullscreen,
    #[cfg(target_os = "macos")]
    SimpleFullscreen,
}

impl Default for StartupMode {
    fn default() -> StartupMode {
        StartupMode::Windowed
    }
}

#[derive(ConfigDeserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Decorations {
    Full,
    #[cfg(target_os = "macos")]
    Transparent,
    #[cfg(target_os = "macos")]
    Buttonless,
    None,
}

impl Default for Decorations {
    fn default() -> Decorations {
        Decorations::Full
    }
}

/// Window Dimensions.
///
/// Newtype to avoid passing values incorrectly.
#[derive(ConfigDeserialize, Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Dimensions {
    /// Window width in character columns.
    pub columns: Column,

    /// Window Height in character lines.
    pub lines: Line,
}

/// Window class hint.
#[serde(untagged)]
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
enum Class {
    Just(String),
    WithGeneral { instance: String, general: String },
}

impl Default for Class {
    fn default() -> Self {
        Class::Just(DEFAULT_NAME.into())
    }
}
