use std::time::Duration;

use serde::{Deserialize, Deserializer};

use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

use crate::config::bindings::{self, MouseBinding};
use crate::config::ui_config;

#[derive(ConfigDeserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub double_click: ClickHandler,
    pub triple_click: ClickHandler,
    pub hide_when_typing: bool,
    pub bindings: MouseBindings,
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClickHandler {
    threshold: u16,
}

impl Default for ClickHandler {
    fn default() -> Self {
        Self { threshold: 300 }
    }
}

impl ClickHandler {
    pub fn threshold(&self) -> Duration {
        Duration::from_millis(self.threshold as u64)
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
