use std::cmp::{max, PartialEq};
use std::mem;
use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

use super::Row;
use crate::grid::GridCell;
use crate::index::{Column, Line};

/// Maximum number of buffered lines outside of the grid for performance optimization.
const MAX_CACHE_SIZE: usize = 1_000;

/// A ring buffer for optimizing indexing and rotation.
///
/// The [`Storage::rotate`] and [`Storage::rotate_up`] functions are fast modular additions on the
/// internal [`zero`] field. As compared with [`slice::rotate_left`] which must rearrange items in
/// memory.
///
/// As a consequence, both [`Index`] and [`IndexMut`] are reimplemented for this type to account
/// for the zeroth element not always being at the start of the allocation.
///
/// Because certain [`Vec`] operations are no longer valid on this type, no [`Deref`]
/// implementation is provided. Anything from [`Vec`] that should be exposed must be done so
/// manually.
///
/// [`slice::rotate_left`]: https://doc.rust-lang.org/std/primitive.slice.html#method.rotate_left
/// [`Deref`]: std::ops::Deref
/// [`zero`]: #structfield.zero
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Storage<T> {
    inner: Vec<Row<T>>,

    /// Starting point for the storage of rows.
    ///
    /// This value represents the starting line offset within the ring buffer. The value of this
    /// offset may be larger than the `len` itself, and will wrap around to the start to form the
    /// ring buffer. It represents the bottommost line of the terminal.
    zero: usize,

    /// Number of visible lines.
    visible_lines: Line,

    /// Total number of lines currently active in the terminal (scrollback + visible)
    ///
    /// Shrinking this length allows reducing the number of lines in the scrollback buffer without
    /// having to truncate the raw `inner` buffer.
    /// As long as `len` is bigger than `inner`, it is also possible to grow the scrollback buffer
    /// without any additional insertions.
    len: usize,
}

impl<T: PartialEq> PartialEq for Storage<T> {
    fn eq(&self, other: &Self) -> bool {
        // Both storage buffers need to be truncated and zeroed.
        assert_eq!(self.zero, 0);
        assert_eq!(other.zero, 0);

        self.inner == other.inner && self.len == other.len
    }
}

impl<T> Storage<T> {
    #[inline]
    pub fn with_capacity(visible_lines: Line, template: Row<T>) -> Storage<T>
    where
        T: Clone,
    {
        // Initialize visible lines, the scrollback buffer is initialized dynamically.
        let inner = vec![template; visible_lines.0];

        Storage { inner, zero: 0, visible_lines, len: visible_lines.0 }
    }

    /// Increase the number of lines in the buffer.
    pub fn grow_visible_lines(&mut self, next: Line, template_row: Row<T>)
    where
        T: Clone,
    {
        // Number of lines the buffer needs to grow.
        let growage = next - self.visible_lines;
        self.grow_lines(growage.0, template_row);

        // Update visible lines.
        self.visible_lines = next;
    }

    /// Grow the number of lines in the buffer, filling new lines with the template.
    fn grow_lines(&mut self, growage: usize, template_row: Row<T>)
    where
        T: Clone,
    {
        // Only grow if there are not enough lines still hidden.
        let mut new_growage = 0;
        if growage > (self.inner.len() - self.len) {
            // Lines to grow additionally to invisible lines.
            new_growage = growage - (self.inner.len() - self.len);

            // Split off the beginning of the raw inner buffer.
            let mut start_buffer = self.inner.split_off(self.zero);

            // Insert new template rows at the end of the raw inner buffer.
            let mut new_lines = vec![template_row; new_growage];
            self.inner.append(&mut new_lines);

            // Add the start to the raw inner buffer again.
            self.inner.append(&mut start_buffer);
        }

        // Update raw buffer length and zero offset.
        self.zero += new_growage;
        self.len += growage;
    }

    /// Decrease the number of lines in the buffer.
    pub fn shrink_visible_lines(&mut self, next: Line) {
        // Shrink the size without removing any lines.
        let shrinkage = self.visible_lines - next;
        self.shrink_lines(shrinkage.0);

        // Update visible lines.
        self.visible_lines = next;
    }

    /// Shrink the number of lines in the buffer.
    pub fn shrink_lines(&mut self, shrinkage: usize) {
        self.len -= shrinkage;

        // Free memory.
        if self.inner.len() > self.len + MAX_CACHE_SIZE {
            self.truncate();
        }
    }

    /// Truncate the invisible elements from the raw buffer.
    pub fn truncate(&mut self) {
        self.inner.rotate_left(self.zero);
        self.inner.truncate(self.len);

        self.zero = 0;
    }

    /// Dynamically grow the storage buffer at runtime.
    #[inline]
    pub fn initialize(&mut self, additional_rows: usize, template: T, cols: Column)
    where
        T: GridCell + Copy,
    {
        if self.len + additional_rows > self.inner.len() {
            let realloc_size = max(additional_rows, MAX_CACHE_SIZE);
            let mut new = vec![Row::new(cols, template); realloc_size];
            let mut split = self.inner.split_off(self.zero);
            self.inner.append(&mut new);
            self.inner.append(&mut split);
            self.zero += realloc_size;
        }

        self.len += additional_rows;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Compute actual index in underlying storage given the requested index.
    #[inline]
    fn compute_index(&self, requested: usize) -> usize {
        debug_assert!(requested < self.len);

        let zeroed = self.zero + requested;

        // Use if/else instead of remainder here to improve performance.
        //
        // Requires `zeroed` to be smaller than `self.inner.len() * 2`,
        // but both `self.zero` and `requested` are always smaller than `self.inner.len()`.
        if zeroed >= self.inner.len() {
            zeroed - self.inner.len()
        } else {
            zeroed
        }
    }

    pub fn swap_lines(&mut self, a: Line, b: Line) {
        let offset = self.inner.len() + self.zero + *self.visible_lines - 1;
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
        debug_assert_eq!(std::mem::size_of::<Row<T>>(), 32);

        let a = self.compute_index(a);
        let b = self.compute_index(b);

        unsafe {
            // Cast to a qword array to opt out of copy restrictions and avoid
            // drop hazards. Byte array is no good here since for whatever
            // reason LLVM won't optimized it.
            let a_ptr = self.inner.as_mut_ptr().add(a) as *mut usize;
            let b_ptr = self.inner.as_mut_ptr().add(b) as *mut usize;

            // Copy 1 qword at a time.
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

    /// Rotate the grid, moving all lines up/down in history.
    #[inline]
    pub fn rotate(&mut self, count: isize) {
        debug_assert!(count.abs() as usize <= self.inner.len());

        let len = self.inner.len();
        self.zero = (self.zero as isize + count + len as isize) as usize % self.inner.len();
    }

    /// Rotate the grid up, moving all existing lines down in history.
    ///
    /// This is a faster, specialized version of [`rotate_left`].
    ///
    /// [`rotate_left`]: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.rotate_left
    #[inline]
    pub fn rotate_up(&mut self, count: usize) {
        self.zero = (self.zero + count) % self.inner.len();
    }

    /// Update the raw storage buffer.
    #[inline]
    pub fn replace_inner(&mut self, vec: Vec<Row<T>>) {
        self.len = vec.len();
        self.inner = vec;
        self.zero = 0;
    }

    /// Remove all rows from storage.
    #[inline]
    pub fn take_all(&mut self) -> Vec<Row<T>> {
        self.truncate();

        let mut buffer = Vec::new();

        mem::swap(&mut buffer, &mut self.inner);
        self.zero = 0;
        self.len = 0;

        buffer
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
        let index = self.visible_lines - 1 - index;
        &self[*index]
    }
}

impl<T> IndexMut<Line> for Storage<T> {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Self::Output {
        let index = self.visible_lines - 1 - index;
        &mut self[*index]
    }
}

#[cfg(test)]
mod tests {
    use crate::grid::row::Row;
    use crate::grid::storage::{Storage, MAX_CACHE_SIZE};
    use crate::grid::GridCell;
    use crate::index::{Column, Line};
    use crate::term::cell::Flags;

    impl GridCell for char {
        fn is_empty(&self) -> bool {
            *self == ' ' || *self == '\t'
        }

        fn flags(&self) -> &Flags {
            unimplemented!();
        }

        fn flags_mut(&mut self) -> &mut Flags {
            unimplemented!();
        }

        fn fast_eq(&self, other: Self) -> bool {
            self == &other
        }
    }

    #[test]
    fn with_capacity() {
        let storage = Storage::with_capacity(Line(3), Row::new(Column(0), ' '));

        assert_eq!(storage.inner.len(), 3);
        assert_eq!(storage.len, 3);
        assert_eq!(storage.zero, 0);
        assert_eq!(storage.visible_lines, Line(3));
    }

    #[test]
    fn indexing() {
        let mut storage = Storage::with_capacity(Line(3), Row::new(Column(0), ' '));

        storage[0] = Row::new(Column(1), '0');
        storage[1] = Row::new(Column(1), '1');
        storage[2] = Row::new(Column(1), '2');

        assert_eq!(storage[0], Row::new(Column(1), '0'));
        assert_eq!(storage[1], Row::new(Column(1), '1'));
        assert_eq!(storage[2], Row::new(Column(1), '2'));

        storage.zero += 1;

        assert_eq!(storage[0], Row::new(Column(1), '1'));
        assert_eq!(storage[1], Row::new(Column(1), '2'));
        assert_eq!(storage[2], Row::new(Column(1), '0'));
    }

    #[test]
    #[should_panic]
    fn indexing_above_inner_len() {
        let storage = Storage::with_capacity(Line(1), Row::new(Column(0), ' '));
        let _ = &storage[2];
    }

    #[test]
    fn rotate() {
        let mut storage = Storage::with_capacity(Line(3), Row::new(Column(0), ' '));
        storage.rotate(2);
        assert_eq!(storage.zero, 2);
        storage.shrink_lines(2);
        assert_eq!(storage.len, 1);
        assert_eq!(storage.inner.len(), 3);
        assert_eq!(storage.zero, 2);
    }

    /// Grow the buffer one line at the end of the buffer.
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
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '-'),
            ],
            zero: 0,
            visible_lines: Line(3),
            len: 3,
        };

        // Grow buffer
        storage.grow_visible_lines(Line(4), Row::new(Column(1), '-'));

        // Make sure the result is correct
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), '-'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '-'),
            ],
            zero: 1,
            visible_lines: Line(4),
            len: 4,
        };
        assert_eq!(storage.visible_lines, expected.visible_lines);
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// Grow the buffer one line at the start of the buffer.
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
        // Setup storage area.
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), '-'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
            ],
            zero: 1,
            visible_lines: Line(3),
            len: 3,
        };

        // Grow buffer.
        storage.grow_visible_lines(Line(4), Row::new(Column(1), '-'));

        // Make sure the result is correct.
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), '-'),
                Row::new(Column(1), '-'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
            ],
            zero: 2,
            visible_lines: Line(4),
            len: 4,
        };
        assert_eq!(storage.visible_lines, expected.visible_lines);
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// Shrink the buffer one line at the start of the buffer.
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
        // Setup storage area.
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), '2'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
            ],
            zero: 1,
            visible_lines: Line(3),
            len: 3,
        };

        // Shrink buffer.
        storage.shrink_visible_lines(Line(2));

        // Make sure the result is correct.
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), '2'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
            ],
            zero: 1,
            visible_lines: Line(2),
            len: 2,
        };
        assert_eq!(storage.visible_lines, expected.visible_lines);
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// Shrink the buffer one line at the end of the buffer.
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
        // Setup storage area.
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
            ],
            zero: 0,
            visible_lines: Line(3),
            len: 3,
        };

        // Shrink buffer.
        storage.shrink_visible_lines(Line(2));

        // Make sure the result is correct.
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
            ],
            zero: 0,
            visible_lines: Line(2),
            len: 2,
        };
        assert_eq!(storage.visible_lines, expected.visible_lines);
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// Shrink the buffer at the start and end of the buffer.
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
        // Setup storage area.
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), '4'),
                Row::new(Column(1), '5'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
                Row::new(Column(1), '3'),
            ],
            zero: 2,
            visible_lines: Line(6),
            len: 6,
        };

        // Shrink buffer.
        storage.shrink_visible_lines(Line(2));

        // Make sure the result is correct.
        let expected = Storage {
            inner: vec![
                Row::new(Column(1), '4'),
                Row::new(Column(1), '5'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
                Row::new(Column(1), '3'),
            ],
            zero: 2,
            visible_lines: Line(2),
            len: 2,
        };
        assert_eq!(storage.visible_lines, expected.visible_lines);
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// Check that when truncating all hidden lines are removed from the raw buffer.
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
        // Setup storage area.
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), '4'),
                Row::new(Column(1), '5'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
                Row::new(Column(1), '3'),
            ],
            zero: 2,
            visible_lines: Line(1),
            len: 2,
        };

        // Truncate buffer.
        storage.truncate();

        // Make sure the result is correct.
        let expected = Storage {
            inner: vec![Row::new(Column(1), '0'), Row::new(Column(1), '1')],
            zero: 0,
            visible_lines: Line(1),
            len: 2,
        };
        assert_eq!(storage.visible_lines, expected.visible_lines);
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// Truncate buffer only at the beginning.
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
        // Setup storage area.
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
                Row::new(Column(1), '0'),
            ],
            zero: 2,
            visible_lines: Line(1),
            len: 2,
        };

        // Truncate buffer.
        storage.truncate();

        // Make sure the result is correct.
        let expected = Storage {
            inner: vec![Row::new(Column(1), '0'), Row::new(Column(1), '1')],
            zero: 0,
            visible_lines: Line(1),
            len: 2,
        };
        assert_eq!(storage.visible_lines, expected.visible_lines);
        assert_eq!(storage.inner, expected.inner);
        assert_eq!(storage.zero, expected.zero);
        assert_eq!(storage.len, expected.len);
    }

    /// First shrink the buffer and then grow it again.
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
        // Setup storage area.
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), '4'),
                Row::new(Column(1), '5'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
                Row::new(Column(1), '3'),
            ],
            zero: 2,
            visible_lines: Line(0),
            len: 6,
        };

        // Shrink buffer.
        storage.shrink_lines(3);

        // Make sure the result after shrinking is correct.
        let shrinking_expected = Storage {
            inner: vec![
                Row::new(Column(1), '4'),
                Row::new(Column(1), '5'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
                Row::new(Column(1), '3'),
            ],
            zero: 2,
            visible_lines: Line(0),
            len: 3,
        };
        assert_eq!(storage.inner, shrinking_expected.inner);
        assert_eq!(storage.zero, shrinking_expected.zero);
        assert_eq!(storage.len, shrinking_expected.len);

        // Grow buffer.
        storage.grow_lines(4, Row::new(Column(1), '-'));

        // Make sure the result after shrinking is correct.
        let growing_expected = Storage {
            inner: vec![
                Row::new(Column(1), '4'),
                Row::new(Column(1), '5'),
                Row::new(Column(1), '-'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
                Row::new(Column(1), '3'),
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
        // Setup storage area.
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), '4'),
                Row::new(Column(1), '5'),
                Row::new(Column(1), '0'),
                Row::new(Column(1), '1'),
                Row::new(Column(1), '2'),
                Row::new(Column(1), '3'),
            ],
            zero: 2,
            visible_lines: Line(0),
            len: 6,
        };

        // Initialize additional lines.
        let init_size = 3;
        storage.initialize(init_size, '-', Column(1));

        // Make sure the lines are present and at the right location.

        let expected_init_size = std::cmp::max(init_size, MAX_CACHE_SIZE);
        let mut expected_inner = vec![Row::new(Column(1), '4'), Row::new(Column(1), '5')];
        expected_inner.append(&mut vec![Row::new(Column(1), '-'); expected_init_size]);
        expected_inner.append(&mut vec![
            Row::new(Column(1), '0'),
            Row::new(Column(1), '1'),
            Row::new(Column(1), '2'),
            Row::new(Column(1), '3'),
        ]);
        let expected_storage = Storage {
            inner: expected_inner,
            zero: 2 + expected_init_size,
            visible_lines: Line(0),
            len: 9,
        };

        assert_eq!(storage.inner, expected_storage.inner);
        assert_eq!(storage.zero, expected_storage.zero);
        assert_eq!(storage.len, expected_storage.len);
    }

    #[test]
    fn rotate_wrap_zero() {
        let mut storage = Storage {
            inner: vec![
                Row::new(Column(1), '-'),
                Row::new(Column(1), '-'),
                Row::new(Column(1), '-'),
            ],
            zero: 2,
            visible_lines: Line(0),
            len: 3,
        };

        storage.rotate(2);

        assert!(storage.zero < storage.inner.len());
    }
}
