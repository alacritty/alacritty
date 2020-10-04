use glutin::platform::unix::{ARGBColor, Button, ButtonState, Element, Theme as WaylandTheme};

use alacritty_terminal::config::Colors;
use alacritty_terminal::term::color::Rgb;

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

        // Blend background and foreground. We use 0.5 to make color look 'equally' with both light
        // and dark themes.
        let dim_foreground = foreground * 0.5 + background * 0.5;

        Self {
            foreground,
            background,
            dim_foreground,
            hovered_close_icon,
            hovered_minimize_icon,
            hovered_maximize_icon,
        }
    }

    fn button_foreground_color(&self, color: Rgb, status: ButtonState, window_active: bool) -> Rgb {
        match (window_active, status) {
            (false, _) => self.dim_foreground,
            (_, ButtonState::Hovered) => color,
            (_, ButtonState::Idle) => self.foreground,
            (_, ButtonState::Disabled) => self.dim_foreground,
        }
    }
}

impl WaylandTheme for AlacrittyWaylandTheme {
    fn element_color(&self, element: Element, window_active: bool) -> ARGBColor {
        match element {
            Element::Bar | Element::Separator => self.background.into_rgba(),
            Element::Text if window_active => self.foreground.into_rgba(),
            Element::Text => self.dim_foreground.into_rgba(),
        }
    }

    fn button_color(
        &self,
        button: Button,
        state: ButtonState,
        foreground: bool,
        window_active: bool,
    ) -> ARGBColor {
        match (foreground, button) {
            (false, _) => ARGBColor { a: 0x00, r: 0x00, g: 0x00, b: 0x00 },
            (_, Button::Minimize) => self
                .button_foreground_color(self.hovered_minimize_icon, state, window_active)
                .into_rgba(),
            (_, Button::Maximize) => self
                .button_foreground_color(self.hovered_maximize_icon, state, window_active)
                .into_rgba(),
            (_, Button::Close) => self
                .button_foreground_color(self.hovered_close_icon, state, window_active)
                .into_rgba(),
        }
    }
}

trait IntoARGBColor {
    fn into_rgba(self) -> ARGBColor;
}

impl IntoARGBColor for Rgb {
    fn into_rgba(self) -> ARGBColor {
        ARGBColor { a: 0xff, r: self.r, g: self.g, b: self.b }
    }
}
