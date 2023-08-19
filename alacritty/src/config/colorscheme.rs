use alacritty_terminal::term::color::{CellRgb, Rgb};
use alacritty_config_derive::ConfigDeserialize;

use crate::config::color::BrightColors;
use crate::config::color::Colors;
use crate::config::color::InvertedCellColors;
use crate::config::color::NormalColors;
use crate::config::color::PrimaryColors;

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum ThemeVariant {
    Light,
    Dark,
    System,
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ColorScheme {
    pub mode: ThemeVariant,
    pub light: Colors,
    pub dark: Colors,
}

impl Default for ColorScheme {
    fn default() -> ColorScheme {
        let cursor_colors = InvertedCellColors {
                foreground: CellRgb::Rgb(Rgb::new(0x28, 0x28, 0x28)),
                background: CellRgb::Rgb(Rgb::new(0x00, 0x00, 0x00)),
            };
        let light_colors = Colors {
            primary: PrimaryColors {
                foreground: Rgb::new(0x00, 0x00, 0x00),
                background: Rgb::new(0xff, 0xff, 0xff),
                bright_foreground: Default::default(),
                dim_foreground: Default::default(),
            },
            cursor: cursor_colors,
            vi_mode_cursor: cursor_colors,
            selection: cursor_colors,
            normal: NormalColors {
                black: Rgb::new(0xd7, 0xd7, 0xd7),
                red: Rgb::new(0x97, 0x25, 0x00),
                green: Rgb::new(0x31, 0x5b, 0x00),
                yellow: Rgb::new(0x70, 0x48, 0x0f),
                blue: Rgb::new(0x25, 0x44, 0xbb),
                magenta: Rgb::new(0x8f, 0x00, 0x75),
                cyan: Rgb::new(0x30, 0x51, 0x7f),
                white: Rgb::new(0x00, 0x00, 0x00),
            },
            bright: BrightColors {
                black: Rgb::new(0x50, 0x50, 0x50),
                red: Rgb::new(0xa6, 0x00, 0x00),
                green: Rgb::new(0x00, 0x5e, 0x00),
                yellow: Rgb::new(0x81, 0x3e, 0x00),
                blue: Rgb::new(0x00, 0x31, 0xa9),
                magenta: Rgb::new(0x72, 0x10, 0x45),
                cyan: Rgb::new(0x00, 0x53, 0x8b),
                white: Rgb::new(0x00, 0x00, 0x00),
            },
            ..Default::default()
        };

        let dark_colors = Colors::default();

        ColorScheme { mode: ThemeVariant::System, light: light_colors, dark: dark_colors }
    }
}
