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

use std::borrow::Cow;
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

use crate::ansi::{Color, CursorStyle, NamedColor};

pub use crate::config::colors::Colors;
pub use crate::config::debug::Debug;
pub use crate::config::font::{Font, FontDescription};
pub use crate::config::scrolling::Scrolling;
pub use crate::config::visual_bell::{VisualBellAnimation, VisualBellConfig};
pub use crate::config::window::{Decorations, Dimensions, StartupMode, WindowConfig, DEFAULT_NAME};
use crate::term::color::Rgb;

pub const LOG_TARGET_CONFIG: &str = "alacritty_config";
const MAX_SCROLLBACK_LINES: u32 = 100_000;

pub type MockConfig = Config<HashMap<String, serde_yaml::Value>>;

/// Top-level config type
#[derive(Debug, PartialEq, Default, Deserialize)]
pub struct Config<T> {
    /// Pixel padding
    #[serde(default, deserialize_with = "failure_default")]
    pub padding: Option<Delta<u8>>,

    /// TERM env variable
    #[serde(default, deserialize_with = "failure_default")]
    pub env: HashMap<String, String>,

    /// Font configuration
    #[serde(default, deserialize_with = "failure_default")]
    pub font: Font,

    /// Should draw bold text with brighter colors instead of bold font
    #[serde(default, deserialize_with = "failure_default")]
    draw_bold_text_with_bright_colors: bool,

    #[serde(default, deserialize_with = "failure_default")]
    pub colors: Colors,

    /// Background opacity from 0.0 to 1.0
    #[serde(default, deserialize_with = "failure_default")]
    background_opacity: Alpha,

    /// Window configuration
    #[serde(default, deserialize_with = "failure_default")]
    pub window: WindowConfig,

    #[serde(default, deserialize_with = "failure_default")]
    pub selection: Selection,

    /// Path to a shell program to run on startup
    #[serde(default, deserialize_with = "from_string_or_deserialize")]
    pub shell: Option<Shell<'static>>,

    /// Path where config was loaded from
    #[serde(default, deserialize_with = "failure_default")]
    pub config_path: Option<PathBuf>,

    /// Visual bell configuration
    #[serde(default, deserialize_with = "failure_default")]
    pub visual_bell: VisualBellConfig,

    /// Use dynamic title
    #[serde(default, deserialize_with = "failure_default")]
    dynamic_title: DefaultTrueBool,

    /// Live config reload
    #[serde(default, deserialize_with = "failure_default")]
    live_config_reload: DefaultTrueBool,

    /// Number of spaces in one tab
    #[serde(default, deserialize_with = "failure_default")]
    tabspaces: Tabspaces,

    /// How much scrolling history to keep
    #[serde(default, deserialize_with = "failure_default")]
    pub scrolling: Scrolling,

    /// Cursor configuration
    #[serde(default, deserialize_with = "failure_default")]
    pub cursor: Cursor,

    /// Use WinPTY backend even if ConPTY is available
    #[cfg(windows)]
    #[serde(default, deserialize_with = "failure_default")]
    pub winpty_backend: bool,

    /// Send escape sequences using the alt key.
    #[serde(default, deserialize_with = "failure_default")]
    alt_send_esc: DefaultTrueBool,

    /// Shell startup directory
    #[serde(default, deserialize_with = "option_explicit_none")]
    pub working_directory: Option<PathBuf>,

    /// Debug options
    #[serde(default, deserialize_with = "failure_default")]
    pub debug: Debug,

    /// Additional configuration options not directly required by the terminal
    #[serde(flatten)]
    pub ui_config: T,

    /// Remain open after child process exits
    #[serde(skip)]
    pub hold: bool,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    pub render_timer: Option<bool>,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    pub persistent_logging: Option<bool>,
}

impl<T> Config<T> {
    pub fn tabspaces(&self) -> usize {
        self.tabspaces.0
    }

    #[inline]
    pub fn draw_bold_text_with_bright_colors(&self) -> bool {
        self.draw_bold_text_with_bright_colors
    }

    /// Should show render timer
    #[inline]
    pub fn render_timer(&self) -> bool {
        self.render_timer.unwrap_or(self.debug.render_timer)
    }

    /// Live config reload
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

    /// Cursor foreground color
    #[inline]
    pub fn cursor_text_color(&self) -> Option<Rgb> {
        self.colors.cursor.text
    }

    /// Cursor background color
    #[inline]
    pub fn cursor_cursor_color(&self) -> Option<Color> {
        self.colors.cursor.cursor.map(|_| Color::Named(NamedColor::Cursor))
    }

    #[inline]
    pub fn set_dynamic_title(&mut self, dynamic_title: bool) {
        self.dynamic_title.0 = dynamic_title;
    }

    /// Send escape sequences using the alt key
    #[inline]
    pub fn alt_send_esc(&self) -> bool {
        self.alt_send_esc.0
    }

    /// Keep the log file after quitting Alacritty
    #[inline]
    pub fn persistent_logging(&self) -> bool {
        self.persistent_logging.unwrap_or(self.debug.persistent_logging)
    }

    #[inline]
    pub fn background_opacity(&self) -> f32 {
        self.background_opacity.0
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
#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Cursor {
    #[serde(deserialize_with = "failure_default")]
    pub style: CursorStyle,
    #[serde(deserialize_with = "failure_default")]
    unfocused_hollow: DefaultTrueBool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self { style: Default::default(), unfocused_hollow: Default::default() }
    }
}

impl Cursor {
    pub fn unfocused_hollow(self) -> bool {
        self.unfocused_hollow.0
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Shell<'a> {
    pub program: Cow<'a, str>,

    #[serde(default, deserialize_with = "failure_default")]
    pub args: Vec<String>,
}

impl<'a> Shell<'a> {
    pub fn new<S>(program: S) -> Shell<'a>
    where
        S: Into<Cow<'a, str>>,
    {
        Shell { program: program.into(), args: Vec::new() }
    }

    pub fn new_with_args<S>(program: S, args: Vec<String>) -> Shell<'a>
    where
        S: Into<Cow<'a, str>>,
    {
        Shell { program: program.into(), args }
    }
}

impl FromString for Option<Shell<'_>> {
    fn from(input: String) -> Self {
        Some(Shell::new(input))
    }
}

/// A delta for a point in a 2 dimensional plane
#[serde(default, bound(deserialize = "T: Deserialize<'de> + Default"))]
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Delta<T: Default + PartialEq + Eq> {
    /// Horizontal change
    #[serde(deserialize_with = "failure_default")]
    pub x: T,
    /// Vertical change
    #[serde(deserialize_with = "failure_default")]
    pub y: T,
}

/// Wrapper around f32 that represents an alpha value between 0.0 and 1.0
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alpha(f32);

impl Alpha {
    pub fn new(value: f32) -> Self {
        Alpha(if value < 0.0 {
            0.0
        } else if value > 1.0 {
            1.0
        } else {
            value
        })
    }
}

impl Default for Alpha {
    fn default() -> Self {
        Alpha(1.0)
    }
}

impl<'a> Deserialize<'a> for Alpha {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        Ok(Alpha::new(f32::deserialize(deserializer)?))
    }
}

#[derive(Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
struct Tabspaces(usize);

impl Default for Tabspaces {
    fn default() -> Self {
        Tabspaces(8)
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

pub fn from_string_or_deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + FromString + Default,
{
    Ok(match Value::deserialize(deserializer)? {
        Value::String(value) => T::from(value),
        value => T::deserialize(value).unwrap_or_else(fallback_default),
    })
}

// Used over From<String>, to allow implementation for foreign types
pub trait FromString {
    fn from(input: String) -> Self;
}
