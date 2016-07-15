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

//! Line and Column newtypes for strongly typed tty/grid/terminal APIs

/// Indexing types and implementations for Grid and Line
use std::fmt;
use std::iter::Step;
use std::mem;
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

macro_rules! ops {
    ($ty:ty, $construct:expr) => {
        add!($ty, $construct);
        sub!($ty, $construct);
        deref!($ty, usize);
        forward_ref_binop!(impl Add, add for $ty, $ty);

        impl Step for $ty {
            #[inline]
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

            #[inline]
            fn steps_between_by_one(start: &$ty, end: &$ty) -> Option<usize> {
                Step::steps_between(start, end, &$construct(1))
            }

            #[inline]
            #[allow(unused_comparisons)]
            fn is_negative(&self) -> bool {
                self.0 < 0
            }

            #[inline]
            fn replace_one(&mut self) -> Self {
                mem::replace(self, $construct(0))
            }

            #[inline]
            fn replace_zero(&mut self) -> Self {
                mem::replace(self, $construct(1))
            }

            #[inline]
            fn add_one(&self) -> Self {
                *self + 1
            }

            #[inline]
            fn sub_one(&self) -> Self {
                *self - 1
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
