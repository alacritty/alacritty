use serde::Serialize;

use alacritty_config_derive::ConfigDeserialize;
use alacritty_terminal::term::SEMANTIC_ESCAPE_CHARS;

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub semantic_escape_chars: String,
    pub save_to_clipboard: bool,
    pub force_x11: bool,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            semantic_escape_chars: SEMANTIC_ESCAPE_CHARS.to_owned(),
            save_to_clipboard: Default::default(),
            force_x11: Default::default(),
        }
    }
}
