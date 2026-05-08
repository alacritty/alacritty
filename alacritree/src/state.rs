//! Persists the sidebar across restarts at `$XDG_CONFIG_HOME/alacritree/state.toml`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    #[serde(default)]
    pub projects: Vec<PersistedProject>,
    #[serde(default = "default_true")]
    pub show_left_sidebar: bool,
    #[serde(default = "default_true")]
    pub show_right_sidebar: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedProject {
    pub root: PathBuf,
    #[serde(default = "default_true")]
    pub expanded: bool,
}

fn default_true() -> bool {
    true
}

pub fn config_path() -> Option<PathBuf> {
    let base = if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        let home = std::env::var_os("HOME")?;
        PathBuf::from(home).join(".config")
    };
    Some(base.join("alacritree").join("state.toml"))
}

pub fn load() -> PersistedState {
    let Some(path) = config_path() else {
        return PersistedState::default();
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return PersistedState::default();
    };
    match toml::from_str(&contents) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("failed to parse {}: {e}", path.display());
            PersistedState::default()
        },
    }
}

pub fn save(state: &PersistedState) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("failed to create {}: {e}", parent.display());
            return;
        }
    }
    let body = match toml::to_string_pretty(state) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("failed to serialize state: {e}");
            return;
        },
    };
    if let Err(e) = std::fs::write(&path, body) {
        log::warn!("failed to write {}: {e}", path.display());
    }
}
