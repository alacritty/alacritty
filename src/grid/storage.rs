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

impl<T: PartialEq> ::std::cmp::PartialEq for Storage<T> {
    fn eq(&self, other: &Self) -> bool {
        let mut equal = true;
        for i in IndexRange(Line(0) .. self.visible_lines) {
            equal = equal && (self[i] == other[i])
        }
        equal
    }
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

    pub fn set_visible_lines(&mut self, next: Line) {
        // Change capacity to fit scrollback + screen size
        if next > self.visible_lines + 1 {
            self.inner.reserve_exact((next - (self.visible_lines + 1)).0);
        } else if next < self.visible_lines + 1 {
            let shrinkage = (self.visible_lines + 1 - next).0;
            let new_size = self.inner.capacity() - shrinkage;
            self.inner.truncate(new_size);
            self.inner.shrink_to_fit();
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
