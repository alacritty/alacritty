use serde::Deserialize;

use crate::config::{
    failure_default, from_string_or_deserialize, option_explicit_none, Delta, FromString,
};
use crate::index::{Column, Line};

/// Default Alacritty name, used for window title and class.
pub const DEFAULT_NAME: &str = "Alacritty";

#[serde(default)]
#[derive(Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowConfig {
    /// Initial dimensions
    #[serde(deserialize_with = "failure_default")]
    pub dimensions: Dimensions,

    /// Initial position
    #[serde(deserialize_with = "failure_default")]
    pub position: Option<Delta<i32>>,

    /// Pixel padding
    #[serde(deserialize_with = "failure_default")]
    pub padding: Delta<u8>,

    /// Draw the window with title bar / borders
    #[serde(deserialize_with = "failure_default")]
    pub decorations: Decorations,

    /// Spread out additional padding evenly
    #[serde(deserialize_with = "failure_default")]
    pub dynamic_padding: bool,

    /// Startup mode
    #[serde(deserialize_with = "failure_default")]
    startup_mode: StartupMode,

    /// Window title
    #[serde(deserialize_with = "failure_default")]
    title: Option<String>,

    /// Window class
    #[serde(deserialize_with = "from_string_or_deserialize")]
    pub class: Class,

    /// XEmbed parent
    #[serde(skip)]
    pub embed: Option<u64>,

    /// GTK theme variant
    #[serde(deserialize_with = "option_explicit_none")]
    pub gtk_theme_variant: Option<String>,

    /// TODO: DEPRECATED
    #[serde(deserialize_with = "failure_default")]
    pub start_maximized: Option<bool>,
}

impl WindowConfig {
    pub fn startup_mode(&self) -> StartupMode {
        match self.start_maximized {
            Some(true) => StartupMode::Maximized,
            _ => self.startup_mode,
        }
    }

    pub fn set_title(&mut self, title: String) {
        self.title = Some(title);
    }

    pub fn title(&self) -> String {
        self.title.clone().unwrap_or_else(|| DEFAULT_NAME.to_owned())
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

/// Window Dimensions
///
/// Newtype to avoid passing values incorrectly
#[serde(default)]
#[derive(Default, Debug, Copy, Clone, Deserialize, PartialEq, Eq)]
pub struct Dimensions {
    /// Window width in character columns
    #[serde(deserialize_with = "failure_default")]
    columns: Column,

    /// Window Height in character lines
    #[serde(deserialize_with = "failure_default")]
    lines: Line,
}

impl Dimensions {
    pub fn new(columns: Column, lines: Line) -> Self {
        Dimensions { columns, lines }
    }

    /// Get lines
    #[inline]
    pub fn lines_u32(&self) -> u32 {
        self.lines.0 as u32
    }

    /// Get columns
    #[inline]
    pub fn columns_u32(&self) -> u32 {
        self.columns.0 as u32
    }
}

/// Window class hint
#[serde(default)]
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub instance: String,
    pub general: String,
}

impl Default for Class {
    fn default() -> Self {
        Class { instance: DEFAULT_NAME.into(), general: DEFAULT_NAME.into() }
    }
}

impl FromString for Class {
    fn from(value: String) -> Self {
        Class { instance: value, general: DEFAULT_NAME.into() }
    }
}
