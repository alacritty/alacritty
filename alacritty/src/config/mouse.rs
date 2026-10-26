use serde::{Deserialize, Deserializer, Serialize, Serializer};

use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

use crate::config::bindings::{self, MouseBinding};
use crate::config::ui_config;

#[derive(ConfigDeserialize, Serialize, Default, Clone, Debug, PartialEq)]
pub struct Mouse {
    pub hide_when_typing: bool,
    pub bindings: MouseBindings,
}

/// Active mouse bindings and the user's binding entries they were created from.
#[derive(SerdeReplace, Clone, Debug, PartialEq)]
pub struct MouseBindings {
    pub bindings: Vec<MouseBinding>,
    user_bindings: Vec<toml::Value>,
}

impl Default for MouseBindings {
    fn default() -> Self {
        Self { bindings: bindings::default_mouse_bindings(), user_bindings: Vec::new() }
    }
}

impl<'de> Deserialize<'de> for MouseBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut user_bindings = Vec::<toml::Value>::deserialize(deserializer)?;
        let bindings =
            ui_config::deserialize_bindings(&mut user_bindings, bindings::default_mouse_bindings());
        Ok(Self { bindings, user_bindings })
    }
}

impl Serialize for MouseBindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.user_bindings.serialize(serializer)
    }
}
