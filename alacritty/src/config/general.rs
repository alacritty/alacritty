use std::path::PathBuf;

use alacritty_config_derive::ConfigDeserialize;

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct General {
    /// Offer IPC through a unix socket.
    #[cfg(unix)]
    pub ipc_socket: bool,

    /// Live config reload.
    pub live_config_reload: bool,

    /// Shell startup directory.
    pub working_directory: Option<PathBuf>,

    /// Configuration file imports.
    ///
    /// This is never read since the field is directly accessed through the config's
    /// [`toml::Value`], but still present to prevent unused field warnings.
    import: Vec<String>,
}

impl Default for General {
    fn default() -> Self {
        Self {
            #[cfg(unix)]
            ipc_socket: true,
            live_config_reload: true,
            import: Default::default(),
            working_directory: Default::default(),
        }
    }
}
