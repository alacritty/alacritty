// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Defines the Row type which makes up lines in the grid

use std::cmp::{max, min};
use std::ops::{Index, IndexMut};
use std::ops::{Range, RangeFrom, RangeFull, RangeTo, RangeToInclusive};
use std::slice;

use crate::grid::GridCell;
use crate::index::Column;

/// A row in the grid
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Row<T> {
    inner: Vec<T>,

    /// occupied entries
    ///
    /// Semantically, this value can be understood as the **end** of an
    /// Exclusive Range. Thus,
    ///
    /// - Zero means there are no occupied entries
    /// - 1 means there is a value at index zero, but nowhere else
    /// - `occ == inner.len` means every value is occupied
    pub(crate) occ: usize,
}

impl<T: PartialEq> PartialEq for Row<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T: Copy> Row<T> {
    pub fn new(columns: Column, template: &T) -> Row<T> {
        Row { inner: vec![*template; *columns], occ: 0 }
    }

    pub fn grow(&mut self, cols: Column, template: &T) {
        if self.inner.len() >= cols.0 {
            return;
        }

        self.inner.append(&mut vec![*template; cols.0 - self.len()]);
    }

    pub fn shrink(&mut self, cols: Column) -> Option<Vec<T>>
    where
        T: GridCell,
    {
        if self.inner.len() <= cols.0 {
            return None;
        }

        // Split off cells for a new row
        let mut new_row = self.inner.split_off(cols.0);
        let index = new_row.iter().rposition(|c| !c.is_empty()).map(|i| i + 1).unwrap_or(0);
        new_row.truncate(index);

        self.occ = min(self.occ, *cols);

        if new_row.is_empty() {
            None
        } else {
            Some(new_row)
        }
    }

    /// Resets contents to the contents of `other`
    pub fn reset(&mut self, other: &T) {
        for item in &mut self.inner[..] {
            *item = *other;
        }
        self.occ = 0;
    }
}

#[allow(clippy::len_without_is_empty)]
impl<T> Row<T> {
    #[inline]
    pub fn from_vec(vec: Vec<T>, occ: usize) -> Row<T> {
        Row { inner: vec, occ }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn last(&self) -> Option<&T> {
        self.inner.last()
    }

    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.occ = self.inner.len();
        self.inner.last_mut()
    }

    #[inline]
    pub fn append(&mut self, vec: &mut Vec<T>)
    where
        T: GridCell,
    {
        self.inner.append(vec);
        self.occ = self.inner.iter().rposition(|c| !c.is_empty()).map(|i| i + 1).unwrap_or(0);
    }

    #[inline]
    pub fn append_front(&mut self, mut vec: Vec<T>) {
        self.occ += vec.len();
        vec.append(&mut self.inner);
        self.inner = vec;
    }

    #[inline]
    pub fn is_empty(&self) -> bool
    where
        T: GridCell,
    {
        self.inner.iter().all(GridCell::is_empty)
    }

    #[inline]
    pub fn front_split_off(&mut self, at: usize) -> Vec<T> {
        self.occ = self.occ.saturating_sub(at);

        let mut split = self.inner.split_off(at);
        std::mem::swap(&mut split, &mut self.inner);
        split
    }
}

impl<'a, T> IntoIterator for &'a mut Row<T> {
    type IntoIter = slice::IterMut<'a, T>;
    type Item = &'a mut T;

    #[inline]
    fn into_iter(self) -> slice::IterMut<'a, T> {
        self.occ = self.len();
        self.inner.iter_mut()
    }
}

impl<T> Index<Column> for Row<T> {
    type Output = T;

    #[inline]
    fn index(&self, index: Column) -> &T {
        &self.inner[index.0]
    }
}

impl<T> IndexMut<Column> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: Column) -> &mut T {
        self.occ = max(self.occ, *index + 1);
        &mut self.inner[index.0]
    }
}

// -----------------------------------------------------------------------------
// Index ranges of columns
// -----------------------------------------------------------------------------

impl<T> Index<Range<Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: Range<Column>) -> &[T] {
        &self.inner[(index.start.0)..(index.end.0)]
    }
}

impl<T> IndexMut<Range<Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: Range<Column>) -> &mut [T] {
        self.occ = max(self.occ, *index.end);
        &mut self.inner[(index.start.0)..(index.end.0)]
    }
}

impl<T> Index<RangeTo<Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeTo<Column>) -> &[T] {
        &self.inner[..(index.end.0)]
    }
}

impl<T> IndexMut<RangeTo<Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeTo<Column>) -> &mut [T] {
        self.occ = max(self.occ, *index.end);
        &mut self.inner[..(index.end.0)]
    }
}

impl<T> Index<RangeFrom<Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeFrom<Column>) -> &[T] {
        &self.inner[(index.start.0)..]
    }
}

impl<T> IndexMut<RangeFrom<Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeFrom<Column>) -> &mut [T] {
        self.occ = self.len();
        &mut self.inner[(index.start.0)..]
    }
}

impl<T> Index<RangeFull> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, _: RangeFull) -> &[T] {
        &self.inner[..]
    }
}

impl<T> IndexMut<RangeFull> for Row<T> {
    #[inline]
    fn index_mut(&mut self, _: RangeFull) -> &mut [T] {
        self.occ = self.len();
        &mut self.inner[..]
    }
}

impl<T> Index<RangeToInclusive<Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeToInclusive<Column>) -> &[T] {
        &self.inner[..=(index.end.0)]
    }
}

impl<T> IndexMut<RangeToInclusive<Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeToInclusive<Column>) -> &mut [T] {
        self.occ = max(self.occ, *index.end);
        &mut self.inner[..=(index.end.0)]
    }
}
