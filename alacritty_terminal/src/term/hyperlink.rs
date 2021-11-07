use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A cheap-to-clone, shared and immutable hyperlink information.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hyperlink {
    id: Id,
    #[serde(with = "serde_arc_str")]
    uri: Arc<str>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
enum Id {
    Number(usize),
    String(#[serde(with = "serde_arc_str")] Arc<str>),
}

impl Hyperlink {
    pub fn new_with_string_id(id: &str, uri: &str) -> Self {
        Self { id: Id::String(id.into()), uri: uri.into() }
    }

    pub fn new_with_numeric_id(id: usize, uri: &str) -> Self {
        Self { id: Id::Number(id), uri: uri.into() }
    }

    /// Get the URI of this hyperlink.
    pub fn uri(&self) -> &str {
        &*self.uri
    }
}

// Not sure why but `Arc<str>` doesn't implement Serialize/Deserialize by default.
mod serde_arc_str {
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(arc: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        <str>::serialize(&*arc, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
    where
        D: Deserializer<'de>,
    {
        <&str>::deserialize(deserializer).map(|s| s.into())
    }
}
