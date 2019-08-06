//! Reading configuration from a yaml file
use log::*;
use serde_yaml;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
static DEFAULT_CHART_CONFIG: &'static str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/charts.yml"));
/// Top-level config type
#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct Config {
    pub charts: Vec<crate::TimeSeriesChart>,
}
impl Default for Config {
    fn default() -> Self {
        serde_yaml::from_str(DEFAULT_CHART_CONFIG).expect("default config is invalid")
    }
}
impl Config {
    /// This method is used from config/mod.rs in Alacritty.
    /// This is a copy for testing
    pub fn read_config(path: &PathBuf) -> Result<Config, String> {
        let mut contents = String::new();
        File::open(path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();

        // Prevent parsing error with empty string
        if contents.is_empty() {
            info!("Config file is empty, using defaults");
            return Ok(Config::default());
        }

        let config: Config = serde_yaml::from_str(&contents).unwrap();

        Ok(config)
    }

    /// `load_config_file` will return the loaded configuration. If the config is
    /// invalid it will return the default config
    pub fn load_config_file() -> Config {
        let config_location = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/charts.yml"));
        let config_res = Config::read_config(&config_location);
        match config_res {
            Err(err) => {
                error!(
                    "Unable to load config from file: {:?}: '{}'",
                    config_location, err
                );
                Config::default()
            }
            Ok(config) => {
                info!("load_config_file: {:?}", config_location);
                for chart in &config.charts {
                    debug!("load_config_file chart config with name: '{}'", chart.name);
                    for series in &chart.sources {
                        debug!(" - load_config_file series with name: '{}'", series.name());
                    }
                }
                debug!("Finished load_config_file");
                config
            }
        }
    }
}
