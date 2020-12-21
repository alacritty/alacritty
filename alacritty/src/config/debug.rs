use log::LevelFilter;

use alacritty_config_derive::ConfigDeserialize;

/// Debugging options.
#[derive(ConfigDeserialize, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Debug {
    pub log_level: LevelFilter,

    pub print_events: bool,

    /// Keep the log file after quitting.
    pub persistent_logging: bool,

    /// Should show render timer.
    pub render_timer: bool,

    /// Record ref test.
    #[config(skip)]
    pub ref_test: bool,
}

impl Default for Debug {
    fn default() -> Self {
        Self {
            log_level: LevelFilter::Warn,
            print_events: Default::default(),
            persistent_logging: Default::default(),
            render_timer: Default::default(),
            ref_test: Default::default(),
        }
    }
}
