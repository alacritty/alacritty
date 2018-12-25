//! Configuration definitions and file loading
//!
//! Alacritty reads from a config file at startup to determine various runtime
//! parameters including font family and style, font size, etc. In the future,
//! the config file will also hold user and platform specific keybindings.
use std::borrow::Cow;
use std::{env, fmt};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc;
use std::time::Duration;
use std::collections::HashMap;

use crate::Rgb;
use font::Size;
use serde_yaml;
use serde::{self, de, Deserialize};
use serde::de::Error as SerdeError;
use serde::de::{Visitor, MapAccess, Unexpected};
use notify::{Watcher, watcher, DebouncedEvent, RecursiveMode};

use glutin::ModifiersState;

use crate::cli::Options;
use crate::input::{Action, Binding, MouseBinding, KeyBinding};
use crate::index::{Line, Column};
use crate::ansi::{CursorStyle, NamedColor, Color};

const MAX_SCROLLBACK_LINES: u32 = 100_000;

/// Function that returns true for serde default
fn true_bool() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
pub struct Selection {
    pub semantic_escape_chars: String,
    #[serde(default, deserialize_with = "failure_default")]
    pub save_to_clipboard: bool,
}

impl Default for Selection {
    fn default() -> Selection {
        Selection {
            semantic_escape_chars: String::new(),
            save_to_clipboard: false
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ClickHandler {
    #[serde(deserialize_with="deserialize_duration_ms")]
    pub threshold: Duration,
}

impl Default for ClickHandler {
    fn default() -> Self {
        ClickHandler { threshold: default_threshold_ms() }
    }
}

fn default_threshold_ms() -> Duration {
    Duration::from_millis(300)
}

fn deserialize_duration_ms<'a, D>(deserializer: D) -> ::std::result::Result<Duration, D::Error>
    where D: de::Deserializer<'a>
{
    match u64::deserialize(deserializer) {
        Ok(threshold_ms) => Ok(Duration::from_millis(threshold_ms)),
        Err(err) => {
            error!("problem with config: {}; Using default value", err);
            Ok(default_threshold_ms())
        },
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Mouse {
    #[serde(default, deserialize_with = "failure_default")]
    pub double_click: ClickHandler,
    #[serde(default, deserialize_with = "failure_default")]
    pub triple_click: ClickHandler,
    #[serde(default, deserialize_with = "failure_default")]
    pub hide_when_typing: bool,
    #[serde(default, deserialize_with = "failure_default")]
    pub url: Url,

    // TODO: DEPRECATED
    #[serde(default)]
    pub faux_scrollback_lines: Option<usize>,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct Url {
    // Program for opening links
    #[serde(default, deserialize_with = "failure_default")]
    pub launcher: Option<CommandWrapper>,

    // Modifier used to open links
    #[serde(default, deserialize_with = "deserialize_modifiers")]
    pub modifiers: ModifiersState,
}

fn deserialize_modifiers<'a, D>(deserializer: D) -> ::std::result::Result<ModifiersState, D::Error>
    where D: de::Deserializer<'a>
{
    ModsWrapper::deserialize(deserializer).map(|wrapper| wrapper.into_inner())
}

impl Default for Mouse {
    fn default() -> Mouse {
        Mouse {
            double_click: ClickHandler {
                threshold: Duration::from_millis(300),
            },
            triple_click: ClickHandler {
                threshold: Duration::from_millis(300),
            },
            hide_when_typing: false,
            url: Url::default(),
            faux_scrollback_lines: None,
        }
    }
}

/// `VisualBellAnimations` are modeled after a subset of CSS transitions and Robert
/// Penner's Easing Functions.
#[derive(Clone, Copy, Debug, Deserialize)]
pub enum VisualBellAnimation {
    Ease,          // CSS
    EaseOut,       // CSS
    EaseOutSine,   // Penner
    EaseOutQuad,   // Penner
    EaseOutCubic,  // Penner
    EaseOutQuart,  // Penner
    EaseOutQuint,  // Penner
    EaseOutExpo,   // Penner
    EaseOutCirc,   // Penner
    Linear,
}

impl Default for VisualBellAnimation {
    fn default() -> Self {
        VisualBellAnimation::EaseOutExpo
    }
}

#[derive(Debug, Deserialize)]
pub struct VisualBellConfig {
    /// Visual bell animation function
    #[serde(default, deserialize_with = "failure_default")]
    animation: VisualBellAnimation,

    /// Visual bell duration in milliseconds
    #[serde(default, deserialize_with = "deserialize_visual_bell_duration")]
    duration: u16,

    /// Visual bell flash color
    #[serde(default="default_visual_bell_color", deserialize_with = "rgb_from_hex")]
    color: Rgb,
}

fn deserialize_visual_bell_duration<'a, D>(deserializer: D) -> ::std::result::Result<u16, D::Error>
    where D: de::Deserializer<'a>
{
    match u16::deserialize(deserializer) {
        Ok(duration) => Ok(duration),
        Err(err) => {
            error!("problem with config: {}; Using default value", err);
            Ok(0)
        },
    }
}

fn default_visual_bell_color() -> Rgb {
    Rgb { r: 255, g: 255, b: 255 }
}

impl VisualBellConfig {
    /// Visual bell animation
    #[inline]
    pub fn animation(&self) -> VisualBellAnimation {
        self.animation
    }

    /// Visual bell duration in milliseconds
    #[inline]
    pub fn duration(&self) -> Duration {
        Duration::from_millis(u64::from(self.duration))
    }

    /// Visual bell flash color
    #[inline]
    pub fn color(&self) -> Rgb {
        self.color
    }
}

impl Default for VisualBellConfig {
    fn default() -> VisualBellConfig {
        VisualBellConfig {
            animation: VisualBellAnimation::default(),
            color: default_visual_bell_color(),
            duration: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Shell<'a> {
    program: Cow<'a, str>,

    #[serde(default, deserialize_with = "failure_default")]
    args: Vec<String>,
}

impl<'a> Shell<'a> {
    pub fn new<S>(program: S) -> Shell<'a>
        where S: Into<Cow<'a, str>>
    {
        Shell {
            program: program.into(),
            args: Vec::new(),
        }
    }

    pub fn new_with_args<S>(program: S, args: Vec<String>) -> Shell<'a>
        where S: Into<Cow<'a, str>>
    {
        Shell {
            program: program.into(),
            args,
        }
    }

    pub fn program(&self) -> &str {
        &*self.program
    }

    pub fn args(&self) -> &[String] {
        self.args.as_slice()
    }
}

/// Wrapper around f32 that represents an alpha value between 0.0 and 1.0
#[derive(Clone, Copy, Debug)]
pub struct Alpha(f32);

impl Alpha {
    pub fn new(value: f32) -> Self {
        Alpha(Self::clamp_to_valid_range(value))
    }

    pub fn set(&mut self, value: f32) {
        self.0 = Self::clamp_to_valid_range(value);
    }

    #[inline]
    pub fn get(self) -> f32 {
        self.0
    }

    fn clamp_to_valid_range(value: f32) -> f32 {
        if value < 0.0 {
            0.0
        } else if value > 1.0 {
            1.0
        } else {
            value
        }
    }
}

impl Default for Alpha {
    fn default() -> Self {
        Alpha(1.0)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Decorations {
    Full,
    Transparent,
    Buttonless,
    None,
}

impl Default for Decorations {
    fn default() -> Decorations {
        Decorations::Full
    }
}

impl<'de> Deserialize<'de> for Decorations {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Decorations, D::Error>
        where D: de::Deserializer<'de>
    {

        struct DecorationsVisitor;

        impl<'de> Visitor<'de> for DecorationsVisitor {
            type Value = Decorations;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Some subset of full|transparent|buttonless|none")
            }

            #[cfg(target_os = "macos")]
            fn visit_str<E>(self, value: &str) -> ::std::result::Result<Decorations, E>
                where E: de::Error
            {
                match value.to_lowercase().as_str() {
                    "transparent" => Ok(Decorations::Transparent),
                    "buttonless" => Ok(Decorations::Buttonless),
                    "none" => Ok(Decorations::None),
                    "full" => Ok(Decorations::Full),
                    "true" => {
                        error!("deprecated decorations boolean value, \
                                   use one of transparent|buttonless|none|full instead; \
                                   Falling back to \"full\"");
                        Ok(Decorations::Full)
                    },
                    "false" => {
                        error!("deprecated decorations boolean value, \
                                   use one of transparent|buttonless|none|full instead; \
                                   Falling back to \"none\"");
                        Ok(Decorations::None)
                    },
                    _ => {
                        error!("invalid decorations value: {}; Using default value", value);
                        Ok(Decorations::Full)
                    }
                }
            }

            #[cfg(not(target_os = "macos"))]
            fn visit_str<E>(self, value: &str) -> ::std::result::Result<Decorations, E>
                where E: de::Error
            {
                match value.to_lowercase().as_str() {
                    "none" => Ok(Decorations::None),
                    "full" => Ok(Decorations::Full),
                    "true" => {
                        error!("deprecated decorations boolean value, \
                                   use one of none|full instead; \
                                   Falling back to \"full\"");
                        Ok(Decorations::Full)
                    },
                    "false" => {
                        error!("deprecated decorations boolean value, \
                                   use one of none|full instead; \
                                   Falling back to \"none\"");
                        Ok(Decorations::None)
                    },
                    "transparent" | "buttonless" => {
                        error!("macos-only decorations value: {}; Using default value", value);
                        Ok(Decorations::Full)
                    },
                    _ => {
                        error!("invalid decorations value: {}; Using default value", value);
                        Ok(Decorations::Full)
                    }
                }
            }
        }

        deserializer.deserialize_str(DecorationsVisitor)
    }
}

#[derive(Debug, Copy, Clone, Deserialize)]
pub struct WindowConfig {
    /// Initial dimensions
    #[serde(default, deserialize_with = "failure_default")]
    dimensions: Dimensions,

    /// Pixel padding
    #[serde(default="default_padding", deserialize_with = "deserialize_padding")]
    padding: Delta<u8>,

    /// Draw the window with title bar / borders
    #[serde(default)]
    decorations: Decorations,

    /// Spread out additional padding evenly
    #[serde(default, deserialize_with = "failure_default")]
    dynamic_padding: bool,

    /// Start maximized
    #[serde(default, deserialize_with = "failure_default")]
    start_maximized: bool,
}

fn default_padding() -> Delta<u8> {
    Delta { x: 2, y: 2 }
}

fn deserialize_padding<'a, D>(deserializer: D) -> ::std::result::Result<Delta<u8>, D::Error>
    where D: de::Deserializer<'a>
{
    match Delta::deserialize(deserializer) {
        Ok(delta) => Ok(delta),
        Err(err) => {
            error!("problem with config: {}; Using default value", err);
            Ok(default_padding())
        },
    }
}

impl WindowConfig {
    pub fn decorations(&self) -> Decorations {
        self.decorations
    }

    pub fn dynamic_padding(&self) -> bool {
        self.dynamic_padding
    }

    pub fn start_maximized(&self) -> bool {
        self.start_maximized
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        WindowConfig{
            dimensions: Default::default(),
            padding: default_padding(),
            decorations: Default::default(),
            dynamic_padding: false,
            start_maximized: false,
        }
    }
}

/// Top-level config type
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Initial dimensions
    #[serde(default, deserialize_with = "failure_default")]
    dimensions: Option<Dimensions>,

    /// Pixel padding
    #[serde(default, deserialize_with = "failure_default")]
    padding: Option<Delta<u8>>,

    /// TERM env variable
    #[serde(default, deserialize_with = "failure_default")]
    env: HashMap<String, String>,

    /// Font configuration
    #[serde(default, deserialize_with = "failure_default")]
    font: Font,

    /// Should show render timer
    #[serde(default, deserialize_with = "failure_default")]
    render_timer: bool,

    /// Should draw bold text with brighter colors instead of bold font
    #[serde(default="true_bool", deserialize_with = "default_true_bool")]
    draw_bold_text_with_bright_colors: bool,

    #[serde(default, deserialize_with = "failure_default")]
    colors: Colors,

    /// Background opacity from 0.0 to 1.0
    #[serde(default, deserialize_with = "failure_default")]
    background_opacity: Alpha,

    /// Window configuration
    #[serde(default, deserialize_with = "failure_default")]
    window: WindowConfig,

    /// Keybindings
    #[serde(default, deserialize_with = "failure_default_vec")]
    key_bindings: Vec<KeyBinding>,

    /// Bindings for the mouse
    #[serde(default, deserialize_with = "failure_default_vec")]
    mouse_bindings: Vec<MouseBinding>,

    #[serde(default, deserialize_with = "failure_default")]
    selection: Selection,

    #[serde(default, deserialize_with = "failure_default")]
    mouse: Mouse,

    /// Path to a shell program to run on startup
    #[serde(default, deserialize_with = "failure_default")]
    shell: Option<Shell<'static>>,

    /// Path where config was loaded from
    #[serde(default, deserialize_with = "failure_default")]
    config_path: Option<PathBuf>,

    /// Visual bell configuration
    #[serde(default, deserialize_with = "failure_default")]
    visual_bell: VisualBellConfig,

    /// Use dynamic title
    #[serde(default="true_bool", deserialize_with = "default_true_bool")]
    dynamic_title: bool,

    /// Live config reload
    #[serde(default="true_bool", deserialize_with = "default_true_bool")]
    live_config_reload: bool,

    /// Number of spaces in one tab
    #[serde(default="default_tabspaces", deserialize_with = "deserialize_tabspaces")]
    tabspaces: usize,

    /// How much scrolling history to keep
    #[serde(default, deserialize_with="failure_default")]
    scrolling: Scrolling,

    /// Cursor configuration
    #[serde(default, deserialize_with="failure_default")]
    cursor: Cursor,

    /// Keep the log file after quitting
    #[serde(default, deserialize_with="failure_default")]
    persistent_logging: bool,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    custom_cursor_colors: Option<bool>,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    hide_cursor_when_typing: Option<bool>,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    cursor_style: Option<CursorStyle>,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    unfocused_hollow_cursor: Option<bool>,
}

fn failure_default_vec<'a, D, T>(deserializer: D) -> ::std::result::Result<Vec<T>, D::Error>
    where D: de::Deserializer<'a>,
          T: Deserialize<'a>
{
    // Deserialize as generic vector
    let vec = match Vec::<serde_yaml::Value>::deserialize(deserializer) {
        Ok(vec) => vec,
        Err(err) => {
            error!("problem with config: {}; Using empty vector", err);
            return Ok(Vec::new());
        },
    };

    // Move to lossy vector
    let mut bindings: Vec<T> = Vec::new();
    for value in vec {
        match T::deserialize(value) {
            Ok(binding) => bindings.push(binding),
            Err(err) => {
                error!("problem with config: {}; Skipping value", err);
            },
        }
    }

    Ok(bindings)
}

fn default_tabspaces() -> usize {
    8
}

fn deserialize_tabspaces<'a, D>(deserializer: D) -> ::std::result::Result<usize, D::Error>
    where D: de::Deserializer<'a>
{
    match usize::deserialize(deserializer) {
        Ok(value) => Ok(value),
        Err(err) => {
            error!("problem with config: {}; Using `8`", err);
            Ok(default_tabspaces())
        },
    }
}

fn default_true_bool<'a, D>(deserializer: D) -> ::std::result::Result<bool, D::Error>
    where D: de::Deserializer<'a>
{
    match bool::deserialize(deserializer) {
        Ok(value) => Ok(value),
        Err(err) => {
            error!("problem with config: {}; Using `true`", err);
            Ok(true)
        },
    }
}

fn failure_default<'a, D, T>(deserializer: D)
    -> ::std::result::Result<T, D::Error>
    where D: de::Deserializer<'a>,
          T: Deserialize<'a> + Default
{
    match T::deserialize(deserializer) {
        Ok(value) => Ok(value),
        Err(err) => {
            error!("problem with config: {}; Using default value", err);
            Ok(T::default())
        },
    }
}

#[cfg(not(any(windows, target_os="macos")))]
static DEFAULT_ALACRITTY_CONFIG: &'static str = include_str!("../alacritty.yml");
#[cfg(target_os="macos")]
static DEFAULT_ALACRITTY_CONFIG: &'static str = include_str!("../alacritty_macos.yml");
#[cfg(windows)]
static DEFAULT_ALACRITTY_CONFIG: &'static str = include_str!("../alacritty_windows.yml");

impl Default for Config {
    fn default() -> Self {
        serde_yaml::from_str(DEFAULT_ALACRITTY_CONFIG)
            .expect("default config is invalid")
    }
}

/// Struct for scrolling related settings
#[derive(Copy, Clone, Debug, Deserialize)]
pub struct Scrolling {
    #[serde(deserialize_with="deserialize_scrolling_history")]
    #[serde(default="default_scrolling_history")]
    pub history: u32,
    #[serde(deserialize_with="deserialize_scrolling_multiplier")]
    #[serde(default="default_scrolling_multiplier")]
    pub multiplier: u8,
    #[serde(deserialize_with="deserialize_scrolling_multiplier")]
    #[serde(default="default_scrolling_multiplier")]
    pub faux_multiplier: u8,
    #[serde(default, deserialize_with="failure_default")]
    pub auto_scroll: bool,
}

fn default_scrolling_history() -> u32 {
    10_000
}

// Default for normal and faux scrolling
fn default_scrolling_multiplier() -> u8 {
    3
}

impl Default for Scrolling {
    fn default() -> Self {
        Self {
            history: default_scrolling_history(),
            multiplier: default_scrolling_multiplier(),
            faux_multiplier: default_scrolling_multiplier(),
            auto_scroll: false,
        }
    }
}

fn deserialize_scrolling_history<'a, D>(deserializer: D) -> ::std::result::Result<u32, D::Error>
    where D: de::Deserializer<'a>
{
    match u32::deserialize(deserializer) {
        Ok(lines) => {
            if lines > MAX_SCROLLBACK_LINES {
                error!(
                    "problem with config: scrollback size is {}, but expected a maximum of {}; \
                     Using {1} instead",
                    lines, MAX_SCROLLBACK_LINES,
                );
                Ok(MAX_SCROLLBACK_LINES)
            } else {
                Ok(lines)
            }
        },
        Err(err) => {
            error!("problem with config: {}; Using default value", err);
            Ok(default_scrolling_history())
        },
    }
}

fn deserialize_scrolling_multiplier<'a, D>(deserializer: D) -> ::std::result::Result<u8, D::Error>
    where D: de::Deserializer<'a>
{
    match u8::deserialize(deserializer) {
        Ok(lines) => Ok(lines),
        Err(err) => {
            error!("problem with config: {}; Using default value", err);
            Ok(default_scrolling_multiplier())
        },
    }
}

/// Newtype for implementing deserialize on glutin Mods
///
/// Our deserialize impl wouldn't be covered by a derive(Deserialize); see the
/// impl below.
#[derive(Debug, Copy, Clone, Hash, Default, Eq, PartialEq)]
struct ModsWrapper(ModifiersState);

impl ModsWrapper {
    fn into_inner(self) -> ModifiersState {
        self.0
    }
}

impl<'a> de::Deserialize<'a> for ModsWrapper {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer<'a>
    {
        struct ModsVisitor;

        impl<'a> Visitor<'a> for ModsVisitor {
            type Value = ModsWrapper;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Some subset of Command|Shift|Super|Alt|Option|Control")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<ModsWrapper, E>
                where E: de::Error,
            {
                let mut res = ModifiersState::default();
                for modifier in value.split('|') {
                    match modifier.trim() {
                        "Command" | "Super" => res.logo = true,
                        "Shift" => res.shift = true,
                        "Alt" | "Option" => res.alt = true,
                        "Control" => res.ctrl = true,
                        _ => error!("unknown modifier {:?}", modifier),
                    }
                }

                Ok(ModsWrapper(res))
            }
        }

        deserializer.deserialize_str(ModsVisitor)
    }
}

struct ActionWrapper(crate::input::Action);

impl ActionWrapper {
    fn into_inner(self) -> crate::input::Action {
        self.0
    }
}

impl<'a> de::Deserialize<'a> for ActionWrapper {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer<'a>
    {
        struct ActionVisitor;

        impl<'a> Visitor<'a> for ActionVisitor {
            type Value = ActionWrapper;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Paste, Copy, PasteSelection, IncreaseFontSize, DecreaseFontSize, \
                            ResetFontSize, ScrollPageUp, ScrollPageDown, ScrollToTop, \
                            ScrollToBottom, ClearHistory, Hide, ClearLogNotice or Quit")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<ActionWrapper, E>
                where E: de::Error,
            {
                Ok(ActionWrapper(match value {
                    "Paste" => Action::Paste,
                    "Copy" => Action::Copy,
                    "PasteSelection" => Action::PasteSelection,
                    "IncreaseFontSize" => Action::IncreaseFontSize,
                    "DecreaseFontSize" => Action::DecreaseFontSize,
                    "ResetFontSize" => Action::ResetFontSize,
                    "ScrollPageUp" => Action::ScrollPageUp,
                    "ScrollPageDown" => Action::ScrollPageDown,
                    "ScrollToTop" => Action::ScrollToTop,
                    "ScrollToBottom" => Action::ScrollToBottom,
                    "ClearHistory" => Action::ClearHistory,
                    "Hide" => Action::Hide,
                    "Quit" => Action::Quit,
                    "ClearLogNotice" => Action::ClearLogNotice,
                    _ => return Err(E::invalid_value(Unexpected::Str(value), &self)),
                }))
            }
        }
        deserializer.deserialize_str(ActionVisitor)
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum CommandWrapper {
    Just(String),
    WithArgs {
        program: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl CommandWrapper {
    pub fn program(&self) -> &str {
        match self {
            CommandWrapper::Just(program) => program,
            CommandWrapper::WithArgs { program, .. } => program,
        }
    }

    pub fn args(&self) -> &[String] {
        match self {
            CommandWrapper::Just(_) => &[],
            CommandWrapper::WithArgs { args, .. } => args,
        }
    }
}

use crate::term::{mode, TermMode};

struct ModeWrapper {
    pub mode: TermMode,
    pub not_mode: TermMode,
}

impl<'a> de::Deserialize<'a> for ModeWrapper {
    fn deserialize<D>(deserializer:  D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer<'a>
    {
        struct ModeVisitor;

        impl<'a> Visitor<'a> for ModeVisitor {
            type Value = ModeWrapper;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Combination of AppCursor | AppKeypad, possibly with negation (~)")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<ModeWrapper, E>
                where E: de::Error,
            {
                let mut res = ModeWrapper {
                    mode: TermMode::empty(),
                    not_mode: TermMode::empty()
                };

                for modifier in value.split('|') {
                    match modifier.trim() {
                        "AppCursor" => res.mode |= mode::TermMode::APP_CURSOR,
                        "~AppCursor" => res.not_mode |= mode::TermMode::APP_CURSOR,
                        "AppKeypad" => res.mode |= mode::TermMode::APP_KEYPAD,
                        "~AppKeypad" => res.not_mode |= mode::TermMode::APP_KEYPAD,
                        _ => error!("unknown mode {:?}", modifier),
                    }
                }

                Ok(res)
            }
        }
        deserializer.deserialize_str(ModeVisitor)
    }
}

struct MouseButton(::glutin::MouseButton);

impl MouseButton {
    fn into_inner(self) -> ::glutin::MouseButton {
        self.0
    }
}

impl<'a> de::Deserialize<'a> for MouseButton {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer<'a>
    {
        struct MouseButtonVisitor;

        impl<'a> Visitor<'a> for MouseButtonVisitor {
            type Value = MouseButton;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Left, Right, Middle, or a number")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<MouseButton, E>
                where E: de::Error,
            {
                match value {
                    "Left" => Ok(MouseButton(::glutin::MouseButton::Left)),
                    "Right" => Ok(MouseButton(::glutin::MouseButton::Right)),
                    "Middle" => Ok(MouseButton(::glutin::MouseButton::Middle)),
                    _ => {
                        if let Ok(index) = u8::from_str(value) {
                            Ok(MouseButton(::glutin::MouseButton::Other(index)))
                        } else {
                            Err(E::invalid_value(Unexpected::Str(value), &self))
                        }
                    }
                }
            }
        }

        deserializer.deserialize_str(MouseButtonVisitor)
    }
}

/// Bindings are deserialized into a `RawBinding` before being parsed as a
/// `KeyBinding` or `MouseBinding`.
struct RawBinding {
    key: Option<Key>,
    mouse: Option<::glutin::MouseButton>,
    mods: ModifiersState,
    mode: TermMode,
    notmode: TermMode,
    action: Action,
}

impl RawBinding {
    fn into_mouse_binding(self) -> ::std::result::Result<MouseBinding, Self> {
        if let Some(mouse) = self.mouse {
            Ok(Binding {
                trigger: mouse,
                mods: self.mods,
                action: self.action,
                mode: self.mode,
                notmode: self.notmode,
            })
        } else {
            Err(self)
        }
    }

    fn into_key_binding(self) -> ::std::result::Result<KeyBinding, Self> {
        if let Some(key) = self.key {
            Ok(KeyBinding {
                trigger: key,
                mods: self.mods,
                action: self.action,
                mode: self.mode,
                notmode: self.notmode,
            })
        } else {
            Err(self)
        }
    }
}

impl<'a> de::Deserialize<'a> for RawBinding {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer<'a>
    {
        enum Field {
            Key,
            Mods,
            Mode,
            Action,
            Chars,
            Mouse,
            Command,
        }

        impl<'a> de::Deserialize<'a> for Field {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Field, D::Error>
                where D: de::Deserializer<'a>
            {
                struct FieldVisitor;

                static FIELDS: &'static [&'static str] = &[
                        "key", "mods", "mode", "action", "chars", "mouse", "command",
                ];

                impl<'a> Visitor<'a> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("binding fields")
                    }

                    fn visit_str<E>(self, value: &str) -> ::std::result::Result<Field, E>
                        where E: de::Error,
                    {
                        match value {
                            "key" => Ok(Field::Key),
                            "mods" => Ok(Field::Mods),
                            "mode" => Ok(Field::Mode),
                            "action" => Ok(Field::Action),
                            "chars" => Ok(Field::Chars),
                            "mouse" => Ok(Field::Mouse),
                            "command" => Ok(Field::Command),
                            _ => Err(E::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_str(FieldVisitor)
            }
        }

        struct RawBindingVisitor;
        impl<'a> Visitor<'a> for RawBindingVisitor {
            type Value = RawBinding;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("binding specification")
            }

            fn visit_map<V>(
                self,
                mut map: V
            ) -> ::std::result::Result<RawBinding, V::Error>
                where V: MapAccess<'a>,
            {
                let mut mods: Option<ModifiersState> = None;
                let mut key: Option<Key> = None;
                let mut chars: Option<String> = None;
                let mut action: Option<crate::input::Action> = None;
                let mut mode: Option<TermMode> = None;
                let mut not_mode: Option<TermMode> = None;
                let mut mouse: Option<::glutin::MouseButton> = None;
                let mut command: Option<CommandWrapper> = None;

                use ::serde::de::Error;

                while let Some(struct_key) = map.next_key::<Field>()? {
                    match struct_key {
                        Field::Key => {
                            if key.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("key"));
                            }

                            let val = map.next_value::<serde_yaml::Value>()?;
                            if val.is_u64() {
                                let scancode = val.as_u64().unwrap();
                                if scancode > u64::from(::std::u32::MAX) {
                                    return Err(<V::Error as Error>::custom(format!(
                                        "invalid key binding, scancode too big: {}",
                                        scancode
                                    )));
                                }
                                key = Some(Key::Scancode(scancode as u32));
                            } else {
                                let k = Key::deserialize(val)
                                    .map_err(V::Error::custom)?;
                                key = Some(k);
                            }
                        },
                        Field::Mods => {
                            if mods.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("mods"));
                            }

                            mods = Some(map.next_value::<ModsWrapper>()?.into_inner());
                        },
                        Field::Mode => {
                            if mode.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("mode"));
                            }

                            let mode_deserializer = map.next_value::<ModeWrapper>()?;
                            mode = Some(mode_deserializer.mode);
                            not_mode = Some(mode_deserializer.not_mode);
                        },
                        Field::Action => {
                            if action.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("action"));
                            }

                            action = Some(map.next_value::<ActionWrapper>()?.into_inner());
                        },
                        Field::Chars => {
                            if chars.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("chars"));
                            }

                            chars = Some(map.next_value()?);
                        },
                        Field::Mouse => {
                            if chars.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("mouse"));
                            }

                            mouse = Some(map.next_value::<MouseButton>()?.into_inner());
                        },
                        Field::Command => {
                            if command.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("command"));
                            }

                            command = Some(map.next_value::<CommandWrapper>()?);
                        },
                    }
                }

                let action = match (action, chars, command) {
                    (Some(action), None, None) => action,
                    (None, Some(chars), None) => Action::Esc(chars),
                    (None, None, Some(cmd)) => {
                        match cmd {
                            CommandWrapper::Just(program) => {
                                Action::Command(program, vec![])
                            },
                            CommandWrapper::WithArgs { program, args } => {
                                Action::Command(program, args)
                            },
                        }
                    },
                    (None, None, None) => return Err(V::Error::custom("must specify chars, action or command")),
                    _ => return Err(V::Error::custom("must specify only chars, action or command")),
                };

                let mode = mode.unwrap_or_else(TermMode::empty);
                let not_mode = not_mode.unwrap_or_else(TermMode::empty);
                let mods = mods.unwrap_or_else(ModifiersState::default);

                if mouse.is_none() && key.is_none() {
                    return Err(V::Error::custom("bindings require mouse button or key"));
                }

                Ok(RawBinding {
                    mode,
                    notmode: not_mode,
                    action,
                    key,
                    mouse,
                    mods,
                })
            }
        }

        const FIELDS: &[&str] = &[
            "key", "mods", "mode", "action", "chars", "mouse", "command",
        ];

        deserializer.deserialize_struct("RawBinding", FIELDS, RawBindingVisitor)
    }
}


impl<'a> de::Deserialize<'a> for Alpha {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer<'a>
    {
        let value = f32::deserialize(deserializer)?;
        Ok(Alpha::new(value))
    }
}

impl<'a> de::Deserialize<'a> for MouseBinding {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer<'a>
    {
        let raw = RawBinding::deserialize(deserializer)?;
        raw.into_mouse_binding()
           .map_err(|_| D::Error::custom("expected mouse binding"))
    }
}

impl<'a> de::Deserialize<'a> for KeyBinding {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer<'a>
    {
        let raw = RawBinding::deserialize(deserializer)?;
        raw.into_key_binding()
           .map_err(|_| D::Error::custom("expected key binding"))
    }
}

/// Errors occurring during config loading
#[derive(Debug)]
pub enum Error {
    /// Config file not found
    NotFound,

    /// Config file empty
    Empty,

    /// Couldn't read $HOME environment variable
    ReadingEnvHome(env::VarError),

    /// io error reading file
    Io(io::Error),

    /// Not valid yaml or missing parameters
    Yaml(serde_yaml::Error),
}

#[derive(Debug, Deserialize)]
pub struct Colors {
    #[serde(default, deserialize_with = "failure_default")]
    pub primary: PrimaryColors,
    #[serde(default, deserialize_with = "failure_default")]
    pub cursor: CursorColors,
    pub normal: AnsiColors,
    pub bright: AnsiColors,
    #[serde(default, deserialize_with = "failure_default")]
    pub dim: Option<AnsiColors>,
    #[serde(default, deserialize_with = "failure_default_vec")]
    pub indexed_colors: Vec<IndexedColor>,
}

#[derive(Debug, Deserialize)]
pub struct IndexedColor {
    #[serde(deserialize_with = "deserialize_color_index")]
    pub index: u8,
    #[serde(deserialize_with = "rgb_from_hex")]
    pub color: Rgb,
}

fn deserialize_color_index<'a, D>(deserializer: D) -> ::std::result::Result<u8, D::Error>
    where D: de::Deserializer<'a>
{
    match u8::deserialize(deserializer) {
        Ok(index) => {
            if index < 16 {
                error!(
                    "problem with config: indexed_color's index is '{}', \
                     but a value bigger than 15 was expected; \
                     Ignoring setting",
                    index
                );

                // Return value out of range to ignore this color
                Ok(0)
            } else {
                Ok(index)
            }
        },
        Err(err) => {
            error!("problem with config: {}; Ignoring setting", err);

            // Return value out of range to ignore this color
            Ok(0)
        },
    }
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub struct Cursor {
    #[serde(default, deserialize_with = "failure_default")]
    pub style: CursorStyle,
    #[serde(default="true_bool", deserialize_with = "default_true_bool")]
    pub unfocused_hollow: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            style: Default::default(),
            unfocused_hollow: true,
        }
    }
}

#[derive(Debug, Copy, Clone, Default, Deserialize)]
pub struct CursorColors {
    #[serde(default, deserialize_with = "deserialize_optional_color")]
    pub text: Option<Rgb>,
    #[serde(default, deserialize_with = "deserialize_optional_color")]
    pub cursor: Option<Rgb>,
}

#[derive(Debug, Deserialize)]
pub struct PrimaryColors {
    #[serde(deserialize_with = "rgb_from_hex")]
    pub background: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    pub foreground: Rgb,
    #[serde(default, deserialize_with = "deserialize_optional_color")]
    pub bright_foreground: Option<Rgb>,
    #[serde(default, deserialize_with = "deserialize_optional_color")]
    pub dim_foreground: Option<Rgb>,
}

fn deserialize_optional_color<'a, D>(deserializer: D) -> ::std::result::Result<Option<Rgb>, D::Error>
    where D: de::Deserializer<'a>
{
    match Option::deserialize(deserializer) {
        Ok(Some(color)) => {
            let color: serde_yaml::Value = color;
            Ok(Some(rgb_from_hex(color).unwrap()))
        },
        Ok(None) => Ok(None),
        Err(err) => {
            error!("problem with config: {}; Using standard foreground color", err);
            Ok(None)
        },
    }
}

impl Default for PrimaryColors {
    fn default() -> Self {
        PrimaryColors {
            background: Rgb { r: 0, g: 0, b: 0 },
            foreground: Rgb { r: 0xea, g: 0xea, b: 0xea },
            bright_foreground: None,
            dim_foreground: None,
        }
    }
}

impl Default for Colors {
    fn default() -> Colors {
        Colors {
            primary: PrimaryColors::default(),
            cursor: CursorColors::default(),
            normal: AnsiColors {
                black: Rgb {r: 0x00, g: 0x00, b: 0x00},
                red: Rgb {r: 0xd5, g: 0x4e, b: 0x53},
                green: Rgb {r: 0xb9, g: 0xca, b: 0x4a},
                yellow: Rgb {r: 0xe6, g: 0xc5, b: 0x47},
                blue: Rgb {r: 0x7a, g: 0xa6, b: 0xda},
                magenta: Rgb {r: 0xc3, g: 0x97, b: 0xd8},
                cyan: Rgb {r: 0x70, g: 0xc0, b: 0xba},
                white: Rgb {r: 0xea, g: 0xea, b: 0xea},
            },
            bright: AnsiColors {
                black: Rgb {r: 0x66, g: 0x66, b: 0x66},
                red: Rgb {r: 0xff, g: 0x33, b: 0x34},
                green: Rgb {r: 0x9e, g: 0xc4, b: 0x00},
                yellow: Rgb {r: 0xe7, g: 0xc5, b: 0x47},
                blue: Rgb {r: 0x7a, g: 0xa6, b: 0xda},
                magenta: Rgb {r: 0xb7, g: 0x7e, b: 0xe0},
                cyan: Rgb {r: 0x54, g: 0xce, b: 0xd6},
                white: Rgb {r: 0xff, g: 0xff, b: 0xff},
            },
            dim: None,
            indexed_colors: Vec::new(),
        }
    }
}

/// The 8-colors sections of config
#[derive(Debug, Deserialize)]
pub struct AnsiColors {
    #[serde(deserialize_with = "rgb_from_hex")]
    pub black: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    pub red: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    pub green: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    pub yellow: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    pub blue: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    pub magenta: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    pub cyan: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    pub white: Rgb,
}

/// Deserialize an Rgb from a hex string
///
/// This is *not* the deserialize impl for Rgb since we want a symmetric
/// serialize/deserialize impl for ref tests.
fn rgb_from_hex<'a, D>(deserializer: D) -> ::std::result::Result<Rgb, D::Error>
    where D: de::Deserializer<'a>
{
    struct RgbVisitor;

    impl<'a> Visitor<'a> for RgbVisitor {
        type Value = Rgb;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("Hex colors spec like 'ffaabb'")
        }

        fn visit_str<E>(self, value: &str) -> ::std::result::Result<Rgb, E>
            where E: ::serde::de::Error
        {
            Rgb::from_str(&value[..])
                .map_err(|_| E::custom("failed to parse rgb; expect 0xrrggbb"))
        }
    }

    let rgb = deserializer.deserialize_str(RgbVisitor);

    // Use #ff00ff as fallback color
    match rgb {
        Ok(rgb) => Ok(rgb),
        Err(err) => {
            error!("problem with config: {}; Using color #ff00ff", err);
            Ok(Rgb { r: 255, g: 0, b: 255 })
        },
    }
}

impl FromStr for Rgb {
    type Err = ();
    fn from_str(s: &str) -> ::std::result::Result<Rgb, ()> {
        let mut chars = s.chars();
        let mut rgb = Rgb::default();

        macro_rules! component {
            ($($c:ident),*) => {
                $(
                    match chars.next().and_then(|c| c.to_digit(16)) {
                        Some(val) => rgb.$c = (val as u8) << 4,
                        None => return Err(())
                    }

                    match chars.next().and_then(|c| c.to_digit(16)) {
                        Some(val) => rgb.$c |= val as u8,
                        None => return Err(())
                    }
                )*
            }
        }

        match chars.next() {
            Some('0') => if chars.next() != Some('x') { return Err(()); },
            Some('#') => (),
            _ => return Err(()),
        }

        component!(r, g, b);

        Ok(rgb)
    }
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&dyn (::std::error::Error)> {
        match *self {
            Error::NotFound | Error::Empty => None,
            Error::ReadingEnvHome(ref err) => Some(err),
            Error::Io(ref err) => Some(err),
            Error::Yaml(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::NotFound => "could not locate config file",
            Error::Empty => "empty config file",
            Error::ReadingEnvHome(ref err) => err.description(),
            Error::Io(ref err) => err.description(),
            Error::Yaml(ref err) => err.description(),
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Error::NotFound | Error::Empty => write!(f, "{}", ::std::error::Error::description(self)),
            Error::ReadingEnvHome(ref err) => {
                write!(f, "could not read $HOME environment variable: {}", err)
            },
            Error::Io(ref err) => write!(f, "error reading config file: {}", err),
            Error::Yaml(ref err) => write!(f, "problem with config: {}", err),
        }
    }
}

impl From<env::VarError> for Error {
    fn from(val: env::VarError) -> Error {
        Error::ReadingEnvHome(val)
    }
}

impl From<io::Error> for Error {
    fn from(val: io::Error) -> Error {
        if val.kind() == io::ErrorKind::NotFound {
            Error::NotFound
        } else {
            Error::Io(val)
        }
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(val: serde_yaml::Error) -> Error {
        Error::Yaml(val)
    }
}

/// Result from config loading
pub type Result<T> = ::std::result::Result<T, Error>;

impl Config {
    /// Get the location of the first found default config file paths
    /// according to the following order:
    ///
    /// 1. $XDG_CONFIG_HOME/alacritty/alacritty.yml
    /// 2. $XDG_CONFIG_HOME/alacritty.yml
    /// 3. $HOME/.config/alacritty/alacritty.yml
    /// 4. $HOME/.alacritty.yml
    #[cfg(not(windows))]
    pub fn installed_config<'a>() -> Option<Cow<'a, Path>> {
        // Try using XDG location by default
        ::xdg::BaseDirectories::with_prefix("alacritty")
            .ok()
            .and_then(|xdg| xdg.find_config_file("alacritty.yml"))
            .or_else(|| {
                ::xdg::BaseDirectories::new().ok().and_then(|fallback| {
                    fallback.find_config_file("alacritty.yml")
                })
            })
            .or_else(|| {
                if let Ok(home) = env::var("HOME") {
                    // Fallback path: $HOME/.config/alacritty/alacritty.yml
                    let fallback = PathBuf::from(&home).join(".config/alacritty/alacritty.yml");
                    if fallback.exists() {
                        return Some(fallback);
                    }
                    // Fallback path: $HOME/.alacritty.yml
                    let fallback = PathBuf::from(&home).join(".alacritty.yml");
                    if fallback.exists() {
                        return Some(fallback);
                    }
                }
                None
            })
            .map(|path| path.into())
    }

    // TODO: Remove old configuration location warning (Deprecated 03/12/2018)
    #[cfg(windows)]
    pub fn installed_config<'a>() -> Option<Cow<'a, Path>> {
        let old = dirs::home_dir()
            .map(|path| path.join("alacritty.yml"));
        let new = dirs::config_dir()
            .map(|path| path.join("alacritty\\alacritty.yml"));

        if let Some(old_path) = old.as_ref().filter(|old| old.exists()) {
            warn!(
                "Found configuration at: '{}'. The file should be moved to the new location: '{}'.",
                old_path.to_string_lossy(),
                new.as_ref().map(|new| new.to_string_lossy()).unwrap(),
            );

            old.map(Cow::from)
        } else {
            new.filter(|new| new.exists()).map(Cow::from)
        }
    }

    #[cfg(not(windows))]
    pub fn write_defaults() -> io::Result<Cow<'static, Path>> {
        let path = xdg::BaseDirectories::with_prefix("alacritty")
            .map_err(|err| io::Error::new(io::ErrorKind::NotFound, err.to_string().as_str()))
            .and_then(|p| p.place_config_file("alacritty.yml"))?;

        File::create(&path)?.write_all(DEFAULT_ALACRITTY_CONFIG.as_bytes())?;

        Ok(path.into())
    }

    #[cfg(windows)]
    pub fn write_defaults() -> io::Result<Cow<'static, Path>> {
        let mut path = dirs::config_dir()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "could not find profile directory")
            }
        )?;

        path = path.join("alacritty/alacritty.yml");

        fs::create_dir_all(&path)?;

        File::create(&path)?.write_all(DEFAULT_ALACRITTY_CONFIG.as_bytes())?;

        Ok(path.into())
    }

    /// Get list of colors
    ///
    /// The ordering returned here is expected by the terminal. Colors are simply indexed in this
    /// array for performance.
    pub fn colors(&self) -> &Colors {
        &self.colors
    }

    #[inline]
    pub fn background_opacity(&self) -> Alpha {
        self.background_opacity
    }

    pub fn key_bindings(&self) -> &[KeyBinding] {
        &self.key_bindings[..]
    }

    pub fn mouse_bindings(&self) -> &[MouseBinding] {
        &self.mouse_bindings[..]
    }

    pub fn mouse(&self) -> &Mouse {
        &self.mouse
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn tabspaces(&self) -> usize {
        self.tabspaces
    }

    pub fn padding(&self) -> &Delta<u8> {
        self.padding.as_ref()
            .unwrap_or(&self.window.padding)
    }

    #[inline]
    pub fn draw_bold_text_with_bright_colors(&self) -> bool {
        self.draw_bold_text_with_bright_colors
    }

    /// Get font config
    #[inline]
    pub fn font(&self) -> &Font {
        &self.font
    }

    /// Get window dimensions
    #[inline]
    pub fn dimensions(&self) -> Dimensions {
        self.dimensions.unwrap_or(self.window.dimensions)
    }

    /// Get window config
    #[inline]
    pub fn window(&self) -> &WindowConfig {
        &self.window
    }

    /// Get visual bell config
    #[inline]
    pub fn visual_bell(&self) -> &VisualBellConfig {
        &self.visual_bell
    }

    /// Should show render timer
    #[inline]
    pub fn render_timer(&self) -> bool {
        self.render_timer
    }

    #[inline]
    pub fn use_thin_strokes(&self) -> bool {
        self.font.use_thin_strokes
    }

    pub fn path(&self) -> Option<&Path> {
        self.config_path
            .as_ref()
            .map(|p| p.as_path())
    }

    pub fn shell(&self) -> Option<&Shell<'_>> {
        self.shell.as_ref()
    }

    pub fn env(&self) -> &HashMap<String, String> {
        &self.env
    }

    /// Should hide mouse cursor when typing
    #[inline]
    pub fn hide_mouse_when_typing(&self) -> bool {
        self.hide_cursor_when_typing.unwrap_or(self.mouse.hide_when_typing)
    }

    /// Style of the cursor
    #[inline]
    pub fn cursor_style(&self) -> CursorStyle {
        self.cursor_style.unwrap_or(self.cursor.style)
    }

    /// Use hollow block cursor when unfocused
    #[inline]
    pub fn unfocused_hollow_cursor(&self) -> bool {
        self.unfocused_hollow_cursor.unwrap_or(self.cursor.unfocused_hollow)
    }

    /// Live config reload
    #[inline]
    pub fn live_config_reload(&self) -> bool {
        self.live_config_reload
    }

    #[inline]
    pub fn dynamic_title(&self) -> bool {
        self.dynamic_title
    }

    /// Scrolling settings
    #[inline]
    pub fn scrolling(&self) -> Scrolling {
        self.scrolling
    }

    /// Cursor foreground color
    #[inline]
    pub fn cursor_text_color(&self) -> Option<Color> {
        self.colors.cursor.text.map(|_| Color::Named(NamedColor::CursorText))
    }

    /// Cursor background color
    #[inline]
    pub fn cursor_cursor_color(&self) -> Option<Color> {
        self.colors.cursor.cursor.map(|_| Color::Named(NamedColor::Cursor))
    }

    // Update the history size, used in ref tests
    pub fn set_history(&mut self, history: u32) {
        self.scrolling.history = history;
    }

    /// Keep the log file after quitting Alacritty
    #[inline]
    pub fn persistent_logging(&self) -> bool {
        self.persistent_logging
    }

    pub fn load_from<P: Into<PathBuf>>(path: P) -> Result<Config> {
        let path = path.into();
        let raw = Config::read_file(path.as_path())?;
        let mut config: Config = serde_yaml::from_str(&raw)?;
        config.config_path = Some(path);
        config.print_deprecation_warnings();

        Ok(config)
    }

    /// Overrides the `dynamic_title` configuration based on `--title`.
    pub fn update_dynamic_title(mut self, options: &Options) -> Self {
        if options.title.is_some() {
            self.dynamic_title = false;
        }
        self
    }

    fn read_file<P: AsRef<Path>>(path: P) -> Result<String> {
        let mut f = fs::File::open(path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        if contents.is_empty() {
            return Err(Error::Empty);
        }

        Ok(contents)
    }

    fn print_deprecation_warnings(&mut self) {
        if self.dimensions.is_some() {
            warn!("{}", "Config `dimensions` is deprecated. \
                  Please use `window.dimensions` instead.");
        }

        if self.padding.is_some() {
            warn!("{}", "Config `padding` is deprecated. Please use `window.padding` instead.");
        }

        if self.mouse.faux_scrollback_lines.is_some() {
            warn!("{}", "Config `mouse.faux_scrollback_lines` is deprecated. \
                  Please use `mouse.faux_scrolling_lines` instead.");
        }

        if let Some(custom_cursor_colors) = self.custom_cursor_colors {
            warn!("{}", "Config `custom_cursor_colors` is deprecated.");

            if !custom_cursor_colors {
                self.colors.cursor.cursor = None;
                self.colors.cursor.text = None;
            }
        }

        if self.cursor_style.is_some() {
            warn!("{}", "Config `cursor_style` is deprecated. Please use `cursor.style` instead.");
        }

        if self.hide_cursor_when_typing.is_some() {
            warn!("{}", "Config `hide_cursor_when_typing` is deprecated. \
                  Please use `mouse.hide_when_typing` instead.");
        }

        if self.unfocused_hollow_cursor.is_some() {
            warn!("{}", "Config `unfocused_hollow_cursor` is deprecated. \
                  Please use `cursor.unfocused_hollow` instead.");
        }
    }
}

/// Window Dimensions
///
/// Newtype to avoid passing values incorrectly
#[derive(Debug, Copy, Clone, Deserialize)]
pub struct Dimensions {
    /// Window width in character columns
    columns: Column,

    /// Window Height in character lines
    lines: Line,
}

impl Default for Dimensions {
    fn default() -> Dimensions {
        Dimensions::new(Column(80), Line(24))
    }
}

impl Dimensions {
    pub fn new(columns: Column, lines: Line) -> Self {
        Dimensions {
            columns,
            lines,
        }
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

/// A delta for a point in a 2 dimensional plane
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de> + Default"))]
pub struct Delta<T: Default> {
    /// Horizontal change
    #[serde(default, deserialize_with = "failure_default")]
    pub x: T,
    /// Vertical change
    #[serde(default, deserialize_with = "failure_default")]
    pub y: T,
}

trait DeserializeSize : Sized {
    fn deserialize<'a, D>(_: D) -> ::std::result::Result<Self, D::Error>
        where D: serde::de::Deserializer<'a>;
}

impl DeserializeSize for Size {
    fn deserialize<'a, D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: serde::de::Deserializer<'a>
    {
        use std::marker::PhantomData;

        struct NumVisitor<__D> {
            _marker: PhantomData<__D>,
        }

        impl<'a, __D> Visitor<'a> for NumVisitor<__D>
            where __D: serde::de::Deserializer<'a>
        {
            type Value = f64;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("f64 or u64")
            }

            fn visit_f64<E>(self, value: f64) -> ::std::result::Result<Self::Value, E>
                where E: ::serde::de::Error
            {
                Ok(value)
            }

            fn visit_u64<E>(self, value: u64) -> ::std::result::Result<Self::Value, E>
                where E: ::serde::de::Error
            {
                Ok(value as f64)
            }
        }

        let size = deserializer
            .deserialize_any(NumVisitor::<D>{ _marker: PhantomData })
            .map(|v| Size::new(v as _));

        // Use font size 12 as fallback
        match size {
            Ok(size) => Ok(size),
            Err(err) => {
                error!("problem with config: {}; Using size 12", err);
                Ok(Size::new(12.))
            },
        }
    }
}

/// Font config
///
/// Defaults are provided at the level of this struct per platform, but not per
/// field in this struct. It might be nice in the future to have defaults for
/// each value independently. Alternatively, maybe erroring when the user
/// doesn't provide complete config is Ok.
#[derive(Debug, Deserialize, Clone)]
pub struct Font {
    /// Font family
    pub normal: FontDescription,

    #[serde(default="default_italic_desc")]
    pub italic: FontDescription,

    #[serde(default="default_bold_desc")]
    pub bold: FontDescription,

    // Font size in points
    #[serde(deserialize_with="DeserializeSize::deserialize")]
    pub size: Size,

    /// Extra spacing per character
    #[serde(default, deserialize_with = "failure_default")]
    offset: Delta<i8>,

    /// Glyph offset within character cell
    #[serde(default, deserialize_with = "failure_default")]
    glyph_offset: Delta<i8>,

    #[serde(default="true_bool", deserialize_with = "default_true_bool")]
    use_thin_strokes: bool,

    // TODO: Deprecated
    #[serde(default, deserialize_with = "deserialize_scale_with_dpi")]
    scale_with_dpi: Option<()>,
}

fn deserialize_scale_with_dpi<'a, D>(deserializer: D) -> ::std::result::Result<Option<()>, D::Error>
where
    D: de::Deserializer<'a>,
{
    // This is necessary in order to get serde to complete deserialization of the configuration
    let _ignored = bool::deserialize(deserializer);
    error!("The `scale_with_dpi` setting has been removed, \
            on X11 the WINIT_HIDPI_FACTOR environment variable can be used instead.");
    Ok(None)
}

fn default_bold_desc() -> FontDescription {
    Font::default().bold
}

fn default_italic_desc() -> FontDescription {
    Font::default().italic
}

/// Description of a single font
#[derive(Debug, Deserialize, Clone)]
pub struct FontDescription {
    pub family: String,
    pub style: Option<String>,
}

impl FontDescription {
    fn new_with_family<S: Into<String>>(family: S) -> FontDescription {
        FontDescription {
            family: family.into(),
            style: None,
        }
    }
}

impl Font {
    /// Get the font size in points
    #[inline]
    pub fn size(&self) -> Size {
        self.size
    }

    /// Get offsets to font metrics
    #[inline]
    pub fn offset(&self) -> &Delta<i8> {
        &self.offset
    }

    /// Get cell offsets for glyphs
    #[inline]
    pub fn glyph_offset(&self) -> &Delta<i8> {
        &self.glyph_offset
    }

    /// Get a font clone with a size modification
    pub fn with_size(self, size: Size) -> Font {
        Font {
            size,
            .. self
        }
    }
}

#[cfg(target_os = "macos")]
impl Default for Font {
    fn default() -> Font {
        Font {
            normal: FontDescription::new_with_family("Menlo"),
            bold: FontDescription::new_with_family("Menlo"),
            italic: FontDescription::new_with_family("Menlo"),
            size: Size::new(11.0),
            use_thin_strokes: true,
            scale_with_dpi: None,
            glyph_offset: Default::default(),
            offset: Default::default(),
        }
    }
}

#[cfg(any(target_os = "linux",target_os = "freebsd",target_os = "openbsd"))]
impl Default for Font {
    fn default() -> Font {
        Font {
            normal: FontDescription::new_with_family("monospace"),
            bold: FontDescription::new_with_family("monospace"),
            italic: FontDescription::new_with_family("monospace"),
            size: Size::new(11.0),
            use_thin_strokes: false,
            scale_with_dpi: None,
            glyph_offset: Default::default(),
            offset: Default::default(),
        }
    }
}

#[cfg(windows)]
impl Default for Font {
    fn default() -> Font {
        Font {
            normal: FontDescription::new_with_family("Consolas"),
            bold: FontDescription::new_with_family("Consolas"),
            italic: FontDescription::new_with_family("Consolas"),
            size: Size::new(11.0),
            use_thin_strokes: false,
            scale_with_dpi: None,
            glyph_offset: Default::default(),
            offset: Default::default(),
        }
    }
}

pub struct Monitor {
    _thread: ::std::thread::JoinHandle<()>,
    rx: mpsc::Receiver<Config>,
}

pub trait OnConfigReload {
    fn on_config_reload(&mut self);
}

impl OnConfigReload for crate::display::Notifier {
    fn on_config_reload(&mut self) {
        self.notify();
    }
}

impl Monitor {
    /// Get pending config changes
    pub fn pending_config(&self) -> Option<Config> {
        let mut config = None;
        while let Ok(new) = self.rx.try_recv() {
            config = Some(new);
        }

        config
    }
    pub fn new<H, P>(path: P, mut handler: H) -> Monitor
        where H: OnConfigReload + Send + 'static,
              P: Into<PathBuf>
    {
        let path = path.into();

        let (config_tx, config_rx) = mpsc::channel();

        Monitor {
            _thread: crate::util::thread::spawn_named("config watcher", move || {
                let (tx, rx) = mpsc::channel();
                // The Duration argument is a debouncing period.
                let mut watcher = watcher(tx, Duration::from_millis(10))
                    .expect("Unable to spawn file watcher");
                let config_path = ::std::fs::canonicalize(path)
                    .expect("canonicalize config path");

                // Get directory of config
                let mut parent = config_path.clone();
                parent.pop();

                // Watch directory
                watcher.watch(&parent, RecursiveMode::NonRecursive)
                    .expect("watch alacritty.yml dir");

                loop {
                    match rx.recv().expect("watcher event") {
                        DebouncedEvent::Rename(_, _) => continue,
                        DebouncedEvent::Write(path) | DebouncedEvent::Create(path)
                         | DebouncedEvent::Chmod(path) => {
                            // Reload file
                            if path == config_path {
                                match Config::load_from(path) {
                                    Ok(config) => {
                                        let _ = config_tx.send(config);
                                        handler.on_config_reload();
                                    },
                                    Err(err) => error!("Ignoring invalid config: {}", err),
                                }
                             }
                        }
                        _ => {}
                    }
                }
            }),
            rx: config_rx,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::Options;
    use super::Config;

    #[cfg(target_os="macos")]
    static ALACRITTY_YML: &'static str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/alacritty_macos.yml"));
    #[cfg(windows)]
    static ALACRITTY_YML: &'static str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/alacritty_windows.yml"));
    #[cfg(not(any(target_os="macos", windows)))]
    static ALACRITTY_YML: &'static str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/alacritty.yml"));

    #[test]
    fn parse_config() {
        let config: Config = ::serde_yaml::from_str(ALACRITTY_YML)
            .expect("deserialize config");

        // Sanity check that mouse bindings are being parsed
        assert!(!config.mouse_bindings.is_empty());

        // Sanity check that key bindings are being parsed
        assert!(!config.key_bindings.is_empty());
    }

    #[test]
    fn dynamic_title_ignoring_options_by_default() {
        let config: Config = ::serde_yaml::from_str(ALACRITTY_YML)
            .expect("deserialize config");
        let old_dynamic_title = config.dynamic_title;
        let options = Options::default();
        let config = config.update_dynamic_title(&options);
        assert_eq!(old_dynamic_title, config.dynamic_title);
    }

    #[test]
    fn dynamic_title_overridden_by_options() {
        let config: Config = ::serde_yaml::from_str(ALACRITTY_YML)
            .expect("deserialize config");
        let mut options = Options::default();
        options.title = Some("foo".to_owned());
        let config = config.update_dynamic_title(&options);
        assert!(!config.dynamic_title);
    }
}

#[derive(Deserialize, Copy, Clone, Debug, Eq, PartialEq)]
pub enum Key {
    Scancode(u32),
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Snapshot,
    Scroll,
    Pause,
    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,
    Left,
    Up,
    Right,
    Down,
    Back,
    Return,
    Space,
    Compose,
    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    AbntC1,
    AbntC2,
    Add,
    Apostrophe,
    Apps,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    Decimal,
    Divide,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    Multiply,
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    OEM102,
    Period,
    PlayPause,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    Subtract,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
    Caret,
    Copy,
    Paste,
    Cut,
}

impl Key {
    pub fn from_glutin_input(key: ::glutin::VirtualKeyCode) -> Self {
        use glutin::VirtualKeyCode::*;
        // Thank you, vim macros and regex!
        match key {
            Key1 => Key::Key1,
            Key2 => Key::Key2,
            Key3 => Key::Key3,
            Key4 => Key::Key4,
            Key5 => Key::Key5,
            Key6 => Key::Key6,
            Key7 => Key::Key7,
            Key8 => Key::Key8,
            Key9 => Key::Key9,
            Key0 => Key::Key0,
            A => Key::A,
            B => Key::B,
            C => Key::C,
            D => Key::D,
            E => Key::E,
            F => Key::F,
            G => Key::G,
            H => Key::H,
            I => Key::I,
            J => Key::J,
            K => Key::K,
            L => Key::L,
            M => Key::M,
            N => Key::N,
            O => Key::O,
            P => Key::P,
            Q => Key::Q,
            R => Key::R,
            S => Key::S,
            T => Key::T,
            U => Key::U,
            V => Key::V,
            W => Key::W,
            X => Key::X,
            Y => Key::Y,
            Z => Key::Z,
            Escape => Key::Escape,
            F1 => Key::F1,
            F2 => Key::F2,
            F3 => Key::F3,
            F4 => Key::F4,
            F5 => Key::F5,
            F6 => Key::F6,
            F7 => Key::F7,
            F8 => Key::F8,
            F9 => Key::F9,
            F10 => Key::F10,
            F11 => Key::F11,
            F12 => Key::F12,
            F13 => Key::F13,
            F14 => Key::F14,
            F15 => Key::F15,
            F16 => Key::F16,
            F17 => Key::F17,
            F18 => Key::F18,
            F19 => Key::F19,
            F20 => Key::F20,
            F21 => Key::F21,
            F22 => Key::F22,
            F23 => Key::F23,
            F24 => Key::F24,
            Snapshot => Key::Snapshot,
            Scroll => Key::Scroll,
            Pause => Key::Pause,
            Insert => Key::Insert,
            Home => Key::Home,
            Delete => Key::Delete,
            End => Key::End,
            PageDown => Key::PageDown,
            PageUp => Key::PageUp,
            Left => Key::Left,
            Up => Key::Up,
            Right => Key::Right,
            Down => Key::Down,
            Back => Key::Back,
            Return => Key::Return,
            Space => Key::Space,
            Compose => Key::Compose,
            Numlock => Key::Numlock,
            Numpad0 => Key::Numpad0,
            Numpad1 => Key::Numpad1,
            Numpad2 => Key::Numpad2,
            Numpad3 => Key::Numpad3,
            Numpad4 => Key::Numpad4,
            Numpad5 => Key::Numpad5,
            Numpad6 => Key::Numpad6,
            Numpad7 => Key::Numpad7,
            Numpad8 => Key::Numpad8,
            Numpad9 => Key::Numpad9,
            AbntC1 => Key::AbntC1,
            AbntC2 => Key::AbntC2,
            Add => Key::Add,
            Apostrophe => Key::Apostrophe,
            Apps => Key::Apps,
            At => Key::At,
            Ax => Key::Ax,
            Backslash => Key::Backslash,
            Calculator => Key::Calculator,
            Capital => Key::Capital,
            Colon => Key::Colon,
            Comma => Key::Comma,
            Convert => Key::Convert,
            Decimal => Key::Decimal,
            Divide => Key::Divide,
            Equals => Key::Equals,
            Grave => Key::Grave,
            Kana => Key::Kana,
            Kanji => Key::Kanji,
            LAlt => Key::LAlt,
            LBracket => Key::LBracket,
            LControl => Key::LControl,
            LShift => Key::LShift,
            LWin => Key::LWin,
            Mail => Key::Mail,
            MediaSelect => Key::MediaSelect,
            MediaStop => Key::MediaStop,
            Minus => Key::Minus,
            Multiply => Key::Multiply,
            Mute => Key::Mute,
            MyComputer => Key::MyComputer,
            NavigateForward => Key::NavigateForward,
            NavigateBackward => Key::NavigateBackward,
            NextTrack => Key::NextTrack,
            NoConvert => Key::NoConvert,
            NumpadComma => Key::NumpadComma,
            NumpadEnter => Key::NumpadEnter,
            NumpadEquals => Key::NumpadEquals,
            OEM102 => Key::OEM102,
            Period => Key::Period,
            PlayPause => Key::PlayPause,
            Power => Key::Power,
            PrevTrack => Key::PrevTrack,
            RAlt => Key::RAlt,
            RBracket => Key::RBracket,
            RControl => Key::RControl,
            RShift => Key::RShift,
            RWin => Key::RWin,
            Semicolon => Key::Semicolon,
            Slash => Key::Slash,
            Sleep => Key::Sleep,
            Stop => Key::Stop,
            Subtract => Key::Subtract,
            Sysrq => Key::Sysrq,
            Tab => Key::Tab,
            Underline => Key::Underline,
            Unlabeled => Key::Unlabeled,
            VolumeDown => Key::VolumeDown,
            VolumeUp => Key::VolumeUp,
            Wake => Key::Wake,
            WebBack => Key::WebBack,
            WebFavorites => Key::WebFavorites,
            WebForward => Key::WebForward,
            WebHome => Key::WebHome,
            WebRefresh => Key::WebRefresh,
            WebSearch => Key::WebSearch,
            WebStop => Key::WebStop,
            Yen => Key::Yen,
            Caret => Key::Caret,
            Copy => Key::Copy,
            Paste => Key::Paste,
            Cut => Key::Cut,
        }
    }
}
