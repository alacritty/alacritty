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
use std::slice;

use index::Line;
use grid::Row;

/// Maximum number of invisible lines before buffer is resized
const TRUNCATE_STEP: usize = 100;

pub trait Swap {
    fn swap(&mut self, _: usize, _: usize);
}

impl<T> Swap for Storage<T> {
    /// Swap two lines in raw buffer
    ///
    /// # Panics
    ///
    /// `swap` will panic if either `a` or `b` are out-of-bounds of the
    /// underlying storage.
    default fn swap(&mut self, a: usize, b: usize) {
        let a = self.compute_index(a);
        let b = self.compute_index(b);

        self.inner.swap(a, b);
    }
}

impl<T> Swap for Storage<Row<T>> {
    /// Custom swap implementation for Row<T>.
    ///
    /// Exploits the known size of Row<T> to produce a slightly more efficient
    /// swap than going through slice::swap.
    ///
    /// The default implementation from swap generates 8 movups and 4 movaps
    /// instructions. This implementation achieves the swap in only 8 movups
    /// instructions.
    fn swap(&mut self, a: usize, b: usize) {
        debug_assert!(::std::mem::size_of::<Row<T>>() == 32);

        let a = self.compute_index(a);
        let b = self.compute_index(b);

        unsafe {
            // Cast to a qword array to opt out of copy restrictions and avoid
            // drop hazards. Byte array is no good here since for whatever
            // reason LLVM won't optimized it.
            let a_ptr = self.inner.as_mut_ptr().offset(a as isize) as *mut u64;
            let b_ptr = self.inner.as_mut_ptr().offset(b as isize) as *mut u64;

            // Copy 1 qword at a time
            //
            // The optimizer unrolls this loop and vectorizes it.
            let mut tmp: u64;
            for i in 0..4 {
                tmp = *a_ptr.offset(i);
                *a_ptr.offset(i) = *b_ptr.offset(i);
                *b_ptr.offset(i) = tmp;
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Storage<T> {
    inner: Vec<T>,
    zero: usize,
    visible_lines: Line,

    /// Total number of lines currently active in the terminal (scrollback + visible)
    ///
    /// Shrinking this length allows reducing the number of lines in the scrollback buffer without
    /// having to truncate the raw `inner` buffer.
    /// As long as `len` is bigger than `inner`, it is also possible to grow the scrollback buffer
    /// without any additional insertions.
    #[serde(skip)]
    len: usize,
}

impl<T: PartialEq> ::std::cmp::PartialEq for Storage<T> {
    fn eq(&self, other: &Self) -> bool {
        // Make sure length is equal
        if self.inner.len() != other.inner.len() {
            return false;
        }

        // Check which vec has the bigger zero
        let (ref bigger, ref smaller) = if self.zero >= other.zero {
            (self, other)
        } else {
            (other, self)
        };

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
        &bigger.inner[bigger_zero..]
            == &smaller.inner[smaller_zero..smaller_zero + (len - bigger_zero)]
            && &bigger.inner[..bigger_zero - smaller_zero]
                == &smaller.inner[smaller_zero + (len - bigger_zero)..]
            && &bigger.inner[bigger_zero - smaller_zero..bigger_zero]
                == &smaller.inner[..smaller_zero]
    }
}

impl<T> Storage<T> {
    #[inline]
    pub fn with_capacity(cap: usize, lines: Line, template: T) -> Storage<T>
    where
        T: Clone,
    {
        // Allocate all lines in the buffer, including scrollback history
        //
        // TODO (jwilm) Allocating each line at this point is expensive and
        // delays startup. A nice solution might be having `Row` delay
        // allocation until it's actually used.
        let inner = vec![template; cap];
        Storage {
            inner,
            zero: 0,
            visible_lines: lines - 1,
            len: cap,
        }
    }

    /// Increase the number of lines in the buffer
    pub fn grow_visible_lines(&mut self, next: Line, template_row: T)
    where
        T: Clone,
    {
        // Number of lines the buffer needs to grow
        let lines_to_grow = (next - (self.visible_lines + 1)).0;

        // Only grow if there are not enough lines still hidden
        if lines_to_grow > (self.inner.len() - self.len) {
            // Lines to grow additionally to invisible lines
            let new_lines_to_grow = lines_to_grow - (self.inner.len() - self.len);

            // Get the position of the start of the buffer
            let offset = self.zero % self.inner.len();

            // Split off the beginning of the raw inner buffer
            let mut start_buffer = self.inner.split_off(offset);

            // Insert new template rows at the end of the raw inner buffer
            let mut new_lines = vec![template_row; new_lines_to_grow];
            self.inner.append(&mut new_lines);

            // Add the start to the raw inner buffer again
            self.inner.append(&mut start_buffer);

            // Update the zero to after the lines we just inserted
            self.zero = offset + lines_to_grow;
        }

        // Update visible lines and raw buffer length
        self.len += lines_to_grow;
        self.visible_lines = next - 1;
    }

    /// Decrease the number of lines in the buffer
    pub fn shrink_visible_lines(&mut self, next: Line) {
        // Shrink the size without removing any lines
        self.len -= (self.visible_lines - (next - 1)).0;

        // Update visible lines
        self.visible_lines = next - 1;

        // Free memory
        if self.inner.len() > self.len() + TRUNCATE_STEP {
            self.truncate();
        }
    }

    /// Truncate the invisible elements from the raw buffer
    pub fn truncate(&mut self) {
        // Calculate shrinkage/offset for indexing
        let offset = self.zero % self.inner.len();
        let shrinkage = self.inner.len() - self.len;
        let shrinkage_start = ::std::cmp::min(offset, shrinkage);

        // Create two vectors with correct ordering
        let mut split = self.inner.split_off(offset);

        // Truncate the buffers
        let len = self.inner.len();
        let split_len = split.len();
        self.inner.truncate(len - shrinkage_start);
        split.truncate(split_len - (shrinkage - shrinkage_start));

        // Merge buffers again and reset zero
        self.zero = self.inner.len();
        self.inner.append(&mut split);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn raw_len(&self) -> usize {
        self.inner.len()
    }

    /// Compute actual index in underlying storage given the requested index.
    fn compute_index(&self, requested: usize) -> usize {
        use ::std::hint::unreachable_unchecked;

        // This prevents an extra branch being inserted which checks for the
        // divisor to be zero thus making a % b generate equivalent code as in C
        if self.inner.len() == 0 {
            unsafe { unreachable_unchecked(); }
        }

        (requested + self.zero) % self.inner.len()
    }

    #[inline]
    pub fn line_offset(&self) -> usize {
        self.inner.len() + self.zero + *self.visible_lines
    }

    pub fn compute_line_index(&self, a: Line) -> usize {
        (self.line_offset() - *a) % self.inner.len()
    }

    pub fn swap_lines(&mut self, a: Line, b: Line) {
        let offset = self.inner.len() + self.zero + *self.visible_lines;
        let a = (offset - *a) % self.inner.len();
        let b = (offset - *b) % self.inner.len();
        self.inner.swap(a, b);
    }

    /// Iterator over *logical* entries in the storage
    ///
    /// This *does not* iterate over hidden entries.
    pub fn iter_mut(&mut self) -> IterMut<T> {
        IterMut { storage: self, index: 0 }
    }

    /// Iterate over *all* entries in the underlying buffer
    ///
    /// This includes hidden entries.
    ///
    /// XXX This suggests that Storage is a leaky abstraction. Ultimately, this
    ///     is needed because of the grow lines functionality implemented on
    ///     this type, and maybe that's where the leak is necessitating this
    ///     accessor.
    pub fn iter_mut_raw<'a>(&'a mut self) -> slice::IterMut<'a, T> {
        self.inner.iter_mut()
    }

    pub fn rotate(&mut self, count: isize) {
        debug_assert!(count.abs() as usize <= self.inner.len());

        let len = self.inner.len();
        self.zero = (self.zero as isize + count + len as isize) as usize % len;
    }

    // Fast path
    pub fn rotate_up(&mut self, count: usize) {
        self.zero = (self.zero + count) % self.inner.len();
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
        len: 3,
    };

    // Grow buffer
    storage.grow_visible_lines(Line(4), "-");

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["-", "0", "1", "-"],
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
        inner: vec!["-", "0", "1"],
        zero: 1,
        visible_lines: Line(2),
        len: 3,
    };

    // Grow buffer
    storage.grow_visible_lines(Line(4), "-");

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["-", "-", "0", "1"],
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
        inner: vec!["2", "0", "1"],
        zero: 1,
        visible_lines: Line(2),
        len: 3,
    };

    // Shrink buffer
    storage.shrink_visible_lines(Line(2));

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["2", "0", "1"],
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
        inner: vec!["0", "1", "2"],
        zero: 0,
        visible_lines: Line(2),
        len: 3,
    };

    // Shrink buffer
    storage.shrink_visible_lines(Line(2));

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["0", "1", "2"],
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
        inner: vec!["4", "5", "0", "1", "2", "3"],
        zero: 2,
        visible_lines: Line(5),
        len: 6,
    };

    // Shrink buffer
    storage.shrink_visible_lines(Line(2));

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["4", "5", "0", "1", "2", "3"],
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
        inner: vec!["4", "5", "0", "1", "2", "3"],
        zero: 2,
        visible_lines: Line(1),
        len: 2,
    };

    // Truncate buffer
    storage.truncate();

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["0", "1"],
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
        inner: vec!["1", "2", "0"],
        zero: 2,
        visible_lines: Line(1),
        len: 2,
    };

    // Truncate buffer
    storage.truncate();

    // Make sure the result is correct
    let expected = Storage {
        inner: vec!["1", "0"],
        zero: 1,
        visible_lines: Line(1),
        len: 2,
    };
    assert_eq!(storage.visible_lines, expected.visible_lines);
    assert_eq!(storage.inner, expected.inner);
    assert_eq!(storage.zero, expected.zero);
    assert_eq!(storage.len, expected.len);
}
