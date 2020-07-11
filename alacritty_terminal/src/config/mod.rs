use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;

use log::error;
use serde::{Deserialize, Deserializer};
use serde_yaml::Value;

mod bell;
mod colors;
mod scrolling;

use crate::ansi::CursorStyle;

pub use crate::config::bell::{BellAnimation, BellConfig};
pub use crate::config::colors::Colors;
pub use crate::config::scrolling::Scrolling;

pub const LOG_TARGET_CONFIG: &str = "alacritty_config";
const MAX_SCROLLBACK_LINES: u32 = 100_000;
const DEFAULT_CURSOR_THICKNESS: f32 = 0.15;

pub type MockConfig = Config<HashMap<String, serde_yaml::Value>>;

/// Top-level config type.
#[derive(Debug, PartialEq, Default, Deserialize)]
pub struct Config<T> {
    /// TERM env variable.
    #[serde(default, deserialize_with = "failure_default")]
    pub env: HashMap<String, String>,

    /// Should draw bold text with brighter colors instead of bold font.
    #[serde(default, deserialize_with = "failure_default")]
    draw_bold_text_with_bright_colors: bool,

    #[serde(default, deserialize_with = "failure_default")]
    pub colors: Colors,

    #[serde(default, deserialize_with = "failure_default")]
    pub selection: Selection,

    /// Path to a shell program to run on startup.
    #[serde(default, deserialize_with = "failure_default")]
    pub shell: Option<Program>,

    /// Path where config was loaded from.
    #[serde(default, deserialize_with = "failure_default")]
    pub config_path: Option<PathBuf>,

    /// Bell configuration.
    #[serde(default, deserialize_with = "failure_default")]
    bell: BellConfig,

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

    /// Shell startup directory.
    #[serde(default, deserialize_with = "option_explicit_none")]
    pub working_directory: Option<PathBuf>,

    /// Additional configuration options not directly required by the terminal.
    #[serde(flatten)]
    pub ui_config: T,

    /// Remain open after child process exits.
    #[serde(skip)]
    pub hold: bool,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    pub visual_bell: Option<BellConfig>,

    // TODO: REMOVED
    #[serde(default, deserialize_with = "failure_default")]
    pub tabspaces: Option<usize>,
}

impl<T> Config<T> {
    #[inline]
    pub fn draw_bold_text_with_bright_colors(&self) -> bool {
        self.draw_bold_text_with_bright_colors
    }

    #[inline]
    pub fn bell(&self) -> &BellConfig {
        self.visual_bell.as_ref().unwrap_or(&self.bell)
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

    pub fn as_f32(self) -> f32 {
        self.0
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
