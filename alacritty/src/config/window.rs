use std::fmt::{self, Formatter};
use std::os::raw::c_ulong;

use log::{error, warn};
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use winit::window::{Fullscreen, Theme};

#[cfg(target_os = "macos")]
use winit::platform::macos::OptionAsAlt;

use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};
use alacritty_terminal::config::{Percentage, LOG_TARGET_CONFIG};
use alacritty_terminal::index::Column;

use crate::config::ui_config::Delta;

/// Default Alacritty name, used for window title and class.
pub const DEFAULT_NAME: &str = "Alacritty";

#[derive(ConfigDeserialize, Debug, Clone, PartialEq)]
pub struct WindowConfig {
    /// Initial position.
    pub position: Option<Delta<i32>>,

    /// Draw the window with title bar / borders.
    pub decorations: Decorations,

    /// Startup mode.
    pub startup_mode: StartupMode,

    /// XEmbed parent.
    #[config(skip)]
    pub embed: Option<c_ulong>,

    /// System decorations theme variant.
    pub decorations_theme_variant: Option<Theme>,

    /// Spread out additional padding evenly.
    pub dynamic_padding: bool,

    /// Use dynamic title.
    pub dynamic_title: bool,

    /// Information to identify a particular window.
    #[config(flatten)]
    pub identity: Identity,

    /// Background opacity from 0.0 to 1.0.
    pub opacity: Percentage,

    /// Controls which `Option` key should be treated as `Alt`.
    #[cfg(target_os = "macos")]
    pub option_as_alt: OptionAsAlt,

    /// Resize increments.
    pub resize_increments: bool,

    /// Pixel padding.
    padding: Delta<u8>,

    /// Initial dimensions.
    dimensions: Dimensions,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            dynamic_title: true,
            position: Default::default(),
            decorations: Default::default(),
            startup_mode: Default::default(),
            embed: Default::default(),
            decorations_theme_variant: Default::default(),
            dynamic_padding: Default::default(),
            identity: Identity::default(),
            opacity: Default::default(),
            padding: Default::default(),
            dimensions: Default::default(),
            resize_increments: Default::default(),
            #[cfg(target_os = "macos")]
            option_as_alt: Default::default(),
        }
    }
}

impl WindowConfig {
    #[inline]
    pub fn dimensions(&self) -> Option<Dimensions> {
        let (lines, columns) = (self.dimensions.lines, self.dimensions.columns.0);
        let (lines_is_non_zero, columns_is_non_zero) = (lines != 0, columns != 0);

        if lines_is_non_zero && columns_is_non_zero {
            // Return dimensions if both `lines` and `columns` are non-zero.
            Some(self.dimensions)
        } else if lines_is_non_zero || columns_is_non_zero {
            // Warn if either `columns` or `lines` is non-zero.

            let (zero_key, non_zero_key, non_zero_value) = if lines_is_non_zero {
                ("columns", "lines", lines)
            } else {
                ("lines", "columns", columns)
            };

            warn!(
                target: LOG_TARGET_CONFIG,
                "Both `lines` and `columns` must be non-zero for `window.dimensions` to take \
                 effect. Configured value of `{}` is 0 while that of `{}` is {}",
                zero_key,
                non_zero_key,
                non_zero_value,
            );

            None
        } else {
            None
        }
    }

    #[inline]
    pub fn padding(&self, scale_factor: f32) -> (f32, f32) {
        let padding_x = (f32::from(self.padding.x) * scale_factor).floor();
        let padding_y = (f32::from(self.padding.y) * scale_factor).floor();
        (padding_x, padding_y)
    }

    #[inline]
    pub fn fullscreen(&self) -> Option<Fullscreen> {
        if self.startup_mode == StartupMode::Fullscreen {
            Some(Fullscreen::Borderless(None))
        } else {
            None
        }
    }

    #[inline]
    pub fn maximized(&self) -> bool {
        self.startup_mode == StartupMode::Maximized
    }
}

#[derive(ConfigDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Window title.
    pub title: String,

    /// Window class.
    pub class: Class,
}

impl Default for Identity {
    fn default() -> Self {
        Self { title: DEFAULT_NAME.into(), class: Default::default() }
    }
}

#[derive(ConfigDeserialize, Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum StartupMode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
    #[cfg(target_os = "macos")]
    SimpleFullscreen,
}

#[derive(ConfigDeserialize, Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Decorations {
    #[default]
    Full,
    #[cfg(target_os = "macos")]
    Transparent,
    #[cfg(target_os = "macos")]
    Buttonless,
    None,
}

/// Window Dimensions.
///
/// Newtype to avoid passing values incorrectly.
#[derive(ConfigDeserialize, Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Dimensions {
    /// Window width in character columns.
    pub columns: Column,

    /// Window Height in character lines.
    pub lines: usize,
}

/// Window class hint.
#[derive(SerdeReplace, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub general: String,
    pub instance: String,
}

impl Class {
    pub fn new(general: impl ToString, instance: impl ToString) -> Self {
        Self { general: general.to_string(), instance: instance.to_string() }
    }
}

impl Default for Class {
    fn default() -> Self {
        Self::new(DEFAULT_NAME, DEFAULT_NAME)
    }
}

impl<'de> Deserialize<'de> for Class {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ClassVisitor;
        impl<'a> Visitor<'a> for ClassVisitor {
            type Value = Class;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a mapping")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value { instance: value.into(), ..Self::Value::default() })
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'a>,
            {
                let mut class = Self::Value::default();

                while let Some((key, value)) = map.next_entry::<String, serde_yaml::Value>()? {
                    match key.as_str() {
                        "instance" => match String::deserialize(value) {
                            Ok(instance) => class.instance = instance,
                            Err(err) => {
                                error!(
                                    target: LOG_TARGET_CONFIG,
                                    "Config error: class.instance: {}", err
                                );
                            },
                        },
                        "general" => match String::deserialize(value) {
                            Ok(general) => class.general = general,
                            Err(err) => {
                                error!(
                                    target: LOG_TARGET_CONFIG,
                                    "Config error: class.instance: {}", err
                                );
                            },
                        },
                        _ => (),
                    }
                }

                Ok(class)
            }
        }

        deserializer.deserialize_any(ClassVisitor)
    }
}
