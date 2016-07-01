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

//! Synchronization types
//!
//! Most importantly, a priority mutex is included
use std::ops::{Deref, DerefMut};

use parking_lot::{Mutex, MutexGuard};

/// A priority mutex
///
/// A triple locking strategy is used where low priority locks must go through an additional mutex
/// to access the data. The gist is
///
/// Low priority: lock low, lock next, lock data, unlock next, {do work}, unlock data, unlock low
/// High priority: lock next, lock data, unlock next, {do work}, unlock data
///
/// By keeping the low lock active while working on data, a high priority consumer has immediate
/// access to the next mutex.
pub struct PriorityMutex<T> {
    /// Data
    data: Mutex<T>,
    /// Next-to-access
    next: Mutex<()>,
    /// Low-priority access
    low: Mutex<()>,
}

/// Mutex guard for low priority locks
pub struct LowPriorityMutexGuard<'a, T: 'a> {
    data: MutexGuard<'a, T>,
    _low: MutexGuard<'a, ()>,
}

impl<'a, T> Deref for LowPriorityMutexGuard<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.data.deref()
    }
}

impl<'a, T> DerefMut for LowPriorityMutexGuard<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.data.deref_mut()
    }
}

impl<T> PriorityMutex<T> {
    /// Create a new priority mutex
    pub fn new(data: T) -> PriorityMutex<T> {
        PriorityMutex {
            data: Mutex::new(data),
            next: Mutex::new(()),
            low: Mutex::new(()),
        }
    }

    /// Lock the mutex with high priority
    pub fn lock_high(&self) -> MutexGuard<T> {
        // Must bind to a temporary or the lock will be freed before going
        // into data.lock()
        let _next = self.next.lock();
        self.data.lock()
    }

    /// Lock the mutex with low priority
    pub fn lock_low(&self) -> LowPriorityMutexGuard<T> {
        let low = self.low.lock();
        // Must bind to a temporary or the lock will be freed before going
        // into data.lock()
        let _next = self.next.lock();
        let data = self.data.lock();

        LowPriorityMutexGuard {
            data: data,
            _low: low,
        }
    }
}
