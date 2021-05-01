use std::time::Duration;

use alacritty_config_derive::ConfigDeserialize;

#[derive(ConfigDeserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub double_click: ClickHandler,
    pub triple_click: ClickHandler,
    pub hide_when_typing: bool,
    #[config(deprecated = "use `hints` section instead")]
    pub url: Option<serde_yaml::Value>,
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
