use std::fmt;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::row::Row;
    use crate::index::{Column, Line};

    // A test struct to verify the Swappable trait works with non-Row types
    #[derive(Clone, Debug, PartialEq)]
    struct TestSwappable {
        value: u32,
        secondary: String,
    }

    impl Swappable for TestSwappable {
        fn swap_with(&mut self, other: &mut Self) {
            mem::swap(self, other);
        }
    }

    // Create a test row with a specific character
    fn create_test_row(ch: char) -> Row<char> {
        let mut row = Row::new(1);
        row[Column(0)] = ch;
        row
    }

    #[test]
    fn test_swappable_row() {
        let mut row1 = create_test_row('a');
        let mut row2 = create_test_row('b');

        row1.swap_with(&mut row2);

        assert_eq!(row1[Column(0)], 'b');
        assert_eq!(row2[Column(0)], 'a');
    }

    #[test]
    fn test_storage_swap() {
        let mut storage = Storage::<char>::with_capacity(3, 1);

        storage[Line(0)] = create_test_row('0');
        storage[Line(1)] = create_test_row('1');

        storage.swap(Line(0), Line(1));

        assert_eq!(storage[Line(0)][Column(0)], '1');
        assert_eq!(storage[Line(1)][Column(0)], '0');
    }

    #[test]
    fn test_custom_swappable() {
        // This test demonstrates that the Swappable trait can work
        // with custom types that aren't Row<T>
        let mut a = TestSwappable { value: 1, secondary: "test".to_string() };
        let mut b = TestSwappable { value: 2, secondary: "other".to_string() };

        a.swap_with(&mut b);

        assert_eq!(a.value, 2);
        assert_eq!(a.secondary, "other");
        assert_eq!(b.value, 1);
        assert_eq!(b.secondary, "test");
    }
}
use super::*;
use crate::grid::row::Row;
use crate::index::Line;
#[cfg(test)]
mod swappable_tests {
    use super::*;
    use crate::grid::row::Row;
    use crate::index::{Column, Line};

    // Helper function to create a test row
    fn create_test_row(ch: char) -> Row<char> {
        let mut row = Row::new(1);
        row[Column(0)] = ch;
        row
    }

    #[test]
    fn test_row_swap_with() {
        let mut row1 = create_test_row('a');
        let mut row2 = create_test_row('b');

        row1.swap_with(&mut row2);

        assert_eq!(row1[Column(0)], 'b');
        assert_eq!(row2[Column(0)], 'a');
    }

    #[test]
    fn test_storage_swap() {
        let mut storage = Storage::<char>::with_capacity(3, 1);

        storage[Line(0)] = create_test_row('0');
        storage[Line(1)] = create_test_row('1');

        storage.swap(Line(0), Line(1));

        assert_eq!(storage[Line(0)][Column(0)], '1');
        assert_eq!(storage[Line(1)][Column(0)], '0');
    }

    #[test]
    fn test_swap_same_index() {
        let mut storage = Storage::<char>::with_capacity(2, 1);

        storage[Line(0)] = create_test_row('0');

        // This should not panic
        storage.swap(Line(0), Line(0));

        assert_eq!(storage[Line(0)][Column(0)], '0');
    }
}
// Simple test cell implementation
#[derive(Clone, Debug, PartialEq)]
struct TestCell {
    value: char,
}

impl TestCell {
    fn new(value: char) -> Self {
        Self { value }
    }
}

impl Default for TestCell {
    fn default() -> Self {
        Self { value: ' ' }
    }
}

// Test Swappable implementation for non-Row types
#[derive(Clone, Debug, PartialEq)]
struct TestSwappable {
    value: usize,
}

impl Swappable for TestSwappable {
    fn swap_with(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.value, &mut other.value);
    }
}

// Create a Row with TestCell values
fn create_test_row(ch: char, width: usize) -> Row<TestCell> {
    let mut cells = Vec::with_capacity(width);
    for _ in 0..width {
        cells.push(TestCell::new(ch));
    }
    Row::new(cells)
}

// Test that the generic swap functionality works with Row<TestCell>
#[test]
fn test_row_swap() {
    let mut storage = Storage::with_capacity(5, 10);
    let row_a = create_test_row('a', 10);
    let row_b = create_test_row('b', 10);

    // Replace rows in storage
    storage.inner[0] = row_a.clone();
    storage.inner[1] = row_b.clone();

    // Swap rows
    storage.swap(Line(0), Line(1));

    // Verify swap was successful
    assert_eq!(storage.inner[0].cells[0].value, 'b');
    assert_eq!(storage.inner[1].cells[0].value, 'a');
}

// Test storage with a completely different Swappable type
// This validates that Storage is truly generic
#[test]
fn test_with_custom_swappable() {
    // This test demonstrates how we might implement a Storage
    // for a completely different type in the future
    struct CustomStorage<T: Swappable> {
        items: Vec<T>,
    }

    impl<T: Swappable> CustomStorage<T> {
        fn new() -> Self {
            Self { items: Vec::new() }
        }

        fn push(&mut self, item: T) {
            self.items.push(item);
        }

        fn swap(&mut self, a: usize, b: usize) {
            self.items[a].swap_with(&mut self.items[b]);
        }
    }

    // Create a storage with TestSwappable
    let mut storage = CustomStorage::new();
    storage.push(TestSwappable { value: 1 });
    storage.push(TestSwappable { value: 2 });

    // Swap and verify
    storage.swap(0, 1);
    assert_eq!(storage.items[0].value, 2);
    assert_eq!(storage.items[1].value, 1);
}
