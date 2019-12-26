use log::error;
use regex::bytes::Regex;
use serde::{Deserialize, Deserializer};

use alacritty_terminal::config::{failure_default, LOG_TARGET_CONFIG};

#[serde(default)]
#[derive(Clone, Debug, Deserialize)]
pub struct TextObject {
    #[serde(deserialize_with = "deserialize_regex")]
    pub search: Option<Regex>,
    #[serde(deserialize_with = "failure_default")]
    pub action: Vec<String>,
    #[serde(deserialize_with = "failure_default")]
    pub priority: usize,
}

impl Default for TextObject {
    fn default() -> Self {
        TextObject { search: None, action: Vec::new(), priority: 0 }
    }
}

impl PartialEq for TextObject {
    // Simple equality for Regex so we add them to containers
    fn eq(&self, other: &Self) -> bool {
        self.action == other.action
            && self.priority == other.priority
            && self.search.as_ref().map(|v| v.as_str()) == other.search.as_ref().map(|v| v.as_str())
    }
}

fn deserialize_regex<'a, D>(deserializer: D) -> Result<Option<Regex>, D::Error>
where
    D: Deserializer<'a>,
{
    let raw = serde_yaml::Value::deserialize(deserializer)?;
    if let Some(val) = raw.as_str() {
        if let Ok(re) = Regex::new(val) {
            return Ok(Some(re));
        }
    }
    error!(target: LOG_TARGET_CONFIG, "Problem with text-object: {:?}; disabled", raw);
    Ok(None)
}
