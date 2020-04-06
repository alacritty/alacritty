use glutin::platform::unix::{ButtonState, Theme as WaylandTheme};

use alacritty_terminal::config::Colors;
use alacritty_terminal::term::color::{Rgb, DIM_FACTOR};

#[derive(Debug, Clone)]
pub struct AlacrittyWaylandTheme {
    pub background: Rgb,
    pub foreground: Rgb,
    pub dim_foreground: Rgb,
    pub hovered_close_icon: Rgb,
    pub hovered_maximize_icon: Rgb,
    pub hovered_minimize_icon: Rgb,
}

impl AlacrittyWaylandTheme {
    pub fn new(colors: &Colors) -> Self {
        let hovered_close_icon = colors.normal().red;
        let hovered_maximize_icon = colors.normal().green;
        let hovered_minimize_icon = colors.normal().yellow;
        let foreground = colors.primary.foreground;
        let background = colors.primary.background;
        let dim_foreground = colors.primary.dim_foreground.unwrap_or(foreground * DIM_FACTOR);

        Self {
            foreground,
            background,
            dim_foreground,
            hovered_close_icon,
            hovered_minimize_icon,
            hovered_maximize_icon,
        }
    }

    fn color_icon_color(&self, color: Rgb, status: ButtonState) -> [u8; 4] {
        match status {
            ButtonState::Hovered => [0xff, color.r, color.g, color.b],
            ButtonState::Idle => [0xff, self.foreground.r, self.foreground.g, self.foreground.b],
            ButtonState::Disabled => {
                [0xff, self.dim_foreground.r, self.dim_foreground.g, self.dim_foreground.b]
            },
        }
    }
}

impl WaylandTheme for AlacrittyWaylandTheme {
    fn primary_color(&self, _window_active: bool) -> [u8; 4] {
        [0xff, self.background.r, self.background.g, self.background.b]
    }

    fn secondary_color(&self, window_active: bool) -> [u8; 4] {
        if window_active {
            [0xff, self.foreground.r, self.foreground.g, self.foreground.b]
        } else {
            [0xff, self.dim_foreground.r, self.dim_foreground.g, self.dim_foreground.b]
        }
    }

    fn close_button_color(&self, _status: ButtonState) -> [u8; 4] {
        [0x00, self.background.r, self.background.g, self.background.b]
    }

    fn close_button_icon_color(&self, status: ButtonState) -> [u8; 4] {
        self.color_icon_color(self.hovered_close_icon, status)
    }

    fn maximize_button_color(&self, _status: ButtonState) -> [u8; 4] {
        [0x00, self.background.r, self.background.g, self.background.b]
    }

    fn maximize_button_icon_color(&self, status: ButtonState) -> [u8; 4] {
        self.color_icon_color(self.hovered_maximize_icon, status)
    }

    fn minimize_button_color(&self, _status: ButtonState) -> [u8; 4] {
        [0x00, self.background.r, self.background.g, self.background.b]
    }

    fn minimize_button_icon_color(&self, status: ButtonState) -> [u8; 4] {
        self.color_icon_color(self.hovered_minimize_icon, status)
    }
}
