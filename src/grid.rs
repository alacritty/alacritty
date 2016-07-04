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

//! A generic 2d grid implementation optimized for use in a terminal.
//!
//! The current implementation uses a vector of vectors to store cell data.
//! Reimplementing the store as a single contiguous vector may be desirable in
//! the future. Rotation and indexing would need to be reconsidered at that
//! time; rotation currently reorganize Vecs in the lines Vec, and indexing with
//! ranges is currently supported.

use std::ops::{Deref, DerefMut, Range, RangeTo, RangeFrom, RangeFull, Index, IndexMut};
use std::cmp::Ordering;
use std::slice::{self, Iter, IterMut};
use std::iter::IntoIterator;
use std::borrow::ToOwned;

use util::Rotate;

/// Indexing types and implementations for Grid and Line
pub mod index {
    use std::fmt;
    use std::iter::Step;
    use std::num::{One, Zero};
    use std::ops::{self, Deref, Add};

    /// Index in the grid using row, column notation
    #[derive(Debug, Clone, Default, Eq, PartialEq)]
    pub struct Cursor {
        pub line: Line,
        pub col: Column,
    }

    /// A line
    ///
    /// Newtype to avoid passing values incorrectly
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Default, Ord, PartialOrd)]
    pub struct Line(pub usize);

    impl fmt::Display for Line {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "Line({})", self.0)
        }
    }

    /// A column
    ///
    /// Newtype to avoid passing values incorrectly
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Default, Ord, PartialOrd)]
    pub struct Column(pub usize);

    impl fmt::Display for Column {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "Column({})", self.0)
        }
    }

    /// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
    /// file at the top-level directory of this distribution and at
    /// http://rust-lang.org/COPYRIGHT.
    ///
    /// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
    /// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
    /// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
    /// option. This file may not be copied, modified, or distributed
    /// except according to those terms.
    ///
    /// implements binary operators "&T op U", "T op &U", "&T op &U"
    /// based on "T op U" where T and U are expected to be `Copy`able
    macro_rules! forward_ref_binop {
        (impl $imp:ident, $method:ident for $t:ty, $u:ty) => {
            impl<'a> $imp<$u> for &'a $t {
                type Output = <$t as $imp<$u>>::Output;

                #[inline]
                fn $method(self, other: $u) -> <$t as $imp<$u>>::Output {
                    $imp::$method(*self, other)
                }
            }

            impl<'a> $imp<&'a $u> for $t {
                type Output = <$t as $imp<$u>>::Output;

                #[inline]
                fn $method(self, other: &'a $u) -> <$t as $imp<$u>>::Output {
                    $imp::$method(self, *other)
                }
            }

            impl<'a, 'b> $imp<&'a $u> for &'b $t {
                type Output = <$t as $imp<$u>>::Output;

                #[inline]
                fn $method(self, other: &'a $u) -> <$t as $imp<$u>>::Output {
                    $imp::$method(*self, *other)
                }
            }
        }
    }

    /// Macro for deriving deref
    macro_rules! deref {
        ($ty:ty, $target:ty) => {
            impl Deref for $ty {
                type Target = $target;

                #[inline]
                fn deref(&self) -> &$target {
                    &self.0
                }
            }
        }
    }

    macro_rules! add {
        ($ty:ty, $construct:expr) => {
            impl ops::Add<$ty> for $ty {
                type Output = $ty;

                #[inline]
                fn add(self, rhs: $ty) -> $ty {
                    $construct(self.0 + rhs.0)
                }
            }
        }
    }

    macro_rules! sub {
        ($ty:ty, $construct:expr) => {
            impl ops::Sub<$ty> for $ty {
                type Output = $ty;

                #[inline]
                fn sub(self, rhs: $ty) -> $ty {
                    $construct(self.0 - rhs.0)
                }
            }
        }
    }

    macro_rules! zero_one {
        ($ty:ty, $construct:expr) => {
            impl One for $ty {
                fn one() -> $ty {
                    $construct(1)
                }
            }

            impl Zero for $ty {
                fn zero() -> $ty {
                    $construct(0)
                }
            }
        }
    }

    macro_rules! ops {
        ($ty:ty, $construct:expr) => {
            add!($ty, $construct);
            sub!($ty, $construct);
            zero_one!($ty, $construct);
            deref!($ty, usize);
            forward_ref_binop!(impl Add, add for $ty, $ty);

            impl Step for $ty {
                fn step(&self, by: &$ty) -> Option<$ty> {
                    Some(*self + *by)
                }

                #[inline]
                #[allow(trivial_numeric_casts)]
                fn steps_between(start: &$ty, end: &$ty, by: &$ty) -> Option<usize> {
                    if *by == $construct(0) { return None; }
                    if *start < *end {
                        // Note: We assume $t <= usize here
                        let diff = (*end - *start).0;
                        let by = by.0;
                        if diff % by > 0 {
                            Some(diff / by + 1)
                        } else {
                            Some(diff / by)
                        }
                    } else {
                        Some(0)
                    }
                }
            }

            impl ops::AddAssign<$ty> for $ty {
                #[inline]
                fn add_assign(&mut self, rhs: $ty) {
                    self.0 += rhs.0
                }
            }

            impl ops::SubAssign<$ty> for $ty {
                #[inline]
                fn sub_assign(&mut self, rhs: $ty) {
                    self.0 -= rhs.0
                }
            }

            impl ops::AddAssign<usize> for $ty {
                #[inline]
                fn add_assign(&mut self, rhs: usize) {
                    self.0 += rhs
                }
            }

            impl ops::SubAssign<usize> for $ty {
                #[inline]
                fn sub_assign(&mut self, rhs: usize) {
                    self.0 -= rhs
                }
            }

            impl From<usize> for $ty {
                #[inline]
                fn from(val: usize) -> $ty {
                    $construct(val)
                }
            }

            impl ops::Add<usize> for $ty {
                type Output = $ty;

                #[inline]
                fn add(self, rhs: usize) -> $ty {
                    $construct(self.0 + rhs)
                }
            }

            impl ops::Sub<usize> for $ty {
                type Output = $ty;

                #[inline]
                fn sub(self, rhs: usize) -> $ty {
                    $construct(self.0 - rhs)
                }
            }
        }
    }

    ops!(Line, Line);
    ops!(Column, Column);
}

use self::index::Cursor;

/// Represents the terminal display contents
#[derive(Clone)]
pub struct Grid<T> {
    /// Lines in the grid. Each row holds a list of cells corresponding to the
    /// columns in that row.
    raw: Vec<Row<T>>,

    /// Number of columns
    cols: index::Column,

    /// Number of lines.
    ///
    /// Invariant: lines is equivalent to raw.len()
    lines: index::Line,
}

impl<T: Clone> Grid<T> {
    pub fn new(lines: index::Line, cols: index::Column, template: &T) -> Grid<T> {
        let mut raw = Vec::with_capacity(*lines);
        for _ in index::Line(0)..lines {
            raw.push(Row::new(cols, template));
        }

        Grid {
            raw: raw,
            cols: cols,
            lines: lines,
        }
    }

    pub fn resize(&mut self, lines: index::Line, cols: index::Column, template: &T) {
        // Check that there's actually work to do and return early if not
        if lines == self.lines && cols == self.cols {
            return;
        }

        match self.lines.cmp(&lines) {
            Ordering::Less => self.grow_lines(lines, template),
            Ordering::Greater => self.shrink_lines(lines),
            Ordering::Equal => (),
        }

        match self.cols.cmp(&cols) {
            Ordering::Less => self.grow_cols(cols, template),
            Ordering::Greater => self.shrink_cols(cols),
            Ordering::Equal => (),
        }
    }

    fn grow_lines(&mut self, lines: index::Line, template: &T) {
        for _ in self.num_lines()..lines {
            self.raw.push(Row::new(self.cols, template));
        }

        self.lines = lines;
    }

    fn grow_cols(&mut self, cols: index::Column, template: &T) {
        for row in self.lines_mut() {
            row.grow(cols, template);
        }

        self.cols = cols;
    }

}

impl<T> Grid<T> {
    #[inline]
    pub fn lines(&self) -> Iter<Row<T>> {
        self.raw.iter()
    }

    #[inline]
    pub fn lines_mut(&mut self) -> IterMut<Row<T>> {
        self.raw.iter_mut()
    }

    #[inline]
    pub fn num_lines(&self) -> index::Line {
        index::Line(self.raw.len())
    }

    #[inline]
    pub fn num_cols(&self) -> index::Column {
        index::Column(self.raw[0].len())
    }

    #[inline]
    pub fn scroll(&mut self, region: Range<index::Line>, positions: isize) {
        self[region].rotate(positions)
    }

    #[inline]
    pub fn clear<F: Fn(&mut T)>(&mut self, func: F) {
        let region = index::Line(0)..self.num_lines();
        self.clear_region(region, func);
    }

    fn shrink_lines(&mut self, lines: index::Line) {
        while index::Line(self.raw.len()) != lines {
            self.raw.pop();
        }

        self.lines = lines;
    }

    fn shrink_cols(&mut self, cols: index::Column) {
        for row in self.lines_mut() {
            row.shrink(cols);
        }

        self.cols = cols;
    }
}

impl<T> Index<index::Line> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index<'a>(&'a self, index: index::Line) -> &'a Row<T> {
        &self.raw[index.0]
    }
}

impl<T> IndexMut<index::Line> for Grid<T> {
    #[inline]
    fn index_mut<'a>(&'a mut self, index: index::Line) -> &'a mut Row<T> {
        &mut self.raw[index.0]
    }
}

impl<'cursor, T> Index<&'cursor Cursor> for Grid<T> {
    type Output = T;

    #[inline]
    fn index<'a, 'b>(&'a self, cursor: &'b Cursor) -> &'a T {
        &self.raw[cursor.line.0][cursor.col]
    }
}

impl<'cursor, T> IndexMut<&'cursor Cursor> for Grid<T> {
    #[inline]
    fn index_mut<'a, 'b>(&'a mut self, cursor: &'b Cursor) -> &'a mut T {
        &mut self.raw[cursor.line.0][cursor.col]
    }
}

/// A row in the grid
#[derive(Clone)]
pub struct Row<T>(Vec<T>);

impl<T: Clone> Row<T> {
    pub fn new(columns: index::Column, template: &T) -> Row<T> {
        Row(vec![template.to_owned(); *columns])
    }

    pub fn grow(&mut self, cols: index::Column, template: &T) {
        while self.len() != *cols {
            self.push(template.to_owned());
        }
    }
}

impl<T> Row<T> {
    pub fn shrink(&mut self, cols: index::Column) {
        while self.len() != *cols {
            self.pop();
        }
    }

    #[inline]
    pub fn cells(&self) -> Iter<T> {
        self.0.iter()
    }

    #[inline]
    pub fn cells_mut(&mut self) -> IterMut<T> {
        self.0.iter_mut()
    }
}

impl<'a, T> IntoIterator for &'a Row<T> {
    type Item = &'a T;
    type IntoIter = slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> slice::Iter<'a, T> {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Row<T> {
    type Item = &'a mut T;
    type IntoIter = slice::IterMut<'a, T>;

    #[inline]
    fn into_iter(mut self) -> slice::IterMut<'a, T> {
        self.iter_mut()
    }
}

impl<T> Deref for Row<T> {
    type Target = Vec<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Row<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Index<index::Column> for Row<T> {
    type Output = T;

    #[inline]
    fn index<'a>(&'a self, index: index::Column) -> &'a T {
        &self.0[index.0]
    }
}

impl<T> IndexMut<index::Column> for Row<T> {
    #[inline]
    fn index_mut<'a>(&'a mut self, index: index::Column) -> &'a mut T {
        &mut self.0[index.0]
    }
}

macro_rules! row_index_range {
    ($range:ty) => {
        impl<T> Index<$range> for Row<T> {
            type Output = [T];

            #[inline]
            fn index<'a>(&'a self, index: $range) -> &'a [T] {
                &self.0[index]
            }
        }

        impl<T> IndexMut<$range> for Row<T> {
            #[inline]
            fn index_mut<'a>(&'a mut self, index: $range) -> &'a mut [T] {
                &mut self.0[index]
            }
        }
    }
}

row_index_range!(Range<usize>);
row_index_range!(RangeTo<usize>);
row_index_range!(RangeFrom<usize>);
row_index_range!(RangeFull);

// -------------------------------------------------------------------------------------------------
// Row ranges for Grid
// -------------------------------------------------------------------------------------------------

impl<T> Index<Range<index::Line>> for Grid<T> {
    type Output = [Row<T>];

    #[inline]
    fn index(&self, index: Range<index::Line>) -> &[Row<T>] {
        &self.raw[(index.start.0)..(index.end.0)]
    }
}

impl<T> IndexMut<Range<index::Line>> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: Range<index::Line>) -> &mut [Row<T>] {
        &mut self.raw[(index.start.0)..(index.end.0)]
    }
}

impl<T> Index<RangeTo<index::Line>> for Grid<T> {
    type Output = [Row<T>];

    #[inline]
    fn index(&self, index: RangeTo<index::Line>) -> &[Row<T>] {
        &self.raw[..(index.end.0)]
    }
}

impl<T> IndexMut<RangeTo<index::Line>> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeTo<index::Line>) -> &mut [Row<T>] {
        &mut self.raw[..(index.end.0)]
    }
}

impl<T> Index<RangeFrom<index::Line>> for Grid<T> {
    type Output = [Row<T>];

    #[inline]
    fn index(&self, index: RangeFrom<index::Line>) -> &[Row<T>] {
        &self.raw[(index.start.0)..]
    }
}

impl<T> IndexMut<RangeFrom<index::Line>> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeFrom<index::Line>) -> &mut [Row<T>] {
        &mut self.raw[(index.start.0)..]
    }
}

// -------------------------------------------------------------------------------------------------
// Column ranges for Row
// -------------------------------------------------------------------------------------------------

impl<T> Index<Range<index::Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: Range<index::Column>) -> &[T] {
        &self.0[(index.start.0)..(index.end.0)]
    }
}

impl<T> IndexMut<Range<index::Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: Range<index::Column>) -> &mut [T] {
        &mut self.0[(index.start.0)..(index.end.0)]
    }
}

impl<T> Index<RangeTo<index::Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeTo<index::Column>) -> &[T] {
        &self.0[..(index.end.0)]
    }
}

impl<T> IndexMut<RangeTo<index::Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeTo<index::Column>) -> &mut [T] {
        &mut self.0[..(index.end.0)]
    }
}

impl<T> Index<RangeFrom<index::Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeFrom<index::Column>) -> &[T] {
        &self.0[(index.start.0)..]
    }
}

impl<T> IndexMut<RangeFrom<index::Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeFrom<index::Column>) -> &mut [T] {
        &mut self.0[(index.start.0)..]
    }
}

pub trait ClearRegion<R, T> {
    fn clear_region<F: Fn(&mut T)>(&mut self, region: R, func: F);
}

macro_rules! clear_region_impl {
    ($range:ty) => {
        impl<T> ClearRegion<$range, T> for Grid<T> {
            fn clear_region<F: Fn(&mut T)>(&mut self, region: $range, func: F) {
                for row in self[region].iter_mut() {
                    for cell in row {
                        func(cell);
                    }
                }
            }
        }
    }
}

clear_region_impl!(Range<index::Line>);
clear_region_impl!(RangeTo<index::Line>);
clear_region_impl!(RangeFrom<index::Line>);
