//! ANSI Terminal Stream Parsing.

pub use vte::ansi::*;

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct CursorShapeShim(CursorShape);

impl Default for CursorShapeShim {
    fn default() -> CursorShapeShim {
        CursorShapeShim(CursorShape::Block)
    }
}

impl From<CursorShapeShim> for CursorShape {
    fn from(value: CursorShapeShim) -> Self {
        value.0
    }
}

struct CursorShapeVisitor;

impl<'de> serde::de::Visitor<'de> for CursorShapeVisitor {
    type Value = CursorShapeShim;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("one of `Block`, `Underline`, `Beam`")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match s.to_lowercase().as_str() {
            "block" => Ok(CursorShapeShim(CursorShape::Block)),
            "underline" => Ok(CursorShapeShim(CursorShape::Underline)),
            "beam" => Ok(CursorShapeShim(CursorShape::Beam)),
            _ => Err(E::custom(format!(
                "unknown variant `{0}`, expected {1}",
                s, "one of `Block`, `Underline`, `Beam`"
            ))),
        }
    }
}

impl<'de> serde::Deserialize<'de> for CursorShapeShim {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(CursorShapeVisitor)
    }
}

impl alacritty_config::SerdeReplace for CursorShapeShim {
    fn replace(&mut self, value: toml::Value) -> Result<(), Box<dyn std::error::Error>> {
        *self = serde::Deserialize::deserialize(value)?;

        Ok(())
    }
}
