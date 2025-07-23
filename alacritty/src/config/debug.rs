use log::LevelFilter;
use serde::Serialize;

use alacritty_config_derive::ConfigDeserialize;

/// Debugging options.
#[derive(ConfigDeserialize, Serialize, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Debug {
    pub log_level: LevelFilter,

    pub print_events: bool,

    /// Keep the log file after quitting.
    pub persistent_logging: bool,

    /// Should show render timer.
    pub render_timer: bool,

    /// Highlight damage information produced by alacritty.
    pub highlight_damage: bool,

    /// The renderer alacritty should be using.
    pub renderer: Option<RendererPreference>,

    /// Use EGL as display API if the current platform allows it.
    pub prefer_egl: bool,

    /// Record ref test.
    #[config(skip)]
    #[serde(skip_serializing)]
    pub ref_test: bool,
}

impl Default for Debug {
    fn default() -> Self {
        Self {
            log_level: LevelFilter::Warn,
            print_events: Default::default(),
            persistent_logging: Default::default(),
            render_timer: Default::default(),
            highlight_damage: Default::default(),
            ref_test: Default::default(),
            renderer: Default::default(),
            prefer_egl: Default::default(),
        }
    }
}

/// The renderer configuration options.
#[derive(ConfigDeserialize, Serialize, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RendererPreference {
    /// OpenGL 3.3 renderer.
    Glsl3,

    /// GLES 2 renderer, with optional extensions like dual source blending.
    Gles2,

    /// Pure GLES 2 renderer.
    Gles2Pure,
}
