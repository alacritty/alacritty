use log::LevelFilter;
use serde::Deserializer;

use crate::config::failure_default;

/// Debugging options
#[serde(default)]
#[derive(Deserialize, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Debug {
    #[serde(default = "default_log_level", deserialize_with = "deserialize_log_level")]
    pub log_level: LevelFilter,

    #[serde(deserialize_with = "failure_default")]
    pub print_events: bool,

    /// Keep the log file after quitting
    #[serde(deserialize_with = "failure_default")]
    pub persistent_logging: bool,

    /// Should show render timer
    #[serde(deserialize_with = "failure_default")]
    pub render_timer: bool,

    /// Record ref test
    #[serde(deserialize_with = "failure_default")]
    pub ref_test: bool,
}

impl Default for Debug {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            print_events: Default::default(),
            persistent_logging: Default::default(),
            render_timer: Default::default(),
            ref_test: Default::default(),
        }
    }
}

fn default_log_level() -> LevelFilter {
    LevelFilter::Warn
}

fn deserialize_log_level<'a, D>(deserializer: D) -> Result<LevelFilter, D::Error>
where
    D: Deserializer<'a>,
{
    Ok(match failure_default::<D, String>(deserializer)?.to_lowercase().as_str() {
        "off" | "none" => LevelFilter::Off,
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        level => {
            error!("Problem with config: invalid log level {}; using level Warn", level);
            default_log_level()
        }
    })
}
