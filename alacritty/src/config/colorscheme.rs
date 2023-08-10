use alacritty_terminal::term::color::Rgb;

use super::color::{BrightColors, Colors, NormalColors, PrimaryColors};

pub enum ThemeVariant {
    Light,
    Dark,
    System,
}

pub struct ColorScheme {
    pub theme_variant: ThemeVariant,
    pub light_colors: Colors,
    pub dark_colors: Colors,
}

impl Default for ColorScheme {
    fn default() -> ColorScheme {
        let light_colors = Colors {
            primary: PrimaryColors {
                foreground: Rgb::new(0x00, 0x00, 0x00),
                background: Rgb::new(0xff, 0xff, 0xff),
                bright_foreground: Default::default(),
                dim_foreground: Default::default(),
            },
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

        let dark_colors = Colors {
            primary: PrimaryColors {
                foreground: Rgb::new(0xff, 0xff, 0xff),
                background: Rgb::new(0x00, 0x00, 0x00),
                bright_foreground: Default::default(),
                dim_foreground: Default::default(),
            },
            normal: NormalColors {
                black: Rgb::new(0x32, 0x32, 0x32),
                red: Rgb::new(0xff, 0x80, 0x59),
                green: Rgb::new(0x44, 0xbc, 0x44),
                yellow: Rgb::new(0xd0, 0xbc, 0x00),
                blue: Rgb::new(0x2f, 0xaf, 0xff),
                magenta: Rgb::new(0xfe, 0xac, 0xd0),
                cyan: Rgb::new(0x00, 0xd3, 0xd0),
                white: Rgb::new(0xff, 0xff, 0xff),
            },
            bright: BrightColors {
                black: Rgb::new(0x53, 0x53, 0x53),
                red: Rgb::new(0xef, 0x8b, 0x50),
                green: Rgb::new(0x70, 0xb9, 0x00),
                yellow: Rgb::new(0xc0, 0xc5, 0x30),
                blue: Rgb::new(0x79, 0xa8, 0xff),
                magenta: Rgb::new(0xf7, 0x8f, 0xe7),
                cyan: Rgb::new(0x4a, 0xe2, 0xf0),
                white: Rgb::new(0xff, 0xff, 0xff),
            },
            ..Default::default()
        };

        ColorScheme { theme_variant: ThemeVariant::System, light_colors, dark_colors }
    }
}
