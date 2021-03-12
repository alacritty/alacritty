use glutin::platform::unix::{ARGBColor, Button, ButtonState, Element, Theme as WaylandTheme};

use alacritty_terminal::term::color::Rgb;

use crate::config::color::Colors;

const INACTIVE_OPACITY: u8 = 127;

#[derive(Debug, Clone)]
pub struct AlacrittyWaylandTheme {
    pub foreground: ARGBColor,
    pub background: ARGBColor,
    pub dim_foreground: ARGBColor,
    pub hovered_close_icon: ARGBColor,
    pub hovered_maximize_icon: ARGBColor,
    pub hovered_minimize_icon: ARGBColor,
}

impl AlacrittyWaylandTheme {
    pub fn new(colors: &Colors) -> Self {
        let hovered_close_icon = colors.normal.red.into_rgba();
        let hovered_maximize_icon = colors.normal.green.into_rgba();
        let hovered_minimize_icon = colors.normal.yellow.into_rgba();
        let foreground = colors.search_bar_foreground().into_rgba();
        let background = colors.search_bar_background().into_rgba();

        let mut dim_foreground = foreground;
        dim_foreground.a = INACTIVE_OPACITY;

        Self {
            foreground,
            background,
            dim_foreground,
            hovered_close_icon,
            hovered_maximize_icon,
            hovered_minimize_icon,
        }
    }
}

impl WaylandTheme for AlacrittyWaylandTheme {
    fn element_color(&self, element: Element, window_active: bool) -> ARGBColor {
        match element {
            Element::Bar | Element::Separator => self.background,
            Element::Text if window_active => self.foreground,
            Element::Text => self.dim_foreground,
        }
    }

    fn button_color(
        &self,
        button: Button,
        state: ButtonState,
        foreground: bool,
        window_active: bool,
    ) -> ARGBColor {
        if !foreground {
            return ARGBColor { a: 0, r: 0, g: 0, b: 0 };
        } else if !window_active {
            return self.dim_foreground;
        }

        match (state, button) {
            (ButtonState::Idle, _) => self.foreground,
            (ButtonState::Disabled, _) => self.dim_foreground,
            (_, Button::Minimize) => self.hovered_minimize_icon,
            (_, Button::Maximize) => self.hovered_maximize_icon,
            (_, Button::Close) => self.hovered_close_icon,
        }
    }
}

trait IntoArgbColor {
    fn into_rgba(self) -> ARGBColor;
}

impl IntoArgbColor for Rgb {
    fn into_rgba(self) -> ARGBColor {
        ARGBColor { a: 0xff, r: self.r, g: self.g, b: self.b }
    }
}
