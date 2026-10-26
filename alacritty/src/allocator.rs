//! Global allocator that waits for memory instead of aborting.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::general::MemoryWait;

/// Pause after the first failed attempt to allocate.
const FIRST_WAIT: Duration = Duration::from_millis(10);

/// Longest pause between attempts to allocate.
const MAX_WAIT: Duration = Duration::from_secs(1);

/// Value of [`WAIT_SECONDS`] for [`MemoryWait::Forever`].
const FOREVER: usize = usize::MAX;

/// Seconds to wait for one allocation before letting it fail, or [`FOREVER`].
static WAIT_SECONDS: AtomicUsize = AtomicUsize::new(wait_seconds(MemoryWait::DEFAULT));

/// Milliseconds spent waiting for memory since [`take_waited`] was last called, summed over all
/// threads.
static WAITED_MS: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: WaitingAllocator = WaitingAllocator;

/// System allocator that retries failed allocations for as long as `general.memory_wait` allows.
///
/// Rust aborts the process when an allocation fails. Running out of memory is often temporary,
/// since other processes free memory and Windows grows the page file, so waiting lets Alacritty
/// continue instead of closing every terminal in it. Allocations that can never succeed still
/// fail once the time is up.
struct WaitingAllocator;

impl WaitingAllocator {
    /// Call `allocate` until it returns a non-null pointer or the time to wait is up.
    ///
    /// This must not allocate, so the time spent waiting is only counted here.
    fn retry(mut allocate: impl FnMut() -> *mut u8) -> *mut u8 {
        let mut ptr = allocate();
        if !ptr.is_null() {
            return ptr;
        }

        let start = Instant::now();
        let mut wait = FIRST_WAIT;
        while ptr.is_null() && may_wait(start.elapsed()) {
            thread::sleep(wait);
            wait = (wait * 2).min(MAX_WAIT);
            ptr = allocate();
        }

        WAITED_MS.fetch_add(start.elapsed().as_millis() as usize, Ordering::Relaxed);

        ptr
    }
}

unsafe impl GlobalAlloc for WaitingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::retry(|| unsafe { System.alloc(layout) })
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::retry(|| unsafe { System.alloc_zeroed(layout) })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // A failed reallocation leaves the original memory untouched, so retrying is safe.
        Self::retry(|| unsafe { System.realloc(ptr, layout, new_size) })
    }
}

/// Whether an allocation that has waited for `waited` may keep waiting.
fn may_wait(waited: Duration) -> bool {
    match WAIT_SECONDS.load(Ordering::Relaxed) {
        FOREVER => true,
        seconds => waited < Duration::from_secs(seconds as u64),
    }
}

/// Value of [`WAIT_SECONDS`] for `memory_wait`.
const fn wait_seconds(memory_wait: MemoryWait) -> usize {
    match memory_wait {
        // Any wait this long is the same as waiting forever, so only `FOREVER` itself is avoided.
        MemoryWait::Seconds(seconds) if seconds >= FOREVER as u64 => FOREVER - 1,
        MemoryWait::Seconds(seconds) => seconds as usize,
        MemoryWait::Forever => FOREVER,
    }
}

/// Set how long to wait for memory when an allocation fails.
pub fn set_memory_wait(memory_wait: MemoryWait) {
    WAIT_SECONDS.store(wait_seconds(memory_wait), Ordering::Relaxed);
}

/// Time spent waiting for memory since the last call, summed over all threads.
pub fn take_waited() -> Duration {
    Duration::from_millis(WAITED_MS.swap(0, Ordering::Relaxed) as u64)
}
