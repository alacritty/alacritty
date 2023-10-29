use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

use alacritty_terminal::term::Osc52 as TermOsc52;
use serde::Deserialize;

#[derive(ConfigDeserialize, Default, Copy, Clone, Debug, PartialEq)]
pub struct Terminal {
    /// How to support OSC52.
    pub osc52: Osc52,
}

#[derive(SerdeReplace, Deserialize, Default, Copy, Clone, Debug, PartialEq)]
pub struct Osc52(pub TermOsc52);
