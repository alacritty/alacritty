//! Synchronization types.
//!
//! Most importantly, a fair mutex is included.

use parking_lot::{Mutex, MutexGuard};

/// A fair mutex.
///
/// Uses an extra lock to ensure that if one thread is waiting that it will get
/// the lock before a single thread can re-lock it.
pub struct FairMutex<T> {
    /// Data.
    data: Mutex<T>,
    /// Next-to-access.
    next: Mutex<()>,
}

impl<T> FairMutex<T> {
    /// Create a new fair mutex.
    pub fn new(data: T) -> FairMutex<T> {
        FairMutex { data: Mutex::new(data), next: Mutex::new(()) }
    }

    /// Lock the mutex.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        // Must bind to a temporary or the lock will be freed before going
        // into data.lock().
        let _next = self.next.lock();
        self.data.lock()
    }
}
