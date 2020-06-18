use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;

use log::error;
use serde::{Deserialize, Deserializer};
use serde_yaml::Value;

mod colors;
mod debug;
mod font;
mod scrolling;
mod visual_bell;
mod window;

use crate::ansi::CursorStyle;

pub use crate::config::colors::Colors;
pub use crate::config::debug::Debug;
pub use crate::config::font::{Font, FontDescription};
pub use crate::config::scrolling::Scrolling;
pub use crate::config::visual_bell::{VisualBellAnimation, VisualBellConfig};
pub use crate::config::window::{Decorations, Dimensions, StartupMode, WindowConfig, DEFAULT_NAME};

pub const LOG_TARGET_CONFIG: &str = "alacritty_config";
const MAX_SCROLLBACK_LINES: u32 = 100_000;
const DEFAULT_CURSOR_THICKNESS: f32 = 0.15;

pub type MockConfig = Config<HashMap<String, serde_yaml::Value>>;

/// Top-level config type.
#[derive(Debug, PartialEq, Default, Deserialize)]
pub struct Config<T> {
    /// Pixel padding.
    #[serde(default, deserialize_with = "failure_default")]
    pub padding: Option<Delta<u8>>,

    /// TERM env variable.
    #[serde(default, deserialize_with = "failure_default")]
    pub env: HashMap<String, String>,

    /// Font configuration.
    #[serde(default, deserialize_with = "failure_default")]
    pub font: Font,

    /// Should draw bold text with brighter colors instead of bold font.
    #[serde(default, deserialize_with = "failure_default")]
    draw_bold_text_with_bright_colors: bool,

    #[serde(default, deserialize_with = "failure_default")]
    pub colors: Colors,

    /// Background opacity from 0.0 to 1.0.
    #[serde(default, deserialize_with = "failure_default")]
    background_opacity: Percentage,

    /// Window configuration.
    #[serde(default, deserialize_with = "failure_default")]
    pub window: WindowConfig,

    #[serde(default, deserialize_with = "failure_default")]
    pub selection: Selection,

    /// Path to a shell program to run on startup.
    #[serde(default, deserialize_with = "failure_default")]
    pub shell: Option<Program>,

    /// Path where config was loaded from.
    #[serde(default, deserialize_with = "failure_default")]
    pub config_path: Option<PathBuf>,

    /// Visual bell configuration.
    #[serde(default, deserialize_with = "failure_default")]
    pub visual_bell: VisualBellConfig,

    /// Use dynamic title.
    #[serde(default, deserialize_with = "failure_default")]
    dynamic_title: DefaultTrueBool,

    /// Live config reload.
    #[serde(default, deserialize_with = "failure_default")]
    live_config_reload: DefaultTrueBool,

    /// How much scrolling history to keep.
    #[serde(default, deserialize_with = "failure_default")]
    pub scrolling: Scrolling,

    /// Cursor configuration.
    #[serde(default, deserialize_with = "failure_default")]
    pub cursor: Cursor,

    /// Use WinPTY backend even if ConPTY is available.
    #[cfg(windows)]
    #[serde(default, deserialize_with = "failure_default")]
    pub winpty_backend: bool,

    /// Send escape sequences using the alt key.
    #[serde(default, deserialize_with = "failure_default")]
    alt_send_esc: DefaultTrueBool,

    /// Shell startup directory.
    #[serde(default, deserialize_with = "option_explicit_none")]
    pub working_directory: Option<PathBuf>,

    /// Debug options.
    #[serde(default, deserialize_with = "failure_default")]
    pub debug: Debug,

    /// Additional configuration options not directly required by the terminal.
    #[serde(flatten)]
    pub ui_config: T,

    /// Remain open after child process exits.
    #[serde(skip)]
    pub hold: bool,

    // TODO: REMOVED
    #[serde(default, deserialize_with = "failure_default")]
    pub tabspaces: Option<usize>,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    pub render_timer: Option<bool>,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    pub persistent_logging: Option<bool>,
}

impl<T> Config<T> {
    #[inline]
    pub fn draw_bold_text_with_bright_colors(&self) -> bool {
        self.draw_bold_text_with_bright_colors
    }

    /// Should show render timer.
    #[inline]
    pub fn render_timer(&self) -> bool {
        self.render_timer.unwrap_or(self.debug.render_timer)
    }

    /// Live config reload.
    #[inline]
    pub fn live_config_reload(&self) -> bool {
        self.live_config_reload.0
    }

    #[inline]
    pub fn set_live_config_reload(&mut self, live_config_reload: bool) {
        self.live_config_reload.0 = live_config_reload;
    }

    #[inline]
    pub fn dynamic_title(&self) -> bool {
        self.dynamic_title.0
    }

    #[inline]
    pub fn set_dynamic_title(&mut self, dynamic_title: bool) {
        self.dynamic_title.0 = dynamic_title;
    }

    /// Send escape sequences using the alt key.
    #[inline]
    pub fn alt_send_esc(&self) -> bool {
        self.alt_send_esc.0
    }

    /// Keep the log file after quitting Alacritty.
    #[inline]
    pub fn persistent_logging(&self) -> bool {
        self.persistent_logging.unwrap_or(self.debug.persistent_logging)
    }

    #[inline]
    pub fn background_opacity(&self) -> f32 {
        self.background_opacity.0 as f32
    }
}

#[serde(default)]
#[derive(Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    #[serde(deserialize_with = "failure_default")]
    semantic_escape_chars: EscapeChars,
    #[serde(deserialize_with = "failure_default")]
    pub save_to_clipboard: bool,
}

impl Selection {
    pub fn semantic_escape_chars(&self) -> &str {
        &self.semantic_escape_chars.0
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
struct EscapeChars(String);

impl Default for EscapeChars {
    fn default() -> Self {
        EscapeChars(String::from(",│`|:\"' ()[]{}<>\t"))
    }
}

#[serde(default)]
#[derive(Deserialize, Copy, Clone, Debug, PartialEq)]
pub struct Cursor {
    #[serde(deserialize_with = "failure_default")]
    pub style: CursorStyle,
    #[serde(deserialize_with = "option_explicit_none")]
    pub vi_mode_style: Option<CursorStyle>,
    #[serde(deserialize_with = "deserialize_cursor_thickness")]
    thickness: Percentage,
    #[serde(deserialize_with = "failure_default")]
    unfocused_hollow: DefaultTrueBool,
}

impl Cursor {
    #[inline]
    pub fn unfocused_hollow(self) -> bool {
        self.unfocused_hollow.0
    }

    #[inline]
    pub fn thickness(self) -> f64 {
        self.thickness.0 as f64
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            style: Default::default(),
            vi_mode_style: Default::default(),
            thickness: Percentage::new(DEFAULT_CURSOR_THICKNESS),
            unfocused_hollow: Default::default(),
        }
    }
}

fn deserialize_cursor_thickness<'a, D>(deserializer: D) -> Result<Percentage, D::Error>
where
    D: Deserializer<'a>,
{
    let value = Value::deserialize(deserializer)?;
    match Percentage::deserialize(value) {
        Ok(value) => Ok(value),
        Err(err) => {
            error!(
                target: LOG_TARGET_CONFIG,
                "Problem with config: {}, using default thickness value {}",
                err,
                DEFAULT_CURSOR_THICKNESS
            );

            Ok(Percentage::new(DEFAULT_CURSOR_THICKNESS))
        },
    }
}

#[serde(untagged)]
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Program {
    Just(String),
    WithArgs {
        program: String,
        #[serde(default, deserialize_with = "failure_default")]
        args: Vec<String>,
    },
}

impl Program {
    pub fn program(&self) -> &str {
        match self {
            Program::Just(program) => program,
            Program::WithArgs { program, .. } => program,
        }
    }

    pub fn args(&self) -> &[String] {
        match self {
            Program::Just(_) => &[],
            Program::WithArgs { args, .. } => args,
        }
    }
}

/// A delta for a point in a 2 dimensional plane.
#[serde(default, bound(deserialize = "T: Deserialize<'de> + Default"))]
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Delta<T: Default + PartialEq + Eq> {
    /// Horizontal change.
    #[serde(deserialize_with = "failure_default")]
    pub x: T,
    /// Vertical change.
    #[serde(deserialize_with = "failure_default")]
    pub y: T,
}

/// Wrapper around f32 that represents a percentage value between 0.0 and 1.0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Percentage(f32);

impl Percentage {
    pub fn new(value: f32) -> Self {
        Percentage(if value < 0.0 {
            0.0
        } else if value > 1.0 {
            1.0
        } else {
            value
        })
    }
}

impl Default for Percentage {
    fn default() -> Self {
        Percentage(1.0)
    }
}

impl<'a> Deserialize<'a> for Percentage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        Ok(Percentage::new(f32::deserialize(deserializer)?))
    }
}

#[derive(Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
struct DefaultTrueBool(bool);

impl Default for DefaultTrueBool {
    fn default() -> Self {
        DefaultTrueBool(true)
    }
}

fn fallback_default<T, E>(err: E) -> T
where
    T: Default,
    E: Display,
{
    error!(target: LOG_TARGET_CONFIG, "Problem with config: {}; using default value", err);
    T::default()
}

pub fn failure_default<'a, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'a>,
    T: Deserialize<'a> + Default,
{
    Ok(T::deserialize(Value::deserialize(deserializer)?).unwrap_or_else(fallback_default))
}

pub fn option_explicit_none<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(match Value::deserialize(deserializer)? {
        Value::String(ref value) if value.to_lowercase() == "none" => None,
        value => Some(T::deserialize(value).unwrap_or_else(fallback_default)),
    })
}
