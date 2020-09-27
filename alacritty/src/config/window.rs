use std::os::raw::c_ulong;

use log::error;
use serde::{Deserialize, Deserializer};
use serde_yaml::Value;

use alacritty_terminal::config::{failure_default, option_explicit_none, LOG_TARGET_CONFIG};
use alacritty_terminal::index::{Column, Line};

use crate::config::ui_config::{DefaultTrueBool, Delta};

/// Default Alacritty name, used for window title and class.
pub const DEFAULT_NAME: &str = "Alacritty";

#[serde(default)]
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    /// Initial position.
    #[serde(deserialize_with = "failure_default")]
    pub position: Option<Delta<i32>>,

    /// Draw the window with title bar / borders.
    #[serde(deserialize_with = "failure_default")]
    pub decorations: Decorations,

    /// Startup mode.
    #[serde(deserialize_with = "failure_default")]
    pub startup_mode: StartupMode,

    /// Window title.
    #[serde(default = "default_title")]
    pub title: String,

    /// Window class.
    #[serde(deserialize_with = "deserialize_class")]
    pub class: Class,

    /// XEmbed parent.
    #[serde(skip)]
    pub embed: Option<c_ulong>,

    /// GTK theme variant.
    #[serde(deserialize_with = "option_explicit_none")]
    pub gtk_theme_variant: Option<String>,

    /// Spread out additional padding evenly.
    #[serde(deserialize_with = "failure_default")]
    pub dynamic_padding: bool,

    /// Pixel padding.
    #[serde(deserialize_with = "failure_default")]
    padding: Delta<u8>,

    /// Use dynamic title.
    #[serde(default, deserialize_with = "failure_default")]
    dynamic_title: DefaultTrueBool,

    /// Initial dimensions.
    #[serde(deserialize_with = "failure_default")]
    dimensions: Dimensions,
}

pub fn default_title() -> String {
    DEFAULT_NAME.to_string()
}

impl WindowConfig {
    #[inline]
    pub fn dynamic_title(&self) -> bool {
        self.dynamic_title.0
    }

    #[inline]
    pub fn set_dynamic_title(&mut self, dynamic_title: bool) {
        self.dynamic_title.0 = dynamic_title;
    }

    #[inline]
    pub fn set_dimensions(&mut self, dimensions: Dimensions) {
        self.dimensions = dimensions;
    }

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
}

impl Default for WindowConfig {
    fn default() -> WindowConfig {
        WindowConfig {
            dimensions: Default::default(),
            position: Default::default(),
            padding: Default::default(),
            decorations: Default::default(),
            dynamic_padding: Default::default(),
            startup_mode: Default::default(),
            class: Default::default(),
            embed: Default::default(),
            gtk_theme_variant: Default::default(),
            title: default_title(),
            dynamic_title: Default::default(),
        }
    }
}

#[derive(Debug, Deserialize, Copy, Clone, PartialEq, Eq)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize)]
pub enum Decorations {
    #[serde(rename = "full")]
    Full,
    #[cfg(target_os = "macos")]
    #[serde(rename = "transparent")]
    Transparent,
    #[cfg(target_os = "macos")]
    #[serde(rename = "buttonless")]
    Buttonless,
    #[serde(rename = "none")]
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
#[serde(default)]
#[derive(Default, Debug, Copy, Clone, Deserialize, PartialEq, Eq)]
pub struct Dimensions {
    /// Window width in character columns.
    #[serde(deserialize_with = "failure_default")]
    pub columns: Column,

    /// Window Height in character lines.
    #[serde(deserialize_with = "failure_default")]
    pub lines: Line,
}

/// Window class hint.
#[serde(default)]
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Class {
    #[serde(deserialize_with = "deserialize_class_resource")]
    pub instance: String,

    #[serde(deserialize_with = "deserialize_class_resource")]
    pub general: String,
}

impl Default for Class {
    fn default() -> Self {
        Class { instance: DEFAULT_NAME.into(), general: DEFAULT_NAME.into() }
    }
}

fn deserialize_class_resource<'a, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'a>,
{
    let value = Value::deserialize(deserializer)?;
    match String::deserialize(value) {
        Ok(value) => Ok(value),
        Err(err) => {
            error!(
                target: LOG_TARGET_CONFIG,
                "Problem with config: {}, using default value {}", err, DEFAULT_NAME,
            );

            Ok(DEFAULT_NAME.into())
        },
    }
}

fn deserialize_class<'a, D>(deserializer: D) -> Result<Class, D::Error>
where
    D: Deserializer<'a>,
{
    let value = Value::deserialize(deserializer)?;

    if let Value::String(instance) = value {
        return Ok(Class { instance, general: DEFAULT_NAME.into() });
    }

    match Class::deserialize(value) {
        Ok(value) => Ok(value),
        Err(err) => {
            error!(
                target: LOG_TARGET_CONFIG,
                "Problem with config: {}; using class {}", err, DEFAULT_NAME
            );
            Ok(Class::default())
        },
    }
}
