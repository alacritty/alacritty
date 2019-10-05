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
use std::vec::Drain;

use serde::{Deserialize, Serialize};
use static_assertions::assert_eq_size;

use super::Row;
use crate::index::Line;

/// Maximum number of invisible lines before buffer is resized
const TRUNCATE_STEP: usize = 100;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Storage<T> {
    inner: Vec<Row<T>>,
    zero: usize,
    visible_lines: Line,

    /// Total number of lines currently active in the terminal (scrollback + visible)
    ///
    /// Shrinking this length allows reducing the number of lines in the scrollback buffer without
    /// having to truncate the raw `inner` buffer.
    /// As long as `len` is bigger than `inner`, it is also possible to grow the scrollback buffer
    /// without any additional insertions.
    #[serde(default)]
    len: usize,
}

impl<T: PartialEq> ::std::cmp::PartialEq for Storage<T> {
    fn eq(&self, other: &Self) -> bool {
        // Make sure length is equal
        if self.inner.len() != other.inner.len() {
            return false;
        }

        // Check which vec has the bigger zero
        let (ref bigger, ref smaller) =
            if self.zero >= other.zero { (self, other) } else { (other, self) };

        // Calculate the actual zero offset
        let len = self.inner.len();
        let bigger_zero = bigger.zero % len;
        let smaller_zero = smaller.zero % len;

        // Compare the slices in chunks
        // Chunks:
        //   - Bigger zero to the end
        //   - Remaining lines in smaller zero vec
        //   - Beginning of smaller zero vec
        //
        // Example:
        //   Bigger Zero (6):
        //     4  5  6  | 7  8  9  | 0  1  2  3
        //     C2 C2 C2 | C3 C3 C3 | C1 C1 C1 C1
        //   Smaller Zero (3):
        //     7  8  9  | 0  1  2  3  | 4  5  6
        //     C3 C3 C3 | C1 C1 C1 C1 | C2 C2 C2
        bigger.inner[bigger_zero..]
            == smaller.inner[smaller_zero..smaller_zero + (len - bigger_zero)]
            && bigger.inner[..bigger_zero - smaller_zero]
                == smaller.inner[smaller_zero + (len - bigger_zero)..]
            && bigger.inner[bigger_zero - smaller_zero..bigger_zero]
                == smaller.inner[..smaller_zero]
    }
}

impl<T> Storage<T> {
    #[inline]
    pub fn with_capacity(lines: Line, template: Row<T>) -> Storage<T>
    where
        T: Clone,
    {
        // Initialize visible lines, the scrollback buffer is initialized dynamically
        let inner = vec![template; lines.0];

        Storage { inner, zero: 0, visible_lines: lines - 1, len: lines.0 }
    }

    /// Update the size of the scrollback history
    pub fn update_history(&mut self, history_size: usize, template_row: Row<T>)
    where
        T: Clone,
    {
        let current_history = self.len - (self.visible_lines.0 + 1);
        if history_size > current_history {
            self.grow_lines(history_size - current_history, template_row);
        } else if history_size < current_history {
            self.shrink_lines(current_history - history_size);
        }
    }

    /// Increase the number of lines in the buffer
    pub fn grow_visible_lines(&mut self, next: Line, template_row: Row<T>)
    where
        T: Clone,
    {
        // Number of lines the buffer needs to grow
        let growage = (next - (self.visible_lines + 1)).0;
        self.grow_lines(growage, template_row);

        // Update visible lines
        self.visible_lines = next - 1;
    }

    /// Grow the number of lines in the buffer, filling new lines with the template
    fn grow_lines(&mut self, growage: usize, template_row: Row<T>)
    where
        T: Clone,
    {
        // Only grow if there are not enough lines still hidden
        let mut new_growage = 0;
        if growage > (self.inner.len() - self.len) {
            // Lines to grow additionally to invisible lines
            new_growage = growage - (self.inner.len() - self.len);

            // Split off the beginning of the raw inner buffer
            let mut start_buffer = self.inner.split_off(self.zero);

            // Insert new template rows at the end of the raw inner buffer
            let mut new_lines = vec![template_row; new_growage];
            self.inner.append(&mut new_lines);

            // Add the start to the raw inner buffer again
            self.inner.append(&mut start_buffer);
        }

        // Update raw buffer length and zero offset
        self.zero = (self.zero + new_growage) % self.inner.len();
        self.len += growage;
    }

    /// Decrease the number of lines in the buffer
    pub fn shrink_visible_lines(&mut self, next: Line) {
        // Shrink the size without removing any lines
        let shrinkage = (self.visible_lines - (next - 1)).0;
        self.shrink_lines(shrinkage);

        // Update visible lines
        self.visible_lines = next - 1;
    }

    // Shrink the number of lines in the buffer
    pub fn shrink_lines(&mut self, shrinkage: usize) {
        self.len -= shrinkage;

        // Free memory
        if self.inner.len() > self.len() + TRUNCATE_STEP {
            self.truncate();
        }
    }

    /// Truncate the invisible elements from the raw buffer
    pub fn truncate(&mut self) {
        self.inner.rotate_left(self.zero);
        self.inner.truncate(self.len);

        self.zero = 0;
    }

    /// Dynamically grow the storage buffer at runtime
    pub fn initialize(&mut self, num_rows: usize, template_row: Row<T>)
    where
        T: Clone,
    {
        let mut new = vec![template_row; num_rows];

        let mut split = self.inner.split_off(self.zero);
        self.inner.append(&mut new);
        self.inner.append(&mut split);

        self.zero += num_rows;
        self.len += num_rows;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    /// Compute actual index in underlying storage given the requested index.
    fn compute_index(&self, requested: usize) -> usize {
        debug_assert!(requested < self.len);
        let zeroed = requested + self.zero;

        // This part is critical for performance,
        // so an if/else is used here instead of a moludo operation
        if zeroed >= self.inner.len() {
            zeroed - self.inner.len()
        } else {
            zeroed
        }
    }

    pub fn swap_lines(&mut self, a: Line, b: Line) {
        let offset = self.inner.len() + self.zero + *self.visible_lines;
        let a = (offset - *a) % self.inner.len();
        let b = (offset - *b) % self.inner.len();
        self.inner.swap(a, b);
    }

    /// Swap implementation for Row<T>.
    ///
    /// Exploits the known size of Row<T> to produce a slightly more efficient
    /// swap than going through slice::swap.
    ///
    /// The default implementation from swap generates 8 movups and 4 movaps
    /// instructions. This implementation achieves the swap in only 8 movups
    /// instructions.
    pub fn swap(&mut self, a: usize, b: usize) {
        assert_eq_size!(Row<T>, [usize; 4]);

        let a = self.compute_index(a);
        let b = self.compute_index(b);

        unsafe {
            // Cast to a qword array to opt out of copy restrictions and avoid
            // drop hazards. Byte array is no good here since for whatever
            // reason LLVM won't optimized it.
            let a_ptr = self.inner.as_mut_ptr().add(a) as *mut usize;
            let b_ptr = self.inner.as_mut_ptr().add(b) as *mut usize;

            // Copy 1 qword at a time
            //
            // The optimizer unrolls this loop and vectorizes it.
            let mut tmp: usize;
            for i in 0..4 {
                tmp = *a_ptr.offset(i);
                *a_ptr.offset(i) = *b_ptr.offset(i);
                *b_ptr.offset(i) = tmp;
            }
        }
    }

    #[inline]
    pub fn rotate(&mut self, count: isize) {
        debug_assert!(count.abs() as usize <= self.inner.len());

        let len = self.inner.len();
        self.zero = (self.zero as isize + count + len as isize) as usize % len;
    }

    // Fast path
    #[inline]
    pub fn rotate_up(&mut self, count: usize) {
        self.zero = (self.zero + count) % self.inner.len();
    }

    pub fn drain(&mut self) -> Drain<'_, Row<T>> {
        self.truncate();
        self.inner.drain(..)
    }

    /// Update the raw storage buffer
    pub fn replace_inner(&mut self, vec: Vec<Row<T>>) {
        self.len = vec.len();
        self.inner = vec;
        self.zero = 0;
    }
}

impl<T> Index<usize> for Storage<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.inner[self.compute_index(index)]
    }
}

impl<T> IndexMut<usize> for Storage<T> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let index = self.compute_index(index); // borrowck
        &mut self.inner[index]
    }
}

impl<T> Index<Line> for Storage<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: Line) -> &Self::Output {
        let index = self.visible_lines - index;
        &self[*index]
    }
}

impl<T> IndexMut<Line> for Storage<T> {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Self::Output {
        let index = self.visible_lines - index;
        &mut self[*index]
    }
}

#[cfg(test)]
mod test {
    use crate::grid::row::Row;
    use crate::grid::storage::Storage;
    use crate::grid::GridCell;
    use crate::index::{Column, Line};

    impl GridCell for char {
        fn is_empty(&self) -> bool {
            *self == ' ' || *self == '\t'
        }

        fn is_wrap(&self) -> bool {
            false
        }

        fn set_wrap(&mut self, _wrap: bool) {}
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
            inner: vec![
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'-'),
            ],
            zero: 0,
            visible_lines: Line(2),
            len: 3,
        };

        // Grow buffer
        storage.grow_visible_lines(Line(4), Row::new(Column(1), &'-'));

        // Make sure the result is correct
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), &'-'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'-'),
            ],
            zero: 1,
            visible_lines: Line(0),
            len: 4,
        };
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
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
            inner: vec![
                Row::new(Column(1), &'-'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
            ],
            zero: 1,
            visible_lines: Line(2),
            len: 3,
        };

        // Grow buffer
        storage.grow_visible_lines(Line(4), Row::new(Column(1), &'-'));

        // Make sure the result is correct
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), &'-'),
                Row::new(Column(1), &'-'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
            ],
            zero: 2,
            visible_lines: Line(0),
            len: 4,
        };
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// Shrink the buffer one line at the start of the buffer
    ///
    /// Before:
    ///   0: 2
    ///   1: 0 <- Zero
    ///   2: 1
    /// After:
    ///   0: 2 <- Hidden
    ///   0: 0 <- Zero
    ///   1: 1
    #[test]
    fn shrink_before_zero() {
        // Setup storage area
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
            ],
            zero: 1,
            visible_lines: Line(2),
            len: 3,
        };

        // Shrink buffer
        storage.shrink_visible_lines(Line(2));

        // Make sure the result is correct
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
            ],
            zero: 1,
            visible_lines: Line(0),
            len: 2,
        };
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
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
    ///   2: 2 <- Hidden
    #[test]
    fn shrink_after_zero() {
        // Setup storage area
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
            ],
            zero: 0,
            visible_lines: Line(2),
            len: 3,
        };

        // Shrink buffer
        storage.shrink_visible_lines(Line(2));

        // Make sure the result is correct
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
            ],
            zero: 0,
            visible_lines: Line(0),
            len: 2,
        };
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
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
    ///   0: 4 <- Hidden
    ///   1: 5 <- Hidden
    ///   2: 0 <- Zero
    ///   3: 1
    ///   4: 2 <- Hidden
    ///   5: 3 <- Hidden
    #[test]
    fn shrink_before_and_after_zero() {
        // Setup storage area
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), &'4'),
                Row::new(Column(1), &'5'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'3'),
            ],
            zero: 2,
            visible_lines: Line(5),
            len: 6,
        };

        // Shrink buffer
        storage.shrink_visible_lines(Line(2));

        // Make sure the result is correct
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), &'4'),
                Row::new(Column(1), &'5'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'3'),
            ],
            zero: 2,
            visible_lines: Line(0),
            len: 2,
        };
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// Check that when truncating all hidden lines are removed from the raw buffer
    ///
    /// Before:
    ///   0: 4 <- Hidden
    ///   1: 5 <- Hidden
    ///   2: 0 <- Zero
    ///   3: 1
    ///   4: 2 <- Hidden
    ///   5: 3 <- Hidden
    /// After:
    ///   0: 0 <- Zero
    ///   1: 1
    #[test]
    fn truncate_invisible_lines() {
        // Setup storage area
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), &'4'),
                Row::new(Column(1), &'5'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'3'),
            ],
            zero: 2,
            visible_lines: Line(1),
            len: 2,
        };

        // Truncate buffer
        storage.truncate();

        // Make sure the result is correct
        let expected = Storage {
            inner: vec![Row::new(Column(1), &'0'), Row::new(Column(1), &'1')],
            zero: 0,
            visible_lines: Line(1),
            len: 2,
        };
        assert_eq!(storage.visible_lines, expected.visible_lines);
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// Truncate buffer only at the beginning
    ///
    /// Before:
    ///   0: 1
    ///   1: 2 <- Hidden
    ///   2: 0 <- Zero
    /// After:
    ///   0: 1
    ///   0: 0 <- Zero
    #[test]
    fn truncate_invisible_lines_beginning() {
        // Setup storage area
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'0'),
            ],
            zero: 2,
            visible_lines: Line(1),
            len: 2,
        };

        // Truncate buffer
        storage.truncate();

        // Make sure the result is correct
        let expected = Storage {
            inner: vec![Row::new(Column(1), &'0'), Row::new(Column(1), &'1')],
            zero: 0,
            visible_lines: Line(1),
            len: 2,
        };
        assert_eq!(storage.visible_lines, expected.visible_lines);
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// First shrink the buffer and then grow it again
    ///
    /// Before:
    ///   0: 4
    ///   1: 5
    ///   2: 0 <- Zero
    ///   3: 1
    ///   4: 2
    ///   5: 3
    /// After Shrinking:
    ///   0: 4 <- Hidden
    ///   1: 5 <- Hidden
    ///   2: 0 <- Zero
    ///   3: 1
    ///   4: 2
    ///   5: 3 <- Hidden
    /// After Growing:
    ///   0: 4
    ///   1: 5
    ///   2: -
    ///   3: 0 <- Zero
    ///   4: 1
    ///   5: 2
    ///   6: 3
    #[test]
    fn shrink_then_grow() {
        // Setup storage area
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), &'4'),
                Row::new(Column(1), &'5'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'3'),
            ],
            zero: 2,
            visible_lines: Line(0),
            len: 6,
        };

        // Shrink buffer
        storage.shrink_lines(3);

        // Make sure the result after shrinking is correct
        let shrinking_expected = Storage {
            inner: vec![
                Row::new(Column(1), &'4'),
                Row::new(Column(1), &'5'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'3'),
            ],
            zero: 2,
            visible_lines: Line(0),
            len: 3,
        };
        assert_eq!(storage.inner, shrinking_expected.inner);
        assert_eq!(storage.zero, shrinking_expected.zero);
        assert_eq!(storage.len, shrinking_expected.len);

        // Grow buffer
        storage.grow_lines(4, Row::new(Column(1), &'-'));

        // Make sure the result after shrinking is correct
        let growing_expected = Storage {
            inner: vec![
                Row::new(Column(1), &'4'),
                Row::new(Column(1), &'5'),
                Row::new(Column(1), &'-'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'3'),
            ],
            zero: 3,
            visible_lines: Line(0),
            len: 7,
        };
        assert_eq!(storage.inner, growing_expected.inner);
        assert_eq!(storage.zero, growing_expected.zero);
        assert_eq!(storage.len, growing_expected.len);
    }

    #[test]
    fn initialize() {
        // Setup storage area
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), &'4'),
                Row::new(Column(1), &'5'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'3'),
            ],
            zero: 2,
            visible_lines: Line(0),
            len: 6,
        };

        // Initialize additional lines
        storage.initialize(3, Row::new(Column(1), &'-'));

        // Make sure the lines are present and at the right location
        let shrinking_expected = Storage {
            inner: vec![
                Row::new(Column(1), &'4'),
                Row::new(Column(1), &'5'),
                Row::new(Column(1), &'-'),
                Row::new(Column(1), &'-'),
                Row::new(Column(1), &'-'),
                Row::new(Column(1), &'0'),
                Row::new(Column(1), &'1'),
                Row::new(Column(1), &'2'),
                Row::new(Column(1), &'3'),
            ],
            zero: 5,
            visible_lines: Line(0),
            len: 9,
        };
        assert_eq!(storage.inner, shrinking_expected.inner);
        assert_eq!(storage.zero, shrinking_expected.zero);
        assert_eq!(storage.len, shrinking_expected.len);
    }
}
