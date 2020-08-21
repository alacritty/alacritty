use std::path::PathBuf;

use log::error;
use serde::{Deserialize, Deserializer};

use alacritty_terminal::config::{failure_default, Percentage, LOG_TARGET_CONFIG};

use crate::config::bindings::{self, Binding, KeyBinding, MouseBinding};
use crate::config::debug::Debug;
use crate::config::font::Font;
use crate::config::mouse::Mouse;
use crate::config::window::WindowConfig;

#[derive(Debug, PartialEq, Deserialize)]
pub struct UIConfig {
    /// Font configuration.
    #[serde(default, deserialize_with = "failure_default")]
    pub font: Font,

    /// Window configuration.
    #[serde(default, deserialize_with = "failure_default")]
    pub window: WindowConfig,

    #[serde(default, deserialize_with = "failure_default")]
    pub mouse: Mouse,

    /// Keybindings.
    #[serde(default = "default_key_bindings", deserialize_with = "deserialize_key_bindings")]
    pub key_bindings: Vec<KeyBinding>,

    /// Bindings for the mouse.
    #[serde(default = "default_mouse_bindings", deserialize_with = "deserialize_mouse_bindings")]
    pub mouse_bindings: Vec<MouseBinding>,

    /// Debug options.
    #[serde(default, deserialize_with = "failure_default")]
    pub debug: Debug,

    /// Send escape sequences using the alt key.
    #[serde(default, deserialize_with = "failure_default")]
    alt_send_esc: DefaultTrueBool,

    /// Live config reload.
    #[serde(default, deserialize_with = "failure_default")]
    live_config_reload: DefaultTrueBool,

    /// Background opacity from 0.0 to 1.0.
    #[serde(default, deserialize_with = "failure_default")]
    background_opacity: Percentage,

    /// Path where config was loaded from.
    #[serde(skip)]
    pub config_paths: Vec<PathBuf>,

    // TODO: DEPRECATED
    #[serde(default, deserialize_with = "failure_default")]
    pub dynamic_title: Option<bool>,
}

impl Default for UIConfig {
    fn default() -> Self {
        UIConfig {
            font: Default::default(),
            window: Default::default(),
            mouse: Default::default(),
            key_bindings: default_key_bindings(),
            mouse_bindings: default_mouse_bindings(),
            debug: Default::default(),
            alt_send_esc: Default::default(),
            background_opacity: Default::default(),
            live_config_reload: Default::default(),
            dynamic_title: Default::default(),
            config_paths: Default::default(),
        }
    }
}

impl UIConfig {
    #[inline]
    pub fn background_opacity(&self) -> f32 {
        self.background_opacity.as_f32()
    }

    #[inline]
    pub fn dynamic_title(&self) -> bool {
        self.dynamic_title.unwrap_or_else(|| self.window.dynamic_title())
    }

    #[inline]
    pub fn set_dynamic_title(&mut self, dynamic_title: bool) {
        self.window.set_dynamic_title(dynamic_title);
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

    /// Send escape sequences using the alt key.
    #[inline]
    pub fn alt_send_esc(&self) -> bool {
        self.alt_send_esc.0
    }
}

fn default_key_bindings() -> Vec<KeyBinding> {
    bindings::default_key_bindings()
}

fn default_mouse_bindings() -> Vec<MouseBinding> {
    bindings::default_mouse_bindings()
}

fn deserialize_key_bindings<'a, D>(deserializer: D) -> Result<Vec<KeyBinding>, D::Error>
where
    D: Deserializer<'a>,
{
    deserialize_bindings(deserializer, bindings::default_key_bindings())
}

fn deserialize_mouse_bindings<'a, D>(deserializer: D) -> Result<Vec<MouseBinding>, D::Error>
where
    D: Deserializer<'a>,
{
    deserialize_bindings(deserializer, bindings::default_mouse_bindings())
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
                error!(target: LOG_TARGET_CONFIG, "Problem with config: {}; ignoring binding", err);
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

#[derive(Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct DefaultTrueBool(pub bool);

impl Default for DefaultTrueBool {
    fn default() -> Self {
        DefaultTrueBool(true)
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
