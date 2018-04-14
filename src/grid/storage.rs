/// Wrapper around Vec which supports fast indexing and rotation
///
/// The rotation implemented by grid::Storage is a simple integer addition.
/// Compare with standard library rotation which requires rearranging items in
/// memory.
///
/// As a consequence, the indexing operators need to be reimplemented for this
/// type to account for the 0th element not always being at the start of the
/// allocation.
///
/// Because certain Vec operations are no longer valid on this type, no Deref
/// implementation is provided. Anything from Vec that should be exposed must be
/// done so manually.
use std::ops::{Index, IndexMut};

use index::{IndexRange, Line};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Storage<T> {
    inner: Vec<T>,
    zero: usize,
    visible_lines: Line,
}

impl<T> Storage<T> {
    #[inline]
    pub fn with_capacity(cap: usize, lines: Line) -> Storage<T> {
        Storage {
            inner: Vec::with_capacity(cap),
            zero: 0,
            visible_lines: lines - 1,
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Increase the number of lines in the buffer
    pub fn grow_visible_lines(&mut self, next: Line, template_row: T)
    where
        T: Clone,
    {
        // Calculate insert position (before the first line)
        let offset = self.zero % self.inner.len();

        // Insert new template row for every line grown
        let lines_to_grow = (next - (self.visible_lines + 1)).0;
        for _ in 0..lines_to_grow {
            self.inner.insert(offset, template_row.clone());
        }

        // Set zero to old zero + lines grown
        self.zero = offset + lines_to_grow;

        // Update visible lines
        self.visible_lines = next - 1;
    }

    /// Decrease the number of lines in the buffer
    pub fn shrink_visible_lines(&mut self, next: Line) {
        // Calculate shrinkage and last line of buffer
        let shrinkage = (self.visible_lines + 1 - next).0;
        let offset = (self.zero + self.inner.len() - 1) % self.inner.len();

        // Generate range of lines that have to be deleted before the zero line
        let start = offset.saturating_sub(shrinkage - 1);
        let shrink_before = start..(offset + 1);

        // Generate range of lines that have to be deleted after the zero line
        let shrink_after = (self.inner.len() + offset + 1 - shrinkage)..self.inner.len();

        // Delete all lines in reverse order
        for i in shrink_before.chain(shrink_after).rev() {
            self.inner.remove(i);
        }

        // Check if zero has moved (not the first line in the buffer)
        if self.zero % (self.inner.len() + shrinkage) != 0 {
            // Set zero to the first deleted line in the buffer
            self.zero = start;
        }

        // Update visible lines
        self.visible_lines = next - 1;
    }

    #[inline]
    pub fn push(&mut self, item: T) {
        self.inner.push(item)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Compute actual index in underlying storage given the requested index.
    #[inline]
    fn compute_index(&self, requested: usize) -> usize {
        (requested + self.zero) % self.len()
    }

    fn compute_line_index(&self, requested: Line) -> usize {
        ((self.len() + self.zero + *self.visible_lines) - *requested) % self.len()
    }

    pub fn swap_lines(&mut self, a: Line, b: Line) {
        let a = self.compute_line_index(a);
        let b = self.compute_line_index(b);
        self.inner.swap(a, b);
    }

    pub fn iter_mut(&mut self) -> IterMut<T> {
        IterMut { storage: self, index: 0 }
    }

    pub fn rotate(&mut self, count: isize) {
        let len = self.len();
        assert!(count.abs() as usize <= len);
        self.zero += (count + len as isize) as usize % len;
    }

    // Fast path
    pub fn rotate_up(&mut self, count: usize) {
        self.zero = (self.zero + count) % self.len();
    }
}

impl<T> Index<usize> for Storage<T> {
    type Output = T;
    #[inline]
    fn index(&self, index: usize) -> &T {
        let index = self.compute_index(index); // borrowck
        &self.inner[index]
    }
}

impl<T> IndexMut<usize> for Storage<T> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut T {
        let index = self.compute_index(index); // borrowck
        &mut self.inner[index]
    }
}

impl<T> Index<Line> for Storage<T> {
    type Output = T;
    #[inline]
    fn index(&self, index: Line) -> &T {
        let index = self.visible_lines - index;
        &self[*index]
    }
}

impl<T> IndexMut<Line> for Storage<T> {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut T {
        let index = self.visible_lines - index;
        &mut self[*index]
    }
}

pub struct IterMut<'a, T: 'a> {
    storage: &'a mut Storage<T>,
    index: usize,
}

impl<'a, T: 'a> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.storage.len() {
            None
        } else {
            let index = self.index;
            self.index += 1;

            unsafe {
                Some(&mut *(&mut self.storage[index] as *mut _))
            }
        }
    }
}

/// Grow the buffer one line at the end of the buffer
///
/// Before:
///   0: 0 <- Zero
///   1: 1
///   2: -
/// After:
///   0: -
///   1: 0 <- Zero
///   2: 1
///   3: -
#[test]
fn grow_after_zero() {
    // Setup storage area
    let mut storage = Storage {
        inner: vec!["0", "1", "-"],
        zero: 0,
        visible_lines: Line(2),
    };

    // Grow buffer
    storage.grow_visible_lines(Line(4), "-");

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["-", "0", "1", "-"],
        zero: 1,
        visible_lines: Line(0),
    };
    assert_eq!(storage.inner, expected.inner);
    assert_eq!(storage.zero, expected.zero);
}

/// Grow the buffer one line at the start of the buffer
///
/// Before:
///   0: -
///   1: 0 <- Zero
///   2: 1
/// After:
///   0: -
///   1: -
///   2: 0 <- Zero
///   3: 1
#[test]
fn grow_before_zero() {
    // Setup storage area
    let mut storage = Storage {
        inner: vec!["-", "0", "1"],
        zero: 1,
        visible_lines: Line(2),
    };

    // Grow buffer
    storage.grow_visible_lines(Line(4), "-");

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["-", "-", "0", "1"],
        zero: 2,
        visible_lines: Line(0),
    };
    assert_eq!(storage.inner, expected.inner);
    assert_eq!(storage.zero, expected.zero);
}

/// Shrink the buffer one line at the start of the buffer
///
/// Before:
///   0: 2
///   1: 0 <- Zero
///   2: 1
/// After:
///   0: 0 <- Zero
///   1: 1
#[test]
fn shrink_before_zero() {
    // Setup storage area
    let mut storage = Storage {
        inner: vec!["2", "0", "1"],
        zero: 1,
        visible_lines: Line(2),
    };

    // Shrink buffer
    storage.shrink_visible_lines(Line(2));

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["0", "1"],
        zero: 0,
        visible_lines: Line(0),
    };
    assert_eq!(storage.inner, expected.inner);
    assert_eq!(storage.zero, expected.zero);
}

/// Shrink the buffer one line at the end of the buffer
///
/// Before:
///   0: 0 <- Zero
///   1: 1
///   2: 2
/// After:
///   0: 0 <- Zero
///   1: 1
#[test]
fn shrink_after_zero() {
    // Setup storage area
    let mut storage = Storage {
        inner: vec!["0", "1", "2"],
        zero: 0,
        visible_lines: Line(2),
    };

    // Shrink buffer
    storage.shrink_visible_lines(Line(2));

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["0", "1"],
        zero: 0,
        visible_lines: Line(0),
    };
    assert_eq!(storage.inner, expected.inner);
    assert_eq!(storage.zero, expected.zero);
}

/// Shrink the buffer at the start and end of the buffer
///
/// Before:
///   0: 4
///   1: 5
///   2: 0 <- Zero
///   3: 1
///   4: 2
///   5: 3
/// After:
///   0: 0 <- Zero
///   1: 1
#[test]
fn shrink_before_and_after_zero() {
    // Setup storage area
    let mut storage = Storage {
        inner: vec!["4", "5", "0", "1", "2", "3"],
        zero: 2,
        visible_lines: Line(5),
    };

    // Shrink buffer
    storage.shrink_visible_lines(Line(2));

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["0", "1"],
        zero: 0,
        visible_lines: Line(0),
    };
    assert_eq!(storage.inner, expected.inner);
    assert_eq!(storage.zero, expected.zero);
}
