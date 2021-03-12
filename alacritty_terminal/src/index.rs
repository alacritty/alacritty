//! Line and Column newtypes for strongly typed tty/grid/terminal APIs.

/// Indexing types and implementations for Grid and Line.
use std::cmp::{Ord, Ordering};
use std::fmt;
use std::ops::{self, Add, AddAssign, Deref, Sub, SubAssign};

use serde::{Deserialize, Serialize};

use crate::grid::Dimensions;

/// The side of a cell.
pub type Side = Direction;

/// Horizontal direction.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    pub fn opposite(self) -> Self {
        match self {
            Side::Right => Side::Left,
            Side::Left => Side::Right,
        }
    }
}

/// Behavior for handling grid boundaries.
pub enum Boundary {
    /// Clamp to grid boundaries.
    ///
    /// When an operation exceeds the grid boundaries, the last point will be returned no matter
    /// how far the boundaries were exceeded.
    Clamp,

    /// Wrap around grid bondaries.
    ///
    /// When an operation exceeds the grid boundaries, the point will wrap around the entire grid
    /// history and continue at the other side.
    Wrap,
}

/// Index in the grid using row, column notation.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct Point<L = Line> {
    pub line: L,
    pub column: Column,
}

impl<L> Point<L> {
    pub fn new(line: L, col: Column) -> Point<L> {
        Point { line, column: col }
    }

    #[inline]
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn sub(mut self, num_cols: Column, rhs: usize) -> Self
    where
        L: Default + PartialOrd<usize> + SubAssign<usize>,
    {
        let num_cols = num_cols.0;
        let line_changes = (rhs + num_cols - 1).saturating_sub(self.column.0) / num_cols;
        if self.line >= line_changes {
            self.line -= line_changes;
            self.column = Column((num_cols + self.column.0 - rhs % num_cols) % num_cols);
            self
        } else {
            Point::new(L::default(), Column(0))
        }
    }

    #[inline]
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn add(mut self, num_cols: Column, rhs: usize) -> Self
    where
        L: AddAssign<usize>
    {
        let num_cols = num_cols.0;
        self.line += (rhs + self.column.0) / num_cols;
        self.column = Column((self.column.0 + rhs) % num_cols);
        self
    }
}

impl Point<usize> {
    #[inline]
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn sub_absolute<D>(mut self, dimensions: &D, boundary: Boundary, rhs: usize) -> Point<usize>
    where
        D: Dimensions,
    {
        let total_lines = dimensions.total_lines();
        let num_cols = dimensions.cols().0;

        self.line += (rhs + num_cols - 1).saturating_sub(self.column.0) / num_cols;
        self.column = Column((num_cols + self.column.0 - rhs % num_cols) % num_cols);

        if self.line >= total_lines {
            match boundary {
                Boundary::Clamp => Point::new(total_lines - 1, Column(0)),
                Boundary::Wrap => Point::new(self.line - total_lines, self.column),
            }
        } else {
            self
        }
    }

    #[inline]
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn add_absolute<D>(mut self, dimensions: &D, boundary: Boundary, rhs: usize) -> Point<usize>
    where
        D: Dimensions,
    {
        let num_cols = dimensions.cols();

        let line_delta = (rhs + self.column.0) / num_cols.0;

        if self.line >= line_delta {
            self.line -= line_delta;
            self.column = Column((self.column.0 + rhs) % num_cols.0);
            self
        } else {
            match boundary {
                Boundary::Clamp => Point::new(0, num_cols - 1),
                Boundary::Wrap => {
                    let col = Column((self.column.0 + rhs) % num_cols.0);
                    let line = dimensions.total_lines() + self.line - line_delta;
                    Point::new(line, col)
                },
            }
        }
    }
}

impl PartialOrd for Point {
    fn partial_cmp(&self, other: &Point) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Point {
    fn cmp(&self, other: &Point) -> Ordering {
        match (self.line.cmp(&other.line), self.column.cmp(&other.column)) {
            (Ordering::Equal, ord) | (ord, _) => ord,
        }
    }
}

impl PartialOrd for Point<usize> {
    fn partial_cmp(&self, other: &Point<usize>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Point<usize> {
    fn cmp(&self, other: &Point<usize>) -> Ordering {
        match (self.line.cmp(&other.line), self.column.cmp(&other.column)) {
            (Ordering::Equal, ord) => ord,
            (Ordering::Less, _) => Ordering::Greater,
            (Ordering::Greater, _) => Ordering::Less,
        }
    }
}

impl From<Point<usize>> for Point<isize> {
    fn from(point: Point<usize>) -> Self {
        Point::new(point.line as isize, point.column)
    }
}

impl From<Point<usize>> for Point {
    fn from(point: Point<usize>) -> Self {
        Point::new(Line(point.line as isize), point.column)
    }
}

/// A line.
///
/// Newtype to avoid passing values incorrectly.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq, Default, Ord, PartialOrd)]
pub struct Line(pub isize);

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for Line {
    fn from(source: usize) -> Self {
        Self(source as isize)
    }
}

impl ops::Add<usize> for Line {
    type Output = Line;

    #[inline]
    fn add(self, rhs: usize) -> Line {
        self + rhs as isize
    }
}

impl AddAssign<usize> for Line {
    #[inline]
    fn add_assign(&mut self, rhs: usize) {
        *self += rhs as isize;
    }
}

impl ops::Sub<usize> for Line {
    type Output = Line;

    #[inline]
    fn sub(self, rhs: usize) -> Line {
        self - rhs as isize
    }
}

impl SubAssign<usize> for Line {
    #[inline]
    fn sub_assign(&mut self, rhs: usize) {
        *self -= rhs as isize;
    }
}

impl PartialOrd<usize> for Line {
    #[inline]
    fn partial_cmp(&self, other: &usize) -> Option<Ordering> {
        self.0.partial_cmp(&(*other as isize))
    }
}

impl PartialEq<usize> for Line {
    #[inline]
    fn eq(&self, other: &usize) -> bool {
        self.0.eq(&(*other as isize))
    }
}

/// A column.
///
/// Newtype to avoid passing values incorrectly.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq, Default, Ord, PartialOrd)]
pub struct Column(pub usize);

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
//
// implements binary operators "&T op U", "T op &U", "&T op &U"
// based on "T op U" where T and U are expected to be `Copy`able
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
    };
}

macro_rules! ops {
    ($ty:ty, $construct:expr, $primitive:ty) => {
        forward_ref_binop!(impl Add, add for $ty, $ty);

        impl Deref for $ty {
            type Target = $primitive;

            #[inline]
            fn deref(&self) -> &$primitive {
                &self.0
            }
        }

        impl From<$primitive> for $ty {
            #[inline]
            fn from(val: $primitive) -> $ty {
                $construct(val)
            }
        }

        impl ops::Add<$ty> for $ty {
            type Output = $ty;

            #[inline]
            fn add(self, rhs: $ty) -> $ty {
                $construct(self.0 + rhs.0)
            }
        }

        impl AddAssign<$ty> for $ty {
            #[inline]
            fn add_assign(&mut self, rhs: $ty) {
                self.0 += rhs.0;
            }
        }

        impl Add<$primitive> for $ty {
            type Output = $ty;

            #[inline]
            fn add(self, rhs: $primitive) -> $ty {
                $construct(self.0 + rhs)
            }
        }

        impl AddAssign<$primitive> for $ty {
            #[inline]
            fn add_assign(&mut self, rhs: $primitive) {
                self.0 += rhs
            }
        }

        impl ops::Sub<$ty> for $ty {
            type Output = $ty;

            #[inline]
            fn sub(self, rhs: $ty) -> $ty {
                $construct(self.0 - rhs.0)
            }
        }

        impl SubAssign<$ty> for $ty {
            #[inline]
            fn sub_assign(&mut self, rhs: $ty) {
                self.0 -= rhs.0;
            }
        }

        impl Sub<$primitive> for $ty {
            type Output = $ty;

            #[inline]
            fn sub(self, rhs: $primitive) -> $ty {
                $construct(self.0 - rhs)
            }
        }

        impl SubAssign<$primitive> for $ty {
            #[inline]
            fn sub_assign(&mut self, rhs: $primitive) {
                self.0 -= rhs
            }
        }

        impl PartialEq<$ty> for $primitive {
            #[inline]
            fn eq(&self, other: &$ty) -> bool {
                self.eq(&other.0)
            }
        }

        impl PartialEq<$primitive> for $ty {
            #[inline]
            fn eq(&self, other: &$primitive) -> bool {
                self.0.eq(other)
            }
        }

        impl PartialOrd<$ty> for $primitive {
            #[inline]
            fn partial_cmp(&self, other: &$ty) -> Option<Ordering> {
                self.partial_cmp(&other.0)
            }
        }

        impl PartialOrd<$primitive> for $ty {
            #[inline]
            fn partial_cmp(&self, other: &$primitive) -> Option<Ordering> {
                self.0.partial_cmp(other)
            }
        }
    }
}

ops!(Column, Column, usize);
ops!(Line, Line, isize);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_ordering() {
        assert!(Point::new(Line(0), Column(0)) == Point::new(Line(0), Column(0)));
        assert!(Point::new(Line(1), Column(0)) > Point::new(Line(0), Column(0)));
        assert!(Point::new(Line(0), Column(1)) > Point::new(Line(0), Column(0)));
        assert!(Point::new(Line(1), Column(1)) > Point::new(Line(0), Column(0)));
        assert!(Point::new(Line(1), Column(1)) > Point::new(Line(0), Column(1)));
        assert!(Point::new(Line(1), Column(1)) > Point::new(Line(1), Column(0)));
        assert!(Point::new(Line(0), Column(0)) > Point::new(Line(-1), Column(0)));
    }

    #[test]
    fn sub() {
        let num_cols = Column(42);
        let point = Point::new(0, Column(13));

        let result = point.sub(num_cols, 1);

        assert_eq!(result, Point::new(0, point.column - 1));
    }

    #[test]
    fn sub_wrap() {
        let num_cols = Column(42);
        let point = Point::new(1, Column(0));

        let result = point.sub(num_cols, 1);

        assert_eq!(result, Point::new(0, num_cols - 1));
    }

    #[test]
    fn sub_clamp() {
        let num_cols = Column(42);
        let point = Point::new(0, Column(0));

        let result = point.sub(num_cols, 1);

        assert_eq!(result, point);
    }

    #[test]
    fn add() {
        let num_cols = Column(42);
        let point = Point::new(0, Column(13));

        let result = point.add(num_cols, 1);

        assert_eq!(result, Point::new(0, point.column + 1));
    }

    #[test]
    fn add_wrap() {
        let num_cols = Column(42);
        let point = Point::new(0, num_cols - 1);

        let result = point.add(num_cols, 1);

        assert_eq!(result, Point::new(1, Column(0)));
    }

    #[test]
    fn add_absolute() {
        let point = Point::new(0, Column(13));

        let result = point.add_absolute(&(1, Column(42)), Boundary::Clamp, 1);

        assert_eq!(result, Point::new(0, point.column + 1));
    }

    #[test]
    fn add_absolute_wrapline() {
        let point = Point::new(1, Column(41));

        let result = point.add_absolute(&(2, Column(42)), Boundary::Clamp, 1);

        assert_eq!(result, Point::new(0, Column(0)));
    }

    #[test]
    fn add_absolute_multiline_wrapline() {
        let point = Point::new(2, Column(9));

        let result = point.add_absolute(&(3, Column(10)), Boundary::Clamp, 11);

        assert_eq!(result, Point::new(0, Column(0)));
    }

    #[test]
    fn add_absolute_clamp() {
        let point = Point::new(0, Column(41));

        let result = point.add_absolute(&(1, Column(42)), Boundary::Clamp, 1);

        assert_eq!(result, point);
    }

    #[test]
    fn add_absolute_wrap() {
        let point = Point::new(0, Column(41));

        let result = point.add_absolute(&(3, Column(42)), Boundary::Wrap, 1);

        assert_eq!(result, Point::new(2, Column(0)));
    }

    #[test]
    fn add_absolute_multiline_wrap() {
        let point = Point::new(0, Column(9));

        let result = point.add_absolute(&(3, Column(10)), Boundary::Wrap, 11);

        assert_eq!(result, Point::new(1, Column(0)));
    }

    #[test]
    fn sub_absolute() {
        let point = Point::new(0, Column(13));

        let result = point.sub_absolute(&(1, Column(42)), Boundary::Clamp, 1);

        assert_eq!(result, Point::new(0, point.column - 1));
    }

    #[test]
    fn sub_absolute_wrapline() {
        let point = Point::new(0, Column(0));

        let result = point.sub_absolute(&(2, Column(42)), Boundary::Clamp, 1);

        assert_eq!(result, Point::new(1, Column(41)));
    }

    #[test]
    fn sub_absolute_multiline_wrapline() {
        let point = Point::new(0, Column(0));

        let result = point.sub_absolute(&(3, Column(10)), Boundary::Clamp, 11);

        assert_eq!(result, Point::new(2, Column(9)));
    }

    #[test]
    fn sub_absolute_wrap() {
        let point = Point::new(2, Column(0));

        let result = point.sub_absolute(&(3, Column(42)), Boundary::Wrap, 1);

        assert_eq!(result, Point::new(0, Column(41)));
    }

    #[test]
    fn sub_absolute_multiline_wrap() {
        let point = Point::new(2, Column(0));

        let result = point.sub_absolute(&(3, Column(10)), Boundary::Wrap, 11);

        assert_eq!(result, Point::new(1, Column(9)));
    }
}
