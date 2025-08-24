use std::fmt;

use super::*;
use crate::grid::row::Row;
use crate::index::Line;

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
