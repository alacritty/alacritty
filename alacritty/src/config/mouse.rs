use serde::{Deserialize, Deserializer};

use super::bindings::ModsWrapper;
use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};
use winit::keyboard::ModifiersState;

use crate::config::bindings::{self, MouseBinding};
use crate::config::ui_config;

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub hide_when_typing: bool,
    pub bindings: MouseBindings,
    pub leader_mod: ModsWrapper,
}

impl Default for Mouse {
    fn default() -> Self {
        Self {
            hide_when_typing: false,
            bindings: MouseBindings::default(),
            leader_mod: ModsWrapper(ModifiersState::SHIFT),
        }
    }
}

#[derive(SerdeReplace, Clone, Debug, PartialEq, Eq)]
pub struct MouseBindings(pub Vec<MouseBinding>);

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
        Ok(Self(ui_config::deserialize_bindings(deserializer, Self::default().0)?))
    }
}
