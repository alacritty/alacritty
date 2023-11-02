use serde::{de, Deserialize, Deserializer};
use toml::Value;

use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};
use alacritty_terminal::term::Osc52 as TermOsc52;

use crate::config::ui_config::StringVisitor;

#[derive(ConfigDeserialize, Default, Copy, Clone, Debug, PartialEq)]
pub struct Terminal {
    /// OSC52 support mode.
    pub osc52: Osc52,
}

#[derive(SerdeReplace, Default, Copy, Clone, Debug, PartialEq)]
pub struct Osc52(pub TermOsc52);

impl<'de> Deserialize<'de> for Osc52 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserializer.deserialize_str(StringVisitor)?;
        TermOsc52::deserialize(Value::String(value)).map(Osc52).map_err(de::Error::custom)
    }
}
