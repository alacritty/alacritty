use glutin::platform::unix::{ARGBColor, Button, ButtonState, Element, Theme as WaylandTheme};

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
        let foreground = colors.search_bar_foreground();
        let background = colors.search_bar_background();
        // TODO decide how to dim properly, maybe we can derive it from the foreground color
        // with our factor and some formula.
        let dim_foreground = foreground * DIM_FACTOR;

        Self {
            foreground,
            background,
            dim_foreground,
            hovered_close_icon,
            hovered_minimize_icon,
            hovered_maximize_icon,
        }
    }

    fn color_icon_color(&self, color: Rgb, status: ButtonState, window_active: bool) -> Rgb {
        if window_active {
            match status {
                ButtonState::Hovered => color,
                ButtonState::Idle => self.foreground,
                ButtonState::Disabled => self.dim_foreground,
            }
        } else {
            self.dim_foreground
        }
    }
}

impl WaylandTheme for AlacrittyWaylandTheme {
    fn element_color(&self, element: Element, window_active: bool) -> ARGBColor {
        let Rgb { r, g, b } = match element {
            Element::Bar => self.background,
            Element::Separator => self.background,
            Element::Text => {
                if window_active {
                    self.foreground
                } else {
                    self.dim_foreground
                }
            },
        };

        ARGBColor { a: 0xff, r, g, b }
    }

    fn button_color(
        &self,
        button: Button,
        state: ButtonState,
        foreground: bool,
        window_active: bool,
    ) -> ARGBColor {
        let (a, Rgb { r, g, b }) = if foreground {
            let color = match button {
                Button::Minimize => {
                    self.color_icon_color(self.hovered_minimize_icon, state, window_active)
                },
                Button::Maximize => {
                    self.color_icon_color(self.hovered_maximize_icon, state, window_active)
                },
                Button::Close => {
                    self.color_icon_color(self.hovered_close_icon, state, window_active)
                },
            };

            (0xff, color)
        } else {
            (0x00, self.background)
        };

        ARGBColor { a, r, g, b }
    }
}
