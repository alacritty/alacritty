use std::cmp;
use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

mod scrolling;

use crate::ansi::{CursorShapeShim, CursorStyle};

pub use crate::config::scrolling::{Scrolling, MAX_SCROLLBACK_LINES};

/// Logging target for config error messages.
pub const LOG_TARGET_CONFIG: &str = "alacritty_config_derive";

const MIN_BLINK_INTERVAL: u64 = 10;

/// Top-level config type.
#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Default)]
pub struct Config {
    /// TERM env variable.
    pub env: HashMap<String, String>,

    pub selection: Selection,

    /// How much scrolling history to keep.
    pub scrolling: Scrolling,

    /// Cursor configuration.
    pub cursor: Cursor,

    #[config(flatten)]
    pub pty_config: PtyConfig,
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct PtyConfig {
    /// Path to a shell program to run on startup.
    pub shell: Option<Program>,

    /// Shell startup directory.
    pub working_directory: Option<PathBuf>,

    /// Remain open after child process exits.
    #[config(skip)]
    pub hold: bool,
}

impl PtyConfig {
    pub fn new() -> Self {
        Default::default()
    }
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub semantic_escape_chars: String,
    pub save_to_clipboard: bool,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            semantic_escape_chars: String::from(",│`|:\"' ()[]{}<>\t"),
            save_to_clipboard: Default::default(),
        }
    }
}

#[derive(ConfigDeserialize, Copy, Clone, Debug, PartialEq)]
pub struct Cursor {
    pub style: ConfigCursorStyle,
    pub vi_mode_style: Option<ConfigCursorStyle>,
    pub unfocused_hollow: bool,

    thickness: Percentage,
    blink_interval: u64,
    blink_timeout: u8,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            thickness: Percentage(0.15),
            unfocused_hollow: true,
            blink_interval: 750,
            blink_timeout: 5,
            style: Default::default(),
            vi_mode_style: Default::default(),
        }
    }
}

impl Cursor {
    #[inline]
    pub fn thickness(self) -> f32 {
        self.thickness.as_f32()
    }

    #[inline]
    pub fn style(self) -> CursorStyle {
        self.style.into()
    }

    #[inline]
    pub fn vi_mode_style(self) -> Option<CursorStyle> {
        self.vi_mode_style.map(From::from)
    }

    #[inline]
    pub fn blink_interval(self) -> u64 {
        cmp::max(self.blink_interval, MIN_BLINK_INTERVAL)
    }

    #[inline]
    pub fn blink_timeout(self) -> u64 {
        const MILLIS_IN_SECOND: u64 = 1000;
        match self.blink_timeout {
            0 => 0,
            blink_timeout => {
                cmp::max(self.blink_interval * 5 / MILLIS_IN_SECOND, blink_timeout as u64)
            },
        }
    }
}

#[derive(SerdeReplace, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub enum ConfigCursorStyle {
    Shape(CursorShapeShim),
    WithBlinking {
        #[serde(default)]
        shape: CursorShapeShim,
        #[serde(default)]
        blinking: CursorBlinking,
    },
}

impl Default for ConfigCursorStyle {
    fn default() -> Self {
        Self::Shape(CursorShapeShim::default())
    }
}

impl ConfigCursorStyle {
    /// Check if blinking is force enabled/disabled.
    pub fn blinking_override(&self) -> Option<bool> {
        match self {
            Self::Shape(_) => None,
            Self::WithBlinking { blinking, .. } => blinking.blinking_override(),
        }
    }
}

impl From<ConfigCursorStyle> for CursorStyle {
    fn from(config_style: ConfigCursorStyle) -> Self {
        match config_style {
            ConfigCursorStyle::Shape(shape) => Self { shape: shape.into(), blinking: false },
            ConfigCursorStyle::WithBlinking { shape, blinking } => {
                Self { shape: shape.into(), blinking: blinking.into() }
            },
        }
    }
}

#[derive(ConfigDeserialize, Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum CursorBlinking {
    Never,
    #[default]
    Off,
    On,
    Always,
}

impl CursorBlinking {
    fn blinking_override(&self) -> Option<bool> {
        match self {
            Self::Never => Some(false),
            Self::Off | Self::On => None,
            Self::Always => Some(true),
        }
    }
}

impl From<CursorBlinking> for bool {
    fn from(blinking: CursorBlinking) -> bool {
        blinking == CursorBlinking::On || blinking == CursorBlinking::Always
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub enum Program {
    Just(String),
    WithArgs {
        program: String,
        #[serde(default)]
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
#[derive(SerdeReplace, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Percentage(f32);

impl Default for Percentage {
    fn default() -> Self {
        Percentage(1.0)
    }
}

impl Percentage {
    pub fn new(value: f32) -> Self {
        Percentage(value.clamp(0., 1.))
    }

    pub fn as_f32(self) -> f32 {
        self.0
    }
}
