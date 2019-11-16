use serde::{Deserialize, Deserializer};

use alacritty_terminal::config::failure_default;

use crate::config::bindings::{self, Binding, KeyBinding, MouseBinding};
use crate::config::mouse::Mouse;

#[derive(Debug, PartialEq, Deserialize)]
pub struct UIConfig {
    #[serde(default, deserialize_with = "failure_default")]
    pub mouse: Mouse,

    /// Keybindings
    #[serde(default = "default_key_bindings", deserialize_with = "deserialize_key_bindings")]
    pub key_bindings: Vec<KeyBinding>,

    /// Bindings for the mouse
    #[serde(default = "default_mouse_bindings", deserialize_with = "deserialize_mouse_bindings")]
    pub mouse_bindings: Vec<MouseBinding>,
}

impl Default for UIConfig {
    fn default() -> Self {
        UIConfig {
            mouse: Mouse::default(),
            key_bindings: default_key_bindings(),
            mouse_bindings: default_mouse_bindings(),
        }
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
    let mut bindings: Vec<Binding<T>> = failure_default(deserializer)?;

    // Remove matching default bindings
    for binding in bindings.iter() {
        default.retain(|b| !b.triggers_match(binding));
    }

    bindings.extend(default);

    Ok(bindings)
}
