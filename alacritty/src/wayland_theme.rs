use alacritty_terminal::term::color::Rgb;
use glutin::platform::unix::{ButtonState, Theme as WaylandTheme};

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
    pub fn color_icon_color(&self, color: Rgb, status: ButtonState) -> [u8; 4] {
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
