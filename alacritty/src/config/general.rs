//! Miscellaneous configuration options.

use std::fmt;
use std::path::PathBuf;

use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

/// General config section.
///
/// This section is for fields which can not be easily categorized,
/// to avoid common TOML issues with root-level fields.
#[derive(ConfigDeserialize, Serialize, Clone, PartialEq, Debug)]
pub struct General {
    /// Configuration file imports.
    ///
    /// This is never read since the field is directly accessed through the config's
    /// [`toml::Value`], but still present to prevent unused field warnings.
    pub import: Vec<String>,

    /// Shell startup directory.
    pub working_directory: Option<PathBuf>,

    /// Live config reload.
    pub live_config_reload: bool,

    /// Offer IPC through a unix socket.
    #[allow(unused)]
    pub ipc_socket: bool,

    /// How long to wait for memory when an allocation fails.
    pub memory_wait: MemoryWait,
}

impl Default for General {
    fn default() -> Self {
        Self {
            live_config_reload: true,
            ipc_socket: true,
            working_directory: Default::default(),
            import: Default::default(),
            memory_wait: Default::default(),
        }
    }
}

/// How long to wait for memory when an allocation fails.
#[derive(SerdeReplace, Copy, Clone, PartialEq, Eq, Debug)]
pub enum MemoryWait {
    /// Wait up to this many seconds, then let the allocation fail.
    Seconds(u64),
    /// Wait until memory is available.
    Forever,
}

impl MemoryWait {
    pub const DEFAULT: Self = Self::Seconds(300);
}

impl Default for MemoryWait {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'de> Deserialize<'de> for MemoryWait {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MemoryWaitVisitor;

        impl Visitor<'_> for MemoryWaitVisitor {
            type Value = MemoryWait;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a number of seconds or Forever")
            }

            fn visit_i64<E>(self, value: i64) -> Result<MemoryWait, E>
            where
                E: de::Error,
            {
                match u64::try_from(value) {
                    Ok(seconds) => Ok(MemoryWait::Seconds(seconds)),
                    Err(_) => Err(E::invalid_value(Unexpected::Signed(value), &self)),
                }
            }

            fn visit_u64<E>(self, value: u64) -> Result<MemoryWait, E>
            where
                E: de::Error,
            {
                Ok(MemoryWait::Seconds(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<MemoryWait, E>
            where
                E: de::Error,
            {
                match value {
                    "Forever" => Ok(MemoryWait::Forever),
                    _ => Err(E::invalid_value(Unexpected::Str(value), &self)),
                }
            }
        }

        deserializer.deserialize_any(MemoryWaitVisitor)
    }
}

impl Serialize for MemoryWait {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Seconds(seconds) => serializer.serialize_u64(*seconds),
            Self::Forever => serializer.serialize_str("Forever"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use toml::Value;

    #[test]
    fn deserialize_memory_wait() {
        assert_eq!(MemoryWait::deserialize(Value::Integer(30)).unwrap(), MemoryWait::Seconds(30));
        assert_eq!(MemoryWait::deserialize(Value::Integer(0)).unwrap(), MemoryWait::Seconds(0));
        assert_eq!(
            MemoryWait::deserialize(Value::String("Forever".into())).unwrap(),
            MemoryWait::Forever
        );

        assert!(MemoryWait::deserialize(Value::Integer(-1)).is_err());
        assert!(MemoryWait::deserialize(Value::String("Never".into())).is_err());
    }

    #[test]
    fn memory_wait_in_config() {
        let general: General = toml::from_str("memory_wait = \"Forever\"").unwrap();
        assert_eq!(general.memory_wait, MemoryWait::Forever);

        let general: General = toml::from_str("").unwrap();
        assert_eq!(general.memory_wait, MemoryWait::DEFAULT);
    }
}
