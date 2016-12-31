// Copyright 2011 Google Inc.
//           2013 Jack Lloyd
//           2013-2014 Steven Fackler
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
use std::cell::UnsafeCell;

/// This is intended to be used as the inner type for `FooRef` types converted from raw C pointers.
/// It has an `UnsafeCell` internally to inform the compiler about aliasability and doesn't
/// implement `Copy`, so it can't be dereferenced.
pub struct Opaque(UnsafeCell<()>);

/// A type implemented by wrappers over foreign types.
///
/// This should not be implemented by anything outside of this crate; new methods may be added at
/// any time.
pub trait ForeignType: Sized {
    /// The raw C type.
    type CType;

    /// The type representing a reference to this type.
    type Ref: ForeignTypeRef<CType = Self::CType>;

    /// Constructs an instance of this type from its raw type.
    unsafe fn from_ptr(ptr: *mut Self::CType) -> Self;
}

/// A trait implemented by types which reference borrowed foreign types.
///
/// This should not be implemented by anything outside of this crate; new methods may be added at
/// any time.
pub trait ForeignTypeRef: Sized {
    /// The raw C type.
    type CType;

    /// Constructs a shared instance of this type from its raw type.
    unsafe fn from_ptr<'a>(ptr: *mut Self::CType) -> &'a Self {
        &*(ptr as *mut _)
    }

    /// Constructs a mutable reference of this type from its raw type.
    unsafe fn from_ptr_mut<'a>(ptr: *mut Self::CType) -> &'a mut Self {
        &mut *(ptr as *mut _)
    }

    /// Returns a raw pointer to the wrapped value.
    fn as_ptr(&self) -> *mut Self::CType {
        self as *const _ as *mut _
    }
}

#[macro_export]
macro_rules! ffi_type {
    ($n:ident, $r:ident, $c:path, $d:path) => {
        pub struct $n(*mut $c);

        impl $crate::ForeignType for $n {
            type CType = $c;
            type Ref = $r;

            unsafe fn from_ptr(ptr: *mut $c) -> $n {
                $n(ptr)
            }
        }

        impl Drop for $n {
            fn drop(&mut self) {
                unsafe { $d(self.0) }
            }
        }

        impl ::std::ops::Deref for $n {
            type Target = $r;

            fn deref(&self) -> &$r {
                unsafe { $crate::ForeignTypeRef::from_ptr(self.0) }
            }
        }

        impl ::std::ops::DerefMut for $n {
            fn deref_mut(&mut self) -> &mut $r {
                unsafe { $crate::ForeignTypeRef::from_ptr_mut(self.0) }
            }
        }

        pub struct $r($crate::Opaque);

        impl $crate::ForeignTypeRef for $r {
            type CType = $c;
        }
    }
}
