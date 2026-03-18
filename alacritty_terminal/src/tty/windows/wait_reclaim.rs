//! Generic one-shot wait registration with background reclamation.
//!
//! Both the IOCP overlapped-I/O drop path (`iocp/reclaim.rs`) and the ConPTY
//! `ConnectNamedPipe` timeout path (`conpty.rs`) need the same pattern:
//!
//! 1. `RegisterWaitForSingleObject` with `WT_EXECUTEINWAITTHREAD | WT_EXECUTEONLYONCE`.
//! 2. Handle the race where the callback fires before the caller stores the wait handle
//!    (`callback_ran` flag).
//! 3. Defer `UnregisterWaitEx` to a helper thread via `QueueUserWorkItem` to avoid deadlocking the
//!    wait thread.
//!
//! This module provides [`register_wait_once`] so that pattern is written once,
//! and [`ReclaimCounters`] so the identical stats-tracking boilerplate is not
//! duplicated across subsystems.

use std::ffi::c_void;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{BOOLEAN, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Threading::{
    QueueUserWorkItem, RegisterWaitForSingleObject, UnregisterWaitEx, WT_EXECUTEDEFAULT,
    WT_EXECUTEINWAITTHREAD, WT_EXECUTEONLYONCE,
};

// -- Shared reclaim-stats counters -------------------------------------------

/// Atomic counters for a reclamation subsystem (timeouts, submissions,
/// completions, leaks). Each subsystem creates one `static` instance.
pub(crate) struct ReclaimCounters {
    timeouts: AtomicU64,
    submitted: AtomicU64,
    completed: AtomicU64,
    leaked: AtomicU64,
}

impl ReclaimCounters {
    pub(crate) const fn new() -> Self {
        Self {
            timeouts: AtomicU64::new(0),
            submitted: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            leaked: AtomicU64::new(0),
        }
    }

    pub(crate) fn snapshot(&self) -> ReclaimStats {
        ReclaimStats {
            timeouts: self.timeouts.load(Ordering::Relaxed),
            submitted: self.submitted.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            leaked: self.leaked.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn note_timeout(&self) -> ReclaimStats {
        self.timeouts.fetch_add(1, Ordering::Relaxed);
        self.snapshot()
    }

    pub(crate) fn note_submission(&self) -> ReclaimStats {
        self.submitted.fetch_add(1, Ordering::Relaxed);
        self.snapshot()
    }

    pub(crate) fn note_completion(&self) -> ReclaimStats {
        self.completed.fetch_add(1, Ordering::Relaxed);
        self.snapshot()
    }

    pub(crate) fn note_leak(&self) -> ReclaimStats {
        self.leaked.fetch_add(1, Ordering::Relaxed);
        self.snapshot()
    }
}

/// Point-in-time snapshot of [`ReclaimCounters`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ReclaimStats {
    pub(crate) timeouts: u64,
    pub(crate) submitted: u64,
    pub(crate) completed: u64,
    pub(crate) leaked: u64,
}

impl std::fmt::Display for ReclaimStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "timeouts={}, submitted={}, completed={}, leaked={}",
            self.timeouts, self.submitted, self.completed, self.leaked
        )
    }
}

#[cfg(test)]
pub(crate) fn wait_for_reclaim_stats_change(
    baseline: ReclaimStats,
    snapshot: impl Fn() -> ReclaimStats,
    predicate: impl Fn(ReclaimStats) -> bool,
    timeout: Duration,
    context: &'static str,
) -> ReclaimStats {
    let deadline = Instant::now() + timeout;
    loop {
        let stats = snapshot();
        if predicate(stats) {
            return stats;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for {context} reclaim stats to change from {:?}, current={:?}",
            baseline,
            stats,
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
pub(crate) fn assert_reclaim_stats_stable(
    baseline: ReclaimStats,
    snapshot: impl Fn() -> ReclaimStats,
    duration: Duration,
    context: &'static str,
) {
    let deadline = Instant::now() + duration;
    loop {
        let stats = snapshot();
        assert_eq!(stats, baseline, "{context} should not touch reclaim stats");

        if Instant::now() >= deadline {
            break;
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

// -- One-shot wait registration ----------------------------------------------

struct WaitOnceState {
    callback: Option<Box<dyn FnOnce(bool) + Send>>,
    wait_handle: Option<HANDLE>,
    callback_ran: bool,
    label: &'static str,
}

// SAFETY: The raw `HANDLE` in `wait_handle` is a kernel object handle safe
// to use from any thread. `Send` is required because the state lives inside
// `Arc<Mutex<...>>` accessed from a threadpool callback.
unsafe impl Send for WaitOnceState {}

// -- Deferred UnregisterWaitEx via QueueUserWorkItem -------------------------

struct UnregisterWaitCtx {
    wait_handle: HANDLE,
    label: &'static str,
}

fn unregister_wait(wait_handle: HANDLE, label: &'static str) {
    unsafe {
        if UnregisterWaitEx(wait_handle, INVALID_HANDLE_VALUE) == 0 {
            let err = io::Error::last_os_error();
            log::warn!("{label} failed to unregister wait handle: {err}");
        } else {
            log::debug!("{label} unregistered wait handle");
        }
    }
}

unsafe extern "system" fn unregister_wait_callback(ctx: *mut c_void) -> u32 {
    let ctx_box = unsafe { Box::from_raw(ctx as *mut UnregisterWaitCtx) };
    unregister_wait(ctx_box.wait_handle, ctx_box.label);
    0
}

fn spawn_wait_unregister(wait_handle: HANDLE, label: &'static str) {
    let ctx = Box::new(UnregisterWaitCtx { wait_handle, label });
    let ctx_ptr = Box::into_raw(ctx);
    let success = unsafe {
        QueueUserWorkItem(Some(unregister_wait_callback), ctx_ptr as *mut c_void, WT_EXECUTEDEFAULT)
    };
    if success == 0 {
        let err = io::Error::last_os_error();
        log::error!("Failed to queue work item to unregister wait handle for {label}: {err}");
        // Reclaim the context so it does not leak. The wait handle itself
        // becomes inert once the callback returns, so not unregistering it
        // is acceptable in this already-exceptional path.
        unsafe {
            let _ = Box::from_raw(ctx_ptr);
        }
    }
}

// -- Callback ----------------------------------------------------------------

extern "system" fn wait_once_callback(ctx: *mut c_void, timed_out: BOOLEAN) {
    let state = unsafe { Arc::from_raw(ctx as *mut Mutex<WaitOnceState>) };

    let (callback, label, wait_handle) = {
        let mut lock = state.lock().unwrap();
        lock.callback_ran = true;
        let wait_handle = lock.wait_handle.take();
        let label = lock.label;
        let Some(callback) = lock.callback.take() else {
            if let Some(wh) = wait_handle {
                spawn_wait_unregister(wh, label);
            }
            // WT_EXECUTEONLYONCE should prevent this, but guard against it
            // to avoid a panic inside a Win32 threadpool callback.
            log::error!("{label} wait callback invoked with no handler; skipping");
            return;
        };
        (callback, label, wait_handle)
    };

    // `UnregisterWaitEx` is intentionally NOT called inline here.
    //
    // `WT_EXECUTEINWAITTHREAD` runs this callback on the wait thread itself,
    // while `UnregisterWaitEx(wait, INVALID_HANDLE_VALUE)` blocks until the
    // callback returns. Calling it inline would therefore deadlock. Instead,
    // the callback hands the wait handle to a normal helper thread, which can
    // block until the callback returns and then retire the registration.
    //
    // In the race path (callback runs before the caller stores the handle),
    // `register_wait_once` performs the blocking unregister itself after
    // observing `callback_ran == true`.
    if let Some(wh) = wait_handle {
        spawn_wait_unregister(wh, label);
    }

    callback(timed_out != 0);
}

// -- Public entry point ------------------------------------------------------

/// Register a one-shot wait on `event_handle` via the Windows threadpool.
///
/// - If the event signals before `timeout_ms`, `callback(false)` is called.
/// - If the timeout expires first, `callback(true)` is called.
/// - If `RegisterWaitForSingleObject` itself fails, `callback(true)` is called as a fallback (both
///   existing callers leak resources in this case).
///
/// The callback runs with `WT_EXECUTEINWAITTHREAD` to minimize threadpool
/// overhead, and `UnregisterWaitEx` is deferred to a helper thread via
/// `QueueUserWorkItem` to avoid deadlocking the wait thread.
pub(crate) fn register_wait_once(
    event_handle: HANDLE,
    timeout_ms: u32,
    label: &'static str,
    callback: Box<dyn FnOnce(bool) + Send>,
) {
    let state = Arc::new(Mutex::new(WaitOnceState {
        callback: Some(callback),
        wait_handle: None,
        callback_ran: false,
        label,
    }));
    let ctx_ptr = Arc::into_raw(state.clone());

    let mut local_wait_handle: HANDLE = std::ptr::null_mut();

    let success = unsafe {
        RegisterWaitForSingleObject(
            &mut local_wait_handle,
            event_handle,
            Some(wait_once_callback),
            ctx_ptr as *mut _,
            timeout_ms,
            WT_EXECUTEINWAITTHREAD | WT_EXECUTEONLYONCE,
        )
    };

    // The callback may fire between `RegisterWaitForSingleObject` returning and
    // this lock acquisition. `callback_ran` disambiguates whether cleanup must
    // happen here or has already moved into the callback path.
    let mut lock = state.lock().unwrap();
    if success == 0 {
        let err = io::Error::last_os_error();
        log::error!("Failed to register wait for {label}: {err}");
        // Treat registration failure as a timeout so callers leak safely.
        let callback = lock.callback.take();
        drop(lock);
        if let Some(callback) = callback {
            callback(true);
        }

        // Manually drop the callback's Arc reference because the callback will never run.
        unsafe {
            let _ = Arc::from_raw(ctx_ptr);
        }
    } else if lock.callback_ran {
        // Callback already ran before the caller stored the wait handle. Since
        // this is a non-callback context, it is safe to block until teardown is
        // fully complete and retire the registration here.
        drop(lock);
        unregister_wait(local_wait_handle, label);
    } else {
        // Callback hasn't run yet. The wait handle becomes inert after the
        // callback returns, but it still must be unregistered to release the
        // underlying wait registration. The callback hands it off to a helper
        // thread for that final step.
        let _ = lock.wait_handle.insert(local_wait_handle);
    }
}
