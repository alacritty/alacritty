use std::time::Duration;

use log::error;
use serde::{self, Deserialize, Deserializer};

use alacritty_config_derive::ConfigDeserialize;

use alacritty_terminal::config::Program;
use alacritty_terminal::index::Point;
use alacritty_terminal::term::{color::Rgb, SizeInfo};

#[derive(Clone, Copy, Debug, PartialEq)]
struct BellRadii {
    /// Horizontal radius in units of cells.
    pub horizontal: f32,
    /// Vertical radius in units of cells.
    pub vertical: f32,
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BellRadii {
    pub fn rect_around(&self, point: Point, size_info: SizeInfo) -> Rect {
        let point_column = point.column.0 as f32;
        let point_row = point.line.0 as f32;
        let bell_column = point_column - self.horizontal;
        let bell_row = point_row - self.vertical;
        let bell_width = point_column + self.horizontal * 2. - bell_column;
        let bell_height = point_row + self.vertical * 2. - bell_row;
        Rect {
            x: bell_column * size_info.cell_width(),
            y: bell_row * size_info.cell_height(),
            width: bell_width * size_info.cell_width(),
            height: bell_height * size_info.cell_height(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OptionalBellRadii(Option<BellRadii>);

impl OptionalBellRadii {
    pub fn rect_around(&self, point: Point, size_info: SizeInfo) -> Rect {
        match self.0 {
            Some(radii) => radii.rect_around(point, size_info),
            None => Rect { x: 0., y: 0., width: size_info.width(), height: size_info.height() },
        }
    }
}

impl From<Option<(f32, f32)>> for OptionalBellRadii {
    fn from(option: Option<(f32, f32)>) -> Self {
        match option {
            Some((horizontal, vertical)) => Self(Some(BellRadii { horizontal, vertical })),
            None => Self(None),
        }
    }
}

impl<'de> Deserialize<'de> for OptionalBellRadii {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(<(f32, f32)>::deserialize(deserializer)
            .and_then(|radius| {
                if radius == (-1., -1.) {
                    Ok(None)
                } else {
                    for radius in [radius.0, radius.1] {
                        if radius < 0. {
                            return Err(::serde::de::Error::custom(
                                "expected positive numbers or (-1, -1)",
                            ));
                        }
                    }
                    Ok(Some(radius))
                }
            })
            .unwrap_or_else(|err| {
                error!("Problem with config: {}; using (-1, -1)", err);
                None
            })
            .into())
    }
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq)]
pub struct BellConfig {
    /// Visual bell animation function.
    pub animation: BellAnimation,

    /// Command to run on bell.
    pub command: Option<Program>,

    /// Visual bell flash color.
    pub color: Rgb,

    /// Visual bell duration in milliseconds.
    duration: u16,

    /// Visual bell flash radii around cursor
    ///
    /// `(-1., -1.)` indicates the whole drawing area should flash
    pub cursor_radii: OptionalBellRadii,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self {
            color: Rgb { r: 255, g: 255, b: 255 },
            animation: Default::default(),
            command: Default::default(),
            duration: Default::default(),
            cursor_radii: Default::default(),
        }
    }
}

impl BellConfig {
    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration as u64)
    }
}

/// `VisualBellAnimations` are modeled after a subset of CSS transitions and Robert
/// Penner's Easing Functions.
#[derive(ConfigDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BellAnimation {
    // CSS animation.
    Ease,
    // CSS animation.
    EaseOut,
    // Penner animation.
    EaseOutSine,
    // Penner animation.
    EaseOutQuad,
    // Penner animation.
    EaseOutCubic,
    // Penner animation.
    EaseOutQuart,
    // Penner animation.
    EaseOutQuint,
    // Penner animation.
    EaseOutExpo,
    // Penner animation.
    EaseOutCirc,
    // Penner animation.
    Linear,
}

impl Default for BellAnimation {
    fn default() -> Self {
        BellAnimation::EaseOutExpo
    }
}
