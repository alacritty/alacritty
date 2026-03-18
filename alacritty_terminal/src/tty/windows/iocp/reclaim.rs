use std::mem::MaybeUninit;
use std::os::windows::io::{AsRawHandle, OwnedHandle};

use log::debug;
use windows_sys::Win32::Foundation::HANDLE;

use super::super::wait_reclaim::{ReclaimCounters, ReclaimStats};
use super::Overlapped;

/// Maximum time (in milliseconds) for the background reclamation thread to
/// wait before permanently leaking overlapped resources as a safe fallback.
const DROP_RECLAIM_TIMEOUT_MS: u32 = 30_000;

static COUNTERS: ReclaimCounters = ReclaimCounters::new();

#[cfg(test)]
pub(super) fn drop_reclaim_stats() -> ReclaimStats {
    COUNTERS.snapshot()
}

pub(super) fn note_cancel_timeout() -> ReclaimStats {
    COUNTERS.note_timeout()
}

/// Resources that must share a lifetime if overlapped I/O is still in flight.
///
/// The kernel may still reference the `OVERLAPPED`, backing buffer, and wait
/// event after the owning reader/writer has begun dropping. They therefore move
/// as a single bundle into the async reclamation path.
pub(super) struct LeakedResources {
    pub(super) label: &'static str,
    pub(super) overlapped: Box<Overlapped>,
    pub(super) buf: Vec<MaybeUninit<u8>>,
    pub(super) wait_event: Option<OwnedHandle>,
}

fn leak_resources(resources: LeakedResources) {
    let stats = COUNTERS.note_leak();
    log::warn!(
        "{} drop reclaim permanently leaked overlapped resources (buffer_len={}, \
         has_wait_event={}, {})",
        resources.label,
        resources.buf.len(),
        resources.wait_event.is_some(),
        stats,
    );
    std::mem::forget(resources.wait_event);
    std::mem::forget(resources.overlapped);
    std::mem::forget(resources.buf);
}

/// Submit leaked overlapped resources for asynchronous reclamation.
pub(super) fn submit_for_reclamation(resources: LeakedResources) {
    submit_for_reclamation_with_timeout(resources, DROP_RECLAIM_TIMEOUT_MS);
}

pub(super) fn submit_for_reclamation_with_timeout(resources: LeakedResources, timeout_ms: u32) {
    let label = resources.label;
    let stats = COUNTERS.note_submission();
    debug!(
        "{} drop reclaim submitted (buffer_len={}, has_wait_event={}, {})",
        label,
        resources.buf.len(),
        resources.wait_event.is_some(),
        stats,
    );

    if resources.wait_event.is_none() {
        log::warn!(
            "{} drop reclaim missing wait event (buffer_len={}, {}); leaking overlapped resources",
            label,
            resources.buf.len(),
            stats,
        );
        leak_resources(resources);
        return;
    }

    let wait_event_handle = resources.wait_event.as_ref().unwrap().as_raw_handle() as HANDLE;
    let buf_len = resources.buf.len();

    super::super::wait_reclaim::register_wait_once(
        wait_event_handle,
        timeout_ms,
        label,
        Box::new(move |timed_out| {
            if timed_out {
                leak_resources(resources);
            } else {
                let stats = COUNTERS.note_completion();
                debug!(
                    "{} drop reclaim completed successfully (buffer_len={}, {})",
                    label, buf_len, stats,
                );
                drop(resources);
            }
        }),
    );
}
