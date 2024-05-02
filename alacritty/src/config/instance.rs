use alacritty_config_derive::ConfigDeserialize;

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    /// Live config reload.
    pub live_config_reload: bool,
}

impl Default for Instance {
    fn default() -> Self {
        Self { live_config_reload: true }
    }
}
