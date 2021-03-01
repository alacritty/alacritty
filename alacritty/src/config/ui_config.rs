use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use log::error;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer};
use unicode_width::UnicodeWidthChar;

use alacritty_config_derive::ConfigDeserialize;
use alacritty_terminal::config::{Percentage, Program, LOG_TARGET_CONFIG};
use alacritty_terminal::term::search::RegexSearch;

use crate::config::bell::BellConfig;
use crate::config::bindings::{
    self, Action, Binding, BindingMode, Key, KeyBinding, ModsWrapper, MouseBinding,
};
use crate::config::color::Colors;
use crate::config::debug::Debug;
use crate::config::font::Font;
use crate::config::mouse::Mouse;
use crate::config::window::WindowConfig;

#[derive(ConfigDeserialize, Debug, PartialEq)]
pub struct UiConfig {
    /// Font configuration.
    pub font: Font,

    /// Window configuration.
    pub window: WindowConfig,

    pub mouse: Mouse,

    /// Debug options.
    pub debug: Debug,

    /// Send escape sequences using the alt key.
    pub alt_send_esc: bool,

    /// Live config reload.
    pub live_config_reload: bool,

    /// Bell configuration.
    pub bell: BellConfig,

    /// RGB values for colors.
    pub colors: Colors,

    /// Should draw bold text with brighter colors instead of bold font.
    pub draw_bold_text_with_bright_colors: bool,

    /// Path where config was loaded from.
    #[config(skip)]
    pub config_paths: Vec<PathBuf>,

    /// Regex hints for interacting with terminal content.
    pub hints: Hints,

    /// Keybindings.
    key_bindings: KeyBindings,

    /// Bindings for the mouse.
    mouse_bindings: MouseBindings,

    /// Background opacity from 0.0 to 1.0.
    background_opacity: Percentage,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            alt_send_esc: true,
            live_config_reload: true,
            font: Default::default(),
            window: Default::default(),
            mouse: Default::default(),
            debug: Default::default(),
            config_paths: Default::default(),
            key_bindings: Default::default(),
            mouse_bindings: Default::default(),
            background_opacity: Default::default(),
            bell: Default::default(),
            colors: Default::default(),
            draw_bold_text_with_bright_colors: Default::default(),
            hints: Default::default(),
        }
    }
}

impl UiConfig {
    /// Generate key bindings for all keyboard hints.
    pub fn generate_hint_bindings(&mut self) {
        for hint in self.hints.enabled.drain(..) {
            let binding = KeyBinding {
                trigger: hint.binding.key,
                mods: hint.binding.mods.0,
                mode: BindingMode::empty(),
                notmode: BindingMode::empty(),
                action: Action::Hint(hint),
            };

            self.key_bindings.0.push(binding);
        }
    }

    #[inline]
    pub fn background_opacity(&self) -> f32 {
        self.background_opacity.as_f32()
    }

    #[inline]
    pub fn key_bindings(&self) -> &[KeyBinding] {
        &self.key_bindings.0.as_slice()
    }

    #[inline]
    pub fn mouse_bindings(&self) -> &[MouseBinding] {
        self.mouse_bindings.0.as_slice()
    }
}

#[derive(Debug, PartialEq)]
struct KeyBindings(Vec<KeyBinding>);

impl Default for KeyBindings {
    fn default() -> Self {
        Self(bindings::default_key_bindings())
    }
}

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(deserialize_bindings(deserializer, Self::default().0)?))
    }
}

#[derive(Debug, PartialEq)]
struct MouseBindings(Vec<MouseBinding>);

impl Default for MouseBindings {
    fn default() -> Self {
        Self(bindings::default_mouse_bindings())
    }
}

impl<'de> Deserialize<'de> for MouseBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(deserialize_bindings(deserializer, Self::default().0)?))
    }
}

fn deserialize_bindings<'a, D, T>(
    deserializer: D,
    mut default: Vec<Binding<T>>,
) -> Result<Vec<Binding<T>>, D::Error>
where
    D: Deserializer<'a>,
    T: Copy + Eq,
    Binding<T>: Deserialize<'a>,
{
    let values = Vec::<serde_yaml::Value>::deserialize(deserializer)?;

    // Skip all invalid values.
    let mut bindings = Vec::with_capacity(values.len());
    for value in values {
        match Binding::<T>::deserialize(value) {
            Ok(binding) => bindings.push(binding),
            Err(err) => {
                error!(target: LOG_TARGET_CONFIG, "Config error: {}; ignoring binding", err);
            },
        }
    }

    // Remove matching default bindings.
    for binding in bindings.iter() {
        default.retain(|b| !b.triggers_match(binding));
    }

    bindings.extend(default);

    Ok(bindings)
}

/// A delta for a point in a 2 dimensional plane.
#[derive(ConfigDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    /// Horizontal change.
    pub x: T,
    /// Vertical change.
    pub y: T,
}

/// Regex terminal hints.
#[derive(ConfigDeserialize, Default, Debug, PartialEq, Eq)]
pub struct Hints {
    /// Characters for the hint labels.
    alphabet: HintsAlphabet,

    /// All configured terminal hints.
    enabled: Vec<Hint>,
}

impl Hints {
    /// Characters for the hint labels.
    pub fn alphabet(&self) -> &str {
        &self.alphabet.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HintsAlphabet(String);

impl Default for HintsAlphabet {
    fn default() -> Self {
        Self(String::from("jfkdls;ahgurieowpq"))
    }
}

impl<'de> Deserialize<'de> for HintsAlphabet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        let mut character_count = 0;
        for character in value.chars() {
            if character.width() != Some(1) {
                return Err(D::Error::custom("characters must be of width 1"));
            }
            character_count += 1;
        }

        if character_count < 2 {
            return Err(D::Error::custom("must include at last 2 characters"));
        }

        Ok(Self(value))
    }
}

/// Hint configuration.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    /// Command the text will be piped to.
    pub command: Program,

    /// Regex for finding matches.
    pub regex: LazyRegex,

    /// Binding required to search for this hint.
    binding: HintBinding,
}

/// Binding for triggering a keyboard hint.
#[derive(Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct HintBinding {
    pub key: Key,
    pub mods: ModsWrapper,
}

/// Lazy regex with interior mutability.
#[derive(Clone, Debug)]
pub struct LazyRegex(Rc<RefCell<LazyRegexVariant>>);

impl LazyRegex {
    /// Execute a function with the compiled regex DFAs as parameter.
    pub fn with_compiled<T, F>(&self, f: F) -> T
    where
        F: Fn(&RegexSearch) -> T,
    {
        f(self.0.borrow_mut().compiled())
    }
}

impl<'de> Deserialize<'de> for LazyRegex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let regex = LazyRegexVariant::Pattern(String::deserialize(deserializer)?);
        Ok(Self(Rc::new(RefCell::new(regex))))
    }
}

/// Implement placeholder to allow derive upstream, since we never need it for this struct itself.
impl PartialEq for LazyRegex {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}
impl Eq for LazyRegex {}

/// Regex which is compiled on demand, to avoid expensive computations at startup.
#[derive(Clone, Debug)]
pub enum LazyRegexVariant {
    Compiled(Box<RegexSearch>),
    Pattern(String),
}

impl LazyRegexVariant {
    /// Get a reference to the compiled regex.
    ///
    /// If the regex is not already compiled, this will compile the DFAs and store them for future
    /// access.
    fn compiled(&mut self) -> &RegexSearch {
        // Check if the regex has already been compiled.
        let regex = match self {
            Self::Compiled(regex_search) => return regex_search,
            Self::Pattern(regex) => regex,
        };

        // Compile the regex.
        let regex_search = match RegexSearch::new(&regex) {
            Ok(regex_search) => regex_search,
            Err(error) => {
                error!("hint regex is invalid: {}", error);
                RegexSearch::new("").unwrap()
            },
        };
        *self = Self::Compiled(Box::new(regex_search));

        // Return a reference to the compiled DFAs.
        match self {
            Self::Compiled(dfas) => dfas,
            Self::Pattern(_) => unreachable!(),
        }
    }
}
