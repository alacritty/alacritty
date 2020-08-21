//! Serde helpers.

use serde_yaml::mapping::Mapping;
use serde_yaml::Value;

/// Merge two serde structures.
///
/// This will take all values from `replacement` and use `base` whenever a value isn't present in
/// `replacement`.
pub fn merge(base: Value, replacement: Value) -> Value {
    match (base, replacement) {
        (Value::Sequence(mut base), Value::Sequence(mut replacement)) => {
            base.append(&mut replacement);
            Value::Sequence(base)
        },
        (Value::Mapping(base), Value::Mapping(replacement)) => {
            Value::Mapping(merge_mapping(base, replacement))
        },
        (value, Value::Null) => value,
        (_, value) => value,
    }
}

/// Merge two key/value mappings.
fn merge_mapping(mut base: Mapping, replacement: Mapping) -> Mapping {
    for (key, value) in replacement {
        let value = match base.remove(&key) {
            Some(base_value) => merge(base_value, value),
            None => value,
        };
        base.insert(key, value);
    }

    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_primitive() {
        let base = Value::Null;
        let replacement = Value::Bool(true);
        assert_eq!(merge(base, replacement.clone()), replacement);

        let base = Value::Bool(false);
        let replacement = Value::Bool(true);
        assert_eq!(merge(base, replacement.clone()), replacement);

        let base = Value::Number(0.into());
        let replacement = Value::Number(1.into());
        assert_eq!(merge(base, replacement.clone()), replacement);

        let base = Value::String(String::new());
        let replacement = Value::String(String::from("test"));
        assert_eq!(merge(base, replacement.clone()), replacement);

        let base = Value::Mapping(Mapping::new());
        let replacement = Value::Null;
        assert_eq!(merge(base.clone(), replacement), base);
    }

    #[test]
    fn merge_sequence() {
        let base = Value::Sequence(vec![Value::Null]);
        let replacement = Value::Sequence(vec![Value::Bool(true)]);
        let expected = Value::Sequence(vec![Value::Null, Value::Bool(true)]);
        assert_eq!(merge(base, replacement), expected);
    }

    #[test]
    fn merge_mapping() {
        let mut base_mapping = Mapping::new();
        base_mapping.insert(Value::String(String::from("a")), Value::Bool(true));
        base_mapping.insert(Value::String(String::from("b")), Value::Bool(false));
        let base = Value::Mapping(base_mapping);

        let mut replacement_mapping = Mapping::new();
        replacement_mapping.insert(Value::String(String::from("a")), Value::Bool(true));
        replacement_mapping.insert(Value::String(String::from("c")), Value::Bool(false));
        let replacement = Value::Mapping(replacement_mapping);

        let merged = merge(base, replacement);

        let mut expected_mapping = Mapping::new();
        expected_mapping.insert(Value::String(String::from("b")), Value::Bool(false));
        expected_mapping.insert(Value::String(String::from("a")), Value::Bool(true));
        expected_mapping.insert(Value::String(String::from("c")), Value::Bool(false));
        let expected = Value::Mapping(expected_mapping);

        assert_eq!(merged, expected);
    }
}
