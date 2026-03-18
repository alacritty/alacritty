use std::ffi::c_void;
use std::io::Error;
use std::num::NonZeroU32;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::process::ExitStatus;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};

use polling::os::iocp::{CompletionPacket, PollerIocpExt};
use polling::{Event, Poller};

use windows_sys::Win32::Foundation::{BOOLEAN, FALSE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessId, INFINITE, RegisterWaitForSingleObject, UnregisterWaitEx,
    WT_EXECUTEINWAITTHREAD, WT_EXECUTEONLYONCE,
};

use crate::tty::ChildEvent;

struct Interest {
    poller: Arc<Poller>,
    event: Event,
}

struct ChildExitState {
    event: Mutex<Option<ChildEvent>>,
    interest: Mutex<Option<Interest>>,
    child_handle: OwnedHandle,
    callback_entered: AtomicBool,
}

/// WinAPI callback to run when child process exits.
///
/// **Ownership invariant:** `Arc::from_raw` *must* execute before any early
/// return so the reconstructed `Arc` is dropped at function exit, balancing
/// the `Arc::into_raw` in `new_owned`. `callback_entered` is stored
/// immediately after reconstruction so the `Drop` impl on `ChildExitWatcher`
/// knows not to reconstruct and drop the raw callback `Arc` a second time.
/// Moving either of these two lines past the `timed_out` early-return would
/// break the ownership protocol.
extern "system" fn child_exit_callback(ctx: *mut c_void, timed_out: BOOLEAN) {
    let state = unsafe { Arc::from_raw(ctx.cast::<ChildExitState>()) };
    state.callback_entered.store(true, Ordering::Release);

    if timed_out != 0 {
        return;
    }

    let mut exit_code = 0_u32;
    let child_handle = state.child_handle.as_raw_handle() as HANDLE;
    let status = unsafe { GetExitCodeProcess(child_handle, &mut exit_code) };
    let exit_status = if status == FALSE { None } else { Some(ExitStatus::from_raw(exit_code)) };
    *state.event.lock().unwrap() = Some(ChildEvent::Exited(exit_status));

    let interest = state.interest.lock().unwrap();
    if let Some(interest) = interest.as_ref() {
        interest.poller.post(CompletionPacket::new(interest.event)).ok();
    }
}

pub struct ChildExitWatcher {
    wait_handle: AtomicPtr<c_void>,
    state: Arc<ChildExitState>,
    /// The raw `Arc` pointer passed to `RegisterWaitForSingleObject`.
    ///
    /// This represents one strong reference created by `Arc::into_raw`. The
    /// callback reconstructs and drops that `Arc` if it runs; otherwise `Drop`
    /// does so after `UnregisterWaitEx` proves the callback never started.
    callback_ctx: *const ChildExitState,
    pid: Option<NonZeroU32>,
    closed: bool,
}

// SAFETY: `ChildExitWatcher` contains raw pointers (`callback_ctx`, `wait_handle`)
// that prevent auto-`Send`. These pointers reference OS resources whose ownership
// is carefully managed between the callback and drop - they are safe to move
// between threads.
unsafe impl Send for ChildExitWatcher {}

impl ChildExitWatcher {
    pub fn new_owned(child_handle: OwnedHandle) -> Result<ChildExitWatcher, Error> {
        let mut wait_handle: HANDLE = ptr::null_mut();
        let raw_child_handle = child_handle.as_raw_handle() as HANDLE;
        let state = Arc::new(ChildExitState {
            event: Mutex::new(None),
            interest: Mutex::new(None),
            child_handle,
            callback_entered: AtomicBool::new(false),
        });
        let callback_ctx = Arc::into_raw(state.clone());

        let success = unsafe {
            RegisterWaitForSingleObject(
                &mut wait_handle,
                raw_child_handle,
                Some(child_exit_callback),
                callback_ctx as *mut c_void,
                INFINITE,
                WT_EXECUTEINWAITTHREAD | WT_EXECUTEONLYONCE,
            )
        };

        if success == 0 {
            let err = Error::last_os_error();
            unsafe {
                drop(Arc::from_raw(callback_ctx));
            }
            Err(err)
        } else {
            let pid = unsafe { NonZeroU32::new(GetProcessId(raw_child_handle)) };
            Ok(ChildExitWatcher {
                pid,
                state,
                callback_ctx,
                wait_handle: AtomicPtr::from(wait_handle),
                closed: false,
            })
        }
    }

    pub fn next_event(&mut self) -> Option<ChildEvent> {
        if self.closed {
            return None;
        }

        let event = self.state.event.lock().unwrap().take();
        self.closed = matches!(event, Some(ChildEvent::Exited(_)));
        event
    }

    pub fn register(&self, poller: &Arc<Poller>, event: Event) {
        *self.state.interest.lock().unwrap() = Some(Interest { poller: poller.clone(), event });

        if self.state.event.lock().unwrap().is_some() {
            poller.post(CompletionPacket::new(event)).ok();
        }
    }

    pub fn deregister(&self) {
        *self.state.interest.lock().unwrap() = None;
    }

    /// Retrieve the process handle of the underlying child process.
    ///
    /// This function does **not** pass ownership of the raw handle to you,
    /// and the handle is only guaranteed to be valid while the hosted application
    /// has not yet been destroyed.
    ///
    /// If you terminate the process using this handle, the child watcher will
    /// eventually emit an `Exited` event.
    pub fn raw_handle(&self) -> HANDLE {
        self.state.child_handle.as_raw_handle() as HANDLE
    }

    /// Retrieve the Process ID associated to the underlying child process.
    pub fn pid(&self) -> Option<NonZeroU32> {
        self.pid
    }
}

impl Drop for ChildExitWatcher {
    fn drop(&mut self) {
        unsafe {
            let wait_handle = self.wait_handle.swap(ptr::null_mut(), Ordering::AcqRel) as HANDLE;
            let mut unregistered = wait_handle.is_null();
            if !unregistered {
                unregistered = UnregisterWaitEx(wait_handle, INVALID_HANDLE_VALUE) != 0;
            }

            if unregistered {
                // SAFETY: `UnregisterWaitEx(_, INVALID_HANDLE_VALUE)` blocks
                // until any in-flight callback has finished, so exactly one of
                // two owners remains for the callback `Arc`:
                //
                // 1. The callback already ran and reconstructed the `Arc` via `Arc::from_raw`,
                //    taking ownership. `callback_entered` is `true` - we must NOT reconstruct and
                //    drop it again.
                //
                // 2. The callback never started. `callback_entered` is `false`
                //    - the raw callback `Arc` is still outstanding and must be
                //    reconstructed and dropped here.
                if !self.state.callback_entered.load(Ordering::Acquire) {
                    drop(Arc::from_raw(self.callback_ctx));
                }
            } else {
                // Intentionally leave the raw callback `Arc` unreclaimed here.
                // If the callback has not finished yet, that outstanding strong
                // reference keeps `ChildExitState` and its process handle alive
                // until the callback can release them safely.
                let err = Error::last_os_error();
                log::warn!(
                    "UnregisterWaitEx failed during child watcher drop ({err}), leaving \
                     callback-owned state unreclaimed to prevent use-after-free"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::process::Command;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    use super::super::PTY_CHILD_EVENT_TOKEN;
    use super::*;

    fn duplicate_process_handle(handle: HANDLE) -> OwnedHandle {
        let current_process = unsafe { GetCurrentProcess() };
        let mut duplicated = ptr::null_mut();
        let success = unsafe {
            DuplicateHandle(
                current_process,
                handle,
                current_process,
                &mut duplicated,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        assert_ne!(success, 0, "DuplicateHandle failed: {}", Error::last_os_error());

        unsafe { OwnedHandle::from_raw_handle(duplicated as _) }
    }

    #[test]
    pub fn event_is_emitted_when_child_exits() {
        const WAIT_TIMEOUT: Duration = Duration::from_millis(200);

        let poller = Arc::new(Poller::new().unwrap());

        let mut child = Command::new("cmd.exe").spawn().unwrap();
        let child_exit_watcher =
            ChildExitWatcher::new_owned(duplicate_process_handle(child.as_raw_handle() as HANDLE))
                .unwrap();
        child_exit_watcher.register(&poller, Event::readable(PTY_CHILD_EVENT_TOKEN));

        child.kill().unwrap();

        // Poll for the event or fail with timeout if nothing has been sent.
        let mut events = polling::Events::new();
        poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
        assert_eq!(events.iter().next().unwrap().key, PTY_CHILD_EVENT_TOKEN);
        // Verify that at least one `ChildEvent::Exited` was received.
        let expected_status = child.wait().unwrap();
        let mut child_exit_watcher = child_exit_watcher;
        assert_eq!(
            child_exit_watcher.next_event(),
            Some(ChildEvent::Exited(Some(expected_status)))
        );
    }

    #[test]
    fn delivered_exit_event_is_not_repeated_after_consumption() {
        let state = Arc::new(ChildExitState {
            event: Mutex::new(Some(ChildEvent::Exited(None))),
            interest: Mutex::new(None),
            child_handle: duplicate_process_handle(unsafe { GetCurrentProcess() }),
            callback_entered: AtomicBool::new(false),
        });
        let mut child_exit_watcher = ChildExitWatcher {
            wait_handle: AtomicPtr::new(ptr::null_mut()),
            callback_ctx: Arc::into_raw(state.clone()),
            state,
            pid: None,
            closed: false,
        };

        assert_eq!(child_exit_watcher.next_event(), Some(ChildEvent::Exited(None)));
        assert_eq!(child_exit_watcher.next_event(), None);
    }

    #[test]
    fn consumed_exit_event_is_not_replayed_on_late_register() {
        const WAIT_TIMEOUT: Duration = Duration::from_millis(50);

        let state = Arc::new(ChildExitState {
            event: Mutex::new(Some(ChildEvent::Exited(None))),
            interest: Mutex::new(None),
            child_handle: duplicate_process_handle(unsafe { GetCurrentProcess() }),
            callback_entered: AtomicBool::new(false),
        });
        let mut child_exit_watcher = ChildExitWatcher {
            wait_handle: AtomicPtr::new(ptr::null_mut()),
            callback_ctx: Arc::into_raw(state.clone()),
            state,
            pid: None,
            closed: false,
        };

        assert_eq!(child_exit_watcher.next_event(), Some(ChildEvent::Exited(None)));

        let poller = Arc::new(Poller::new().unwrap());
        child_exit_watcher.register(&poller, Event::readable(PTY_CHILD_EVENT_TOKEN));

        let mut events = polling::Events::new();
        poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
        assert!(events.is_empty(), "consumed exit event should not be replayed");
    }

    #[test]
    fn late_register_replays_pending_exit_into_poller() {
        const WAIT_TIMEOUT: Duration = Duration::from_millis(500);
        const STATE_TIMEOUT: Duration = Duration::from_secs(5);

        let poller = Arc::new(Poller::new().unwrap());
        let mut child = Command::new("cmd.exe").args(["/Q", "/D", "/C", "exit 0"]).spawn().unwrap();
        let mut child_exit_watcher =
            ChildExitWatcher::new_owned(duplicate_process_handle(child.as_raw_handle() as HANDLE))
                .unwrap();
        let expected_status = child.wait().unwrap();

        let deadline = Instant::now() + STATE_TIMEOUT;
        while Instant::now() < deadline {
            if child_exit_watcher.state.event.lock().unwrap().is_some() {
                break;
            }

            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(
            child_exit_watcher.state.event.lock().unwrap().is_some(),
            "timed out waiting for pending child exit before register"
        );

        child_exit_watcher.register(&poller, Event::readable(PTY_CHILD_EVENT_TOKEN));

        let mut events = polling::Events::new();
        poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
        assert_eq!(events.iter().next().unwrap().key, PTY_CHILD_EVENT_TOKEN);

        assert_eq!(
            child_exit_watcher.next_event(),
            Some(ChildEvent::Exited(Some(expected_status)))
        );
    }
}
