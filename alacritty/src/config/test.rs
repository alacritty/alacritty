use alacritty_terminal::config::DEFAULT_ALACRITTY_CONFIG;

use crate::config::Config;

#[test]
fn parse_config() {
    let config: Config =
        ::serde_yaml::from_str(DEFAULT_ALACRITTY_CONFIG).expect("deserialize config");

    // Sanity check that mouse bindings are being parsed
    assert!(!config.ui_config.mouse_bindings.is_empty());

    // Sanity check that key bindings are being parsed
    assert!(!config.ui_config.key_bindings.is_empty());
}

#[test]
fn default_match_empty() {
    let default = Config::default();

    let empty = serde_yaml::from_str("key: val\n").unwrap();

    assert_eq!(default, empty);
}
