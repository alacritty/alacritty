//! Serde helpers.

use toml::{Table, Value};

/// Merge two serde structures.
///
/// This will take all values from `replacement` and use `base` whenever a value isn't present in
/// `replacement`.
pub fn merge(base: Value, replacement: Value) -> Value {
    match (base, replacement) {
        (Value::Array(mut base), Value::Array(mut replacement)) => {
            base.append(&mut replacement);
            Value::Array(base)
        },
        (Value::Table(base), Value::Table(replacement)) => {
            Value::Table(merge_tables(base, replacement))
        },
        (_, value) => value,
    }
}

/// Merge two key/value tables.
fn merge_tables(mut base: Table, replacement: Table) -> Table {
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
        let base = Value::Table(Table::new());
        let replacement = Value::Boolean(true);
        assert_eq!(merge(base, replacement.clone()), replacement);

        let base = Value::Boolean(false);
        let replacement = Value::Boolean(true);
        assert_eq!(merge(base, replacement.clone()), replacement);

        let base = Value::Integer(0.into());
        let replacement = Value::Integer(1.into());
        assert_eq!(merge(base, replacement.clone()), replacement);

        let base = Value::String(String::new());
        let replacement = Value::String(String::from("test"));
        assert_eq!(merge(base, replacement.clone()), replacement);

        let base = Value::Table(Table::new());
        let replacement = Value::Table(Table::new());
        assert_eq!(merge(base.clone(), replacement), base);
    }

    #[test]
    fn merge_sequence() {
        let base = Value::Array(vec![Value::Table(Table::new())]);
        let replacement = Value::Array(vec![Value::Boolean(true)]);
        let expected = Value::Array(vec![Value::Table(Table::new()), Value::Boolean(true)]);
        assert_eq!(merge(base, replacement), expected);
    }

    #[test]
    fn merge_tables() {
        let mut base_table = Table::new();
        base_table.insert(String::from("a"), Value::Boolean(true));
        base_table.insert(String::from("b"), Value::Boolean(false));
        let base = Value::Table(base_table);

        let mut replacement_table = Table::new();
        replacement_table.insert(String::from("a"), Value::Boolean(true));
        replacement_table.insert(String::from("c"), Value::Boolean(false));
        let replacement = Value::Table(replacement_table);

        let merged = merge(base, replacement);

        let mut expected_table = Table::new();
        expected_table.insert(String::from("b"), Value::Boolean(false));
        expected_table.insert(String::from("a"), Value::Boolean(true));
        expected_table.insert(String::from("c"), Value::Boolean(false));
        let expected = Value::Table(expected_table);

        assert_eq!(merged, expected);
    }
}
