//! Windows PTY I/O backed by overlapped I/O and poller waitables.

use std::cmp;
use std::io::{self, ErrorKind, Read, Write};
use std::mem::MaybeUninit;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::sync::Arc;

use log::debug;
use polling::os::iocp::{CompletionPacket, PollerIocpExt};
use polling::{Event, PollMode, Poller};

use windows_sys::Win32::Foundation::{
    ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF, ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_MORE_DATA,
    ERROR_NO_DATA, ERROR_NOT_FOUND, ERROR_OPERATION_ABORTED, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows_sys::Win32::System::IO::{
    CancelIoEx, GetOverlappedResult, OVERLAPPED as WinOverlapped,
};
use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject};

mod reclaim;

use self::reclaim::{LeakedResources, note_cancel_timeout, submit_for_reclamation};

// Windows PTY I/O is organized in three layers:
//
// - raw overlapped helpers (`Overlapped`, `OverlappedPipeExt`)
// - steady-state reader/writer state (`IocpPipeState`, `IocpReader`, `IocpWriter`)
// - async drop fallback (`reclaim`) for the rare case where cancellation does not complete before
//   teardown must proceed
//
// Keeping the reclamation path in a dedicated submodule makes it explicit that
// the leak-on-timeout machinery is not part of the normal data path.

/// Thin wrapper around a Win32 `OVERLAPPED` structure.
///
/// **Ownership model:** `Overlapped` does *not* close the `hEvent` handle on
/// drop - the handle's lifetime is managed externally by `IocpPipeState`,
/// which wraps it in an `OwnedHandle`. This split exists so that the event
/// handle can be independently registered with the poller and, in the leak
/// path, transferred to a background reclamation thread together with the
/// buffer and overlapped struct as a unit.
///
/// `Overlapped::zero()` creates a sentinel with a null `hEvent`; it is only
/// used as a replacement value when the real overlapped struct is moved into
/// `LeakedResources`.
struct Overlapped {
    inner: WinOverlapped,
}

unsafe impl Send for Overlapped {}

impl Overlapped {
    fn initialize_with_manual_reset_event() -> io::Result<(Self, OwnedHandle)> {
        let event = unsafe {
            CreateEventW(
                std::ptr::null(),
                1, // manual reset = TRUE
                0, // initial state = FALSE
                std::ptr::null(),
            )
        };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let event_handle = unsafe { OwnedHandle::from_raw_handle(event as RawHandle) };
        let mut inner: WinOverlapped = unsafe { std::mem::zeroed() };
        inner.hEvent = event_handle.as_raw_handle() as _;
        Ok((Self { inner }, event_handle))
    }

    fn event(&self) -> RawHandle {
        self.inner.hEvent as _
    }

    fn raw(&mut self) -> *mut WinOverlapped {
        &mut self.inner as *mut _
    }

    fn zero() -> Self {
        Self { inner: unsafe { std::mem::zeroed() } }
    }
}

trait OverlappedPipeExt {
    unsafe fn read_overlapped(
        &self,
        buf: &mut [MaybeUninit<u8>],
        overlapped: *mut WinOverlapped,
    ) -> io::Result<Option<usize>>;
    unsafe fn write_overlapped(
        &self,
        buf: &[u8],
        overlapped: *mut WinOverlapped,
    ) -> io::Result<Option<usize>>;
    unsafe fn result(&self, overlapped: *mut WinOverlapped) -> io::Result<usize>;
}

impl OverlappedPipeExt for OwnedHandle {
    unsafe fn read_overlapped(
        &self,
        buf: &mut [MaybeUninit<u8>],
        overlapped: *mut WinOverlapped,
    ) -> io::Result<Option<usize>> {
        let len = cmp::min(buf.len(), u32::MAX as usize) as u32;
        let mut transferred = 0;
        let success = unsafe {
            ReadFile(
                self.as_raw_handle() as _,
                buf.as_mut_ptr() as _,
                len,
                &mut transferred,
                overlapped,
            )
        };

        if success != 0 {
            // ReadFile completed synchronously; `transferred` is already valid.
            Ok(Some(transferred as usize))
        } else {
            let err = io::Error::last_os_error();
            match err.raw_os_error() {
                Some(code) if code == ERROR_IO_PENDING as i32 => Ok(None),
                // ERROR_MORE_DATA: byte-mode pipes should not produce this, but
                // handle it defensively - the buffer is filled and more data is
                // available; treat as a successful partial read.
                Some(code) if code == ERROR_MORE_DATA as i32 => Ok(Some(len as usize)),
                _ => Err(err),
            }
        }
    }

    unsafe fn write_overlapped(
        &self,
        buf: &[u8],
        overlapped: *mut WinOverlapped,
    ) -> io::Result<Option<usize>> {
        let len = cmp::min(buf.len(), u32::MAX as usize) as u32;
        let mut transferred = 0;
        let success = unsafe {
            WriteFile(
                self.as_raw_handle() as _,
                buf.as_ptr() as _,
                len,
                &mut transferred,
                overlapped,
            )
        };

        if success != 0 {
            // WriteFile completed synchronously; `transferred` is already valid.
            Ok(Some(transferred as usize))
        } else {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_IO_PENDING as i32) { Ok(None) } else { Err(err) }
        }
    }

    unsafe fn result(&self, overlapped: *mut WinOverlapped) -> io::Result<usize> {
        let mut transferred = 0;
        let success = unsafe {
            GetOverlappedResult(self.as_raw_handle() as _, overlapped, &mut transferred, 0)
        };
        if success != 0 {
            Ok(transferred as usize)
        } else {
            let err = io::Error::last_os_error();
            // ERROR_MORE_DATA: the buffer was filled and more data remains.
            // `transferred` is valid; treat as a successful partial read.
            if err.raw_os_error() == Some(ERROR_MORE_DATA as i32) {
                Ok(transferred as usize)
            } else {
                Err(err)
            }
        }
    }
}

/// Maximum time (in milliseconds) to wait for a cancelled overlapped I/O
/// operation to complete during drop. Cancellation normally completes almost
/// instantly; this timeout exists only to prevent a hung ConPTY child from
/// blocking process teardown indefinitely.
const DROP_CANCEL_TIMEOUT_MS: u32 = 100;

/// Result of attempting to drain a pending overlapped I/O operation during drop.
enum DrainResult {
    /// The operation completed (or was successfully cancelled); resources can be freed normally.
    Completed,
    /// The operation may still be in-flight; resources must be leaked or reclaimed asynchronously.
    StillInFlight,
}

/// Returns `true` for Win32 error codes that indicate the pipe has reached
/// end-of-stream (broken pipe, explicit EOF, or the write end closing).
///
/// `ERROR_NO_DATA` specifically means "the pipe is being closed" from the
/// write direction, but we include it here so both reader and writer
/// teardown paths share a single predicate for pipe-gone conditions.
fn is_eof_error(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(code)
            if code == ERROR_BROKEN_PIPE as i32
                || code == ERROR_HANDLE_EOF as i32
                || code == ERROR_NO_DATA as i32
    )
}

/// Returns `true` for Win32 error codes that are expected during pipe
/// teardown and should not be logged as unexpected failures.
fn is_expected_pipe_error(err: &io::Error) -> bool {
    is_eof_error(err) || err.raw_os_error() == Some(ERROR_OPERATION_ABORTED as i32)
}

#[inline]
fn wait_for_signal(handle: HANDLE, timeout_ms: u32) -> u32 {
    unsafe { WaitForSingleObject(handle, timeout_ms) }
}

#[inline]
fn clear_overlapped_signal(overlapped: &Overlapped, label: &'static str) {
    let event = overlapped.event();
    if event.is_null() {
        return;
    }

    unsafe {
        if ResetEvent(event as HANDLE) == 0 {
            let err = io::Error::last_os_error();
            debug!("ResetEvent failed for {label}: {err}");
        }
    }
}

fn drain_pending_overlapped_io(pipe: &OwnedHandle, overlapped: &mut Overlapped) -> DrainResult {
    if overlapped.event().is_null() {
        // Sentinel overlapped (e.g. after LeakedResources swap) - nothing to drain.
        return DrainResult::Completed;
    }

    // SAFETY: We have exclusive access to the pipe and overlapped structure during Drop.
    // The handle is guaranteed valid by the OwnedHandle ownership.
    unsafe {
        let handle = pipe.as_raw_handle() as HANDLE;
        let raw_overlapped = overlapped.raw();
        let mut transferred = 0;

        // Best-effort completion check before cancellation.
        if GetOverlappedResult(handle, raw_overlapped, &mut transferred, 0) != 0 {
            return DrainResult::Completed;
        }

        let initial_err = io::Error::last_os_error();
        if initial_err.raw_os_error() != Some(ERROR_IO_INCOMPLETE as i32) {
            if !is_expected_pipe_error(&initial_err) {
                debug!(
                    "GetOverlappedResult pre-check failed while dropping overlapped PTY I/O: \
                     {initial_err}"
                );
            }
            return DrainResult::Completed;
        }

        // Make sure no in-flight operation still references `OVERLAPPED` or I/O buffers.
        if CancelIoEx(handle, raw_overlapped) == 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
                return DrainResult::Completed;
            }

            if is_expected_pipe_error(&err) {
                return DrainResult::Completed;
            }

            debug!("CancelIoEx failed while dropping pending overlapped PTY I/O: {err}");
            // The operation may still be in-flight; take the safe leak path.
            return DrainResult::StillInFlight;
        }

        // Wait with a timeout instead of blocking indefinitely so a hung
        // ConPTY child cannot stall process teardown.
        let wait_result = wait_for_signal(overlapped.event() as HANDLE, DROP_CANCEL_TIMEOUT_MS);
        match wait_result {
            WAIT_OBJECT_0 => {
                // Cancellation completed; drain the result.
                if GetOverlappedResult(handle, raw_overlapped, &mut transferred, 0) == 0 {
                    let err = io::Error::last_os_error();
                    if !is_expected_pipe_error(&err) {
                        debug!(
                            "GetOverlappedResult failed while draining dropped overlapped PTY \
                             I/O: {err}"
                        );
                    }
                }
                DrainResult::Completed
            },
            WAIT_TIMEOUT => {
                let stats = note_cancel_timeout();
                log::warn!(
                    "timed out waiting {DROP_CANCEL_TIMEOUT_MS}ms for cancelled overlapped PTY \
                     I/O to complete ({stats})",
                );
                DrainResult::StillInFlight
            },
            other => {
                let err = io::Error::last_os_error();
                log::warn!(
                    "WaitForSingleObject returned unexpected value {other} during PTY I/O drop \
                     (last error: {err}); assuming operation may still be in-flight"
                );
                DrainResult::StillInFlight
            },
        }
    }
}

struct Interest {
    event: Event,
    poller: Arc<Poller>,
}

#[inline]
fn initialized_range(buf: &[MaybeUninit<u8>], start: usize, end: usize) -> &[u8] {
    debug_assert!(start <= end);
    debug_assert!(end <= buf.len());

    // SAFETY: Callers guarantee that the `[start, end)` byte range has been initialized.
    unsafe { std::slice::from_raw_parts(buf.as_ptr().add(start).cast::<u8>(), end - start) }
}

#[inline]
fn copy_into_maybe_uninit_slice(dst: &mut [MaybeUninit<u8>], src: &[u8]) {
    debug_assert!(src.len() <= dst.len());

    // SAFETY: `src` and `dst` do not overlap, and writing bytes into `MaybeUninit<u8>`
    // initializes exactly the copied prefix.
    unsafe {
        std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr().cast::<u8>(), src.len());
    }
}

/// Shared state for IOCP-backed pipe I/O (reader or writer).
///
/// Owns the pipe handle, overlapped structure, waitable event, I/O buffer,
/// and pending flag. Provides waitable registration/deregistration and
/// the drop leak-on-timeout safety pattern.
struct IocpPipeState {
    label: &'static str,
    pipe: OwnedHandle,
    overlapped: Box<Overlapped>,
    wait_event: Option<OwnedHandle>,
    interest: Option<Interest>,
    waitable_registered: bool,
    buf: Vec<MaybeUninit<u8>>,
    pending: bool,
}

impl IocpPipeState {
    fn new(pipe: OwnedHandle, capacity: usize, label: &'static str) -> io::Result<Self> {
        debug_assert!(capacity > 0, "IocpPipeState requires a non-zero buffer capacity");

        let (overlapped_struct, wait_event) = Overlapped::initialize_with_manual_reset_event()?;
        // Keep the OVERLAPPED backing storage on the heap so its address remains
        // stable for the duration of async I/O.
        let overlapped = Box::new(overlapped_struct);

        let mut buf = Vec::with_capacity(capacity);
        // SAFETY: The elements are MaybeUninit, so we don't need to initialize them.
        unsafe { buf.set_len(capacity) };

        Ok(Self {
            label,
            pipe,
            overlapped,
            wait_event: Some(wait_event),
            interest: None,
            waitable_registered: false,
            buf,
            pending: false,
        })
    }

    /// Register or modify the waitable event with the poller.
    ///
    /// Caller is responsible for post-registration actions (arming a read,
    /// posting a writable event, etc.).
    fn register_waitable(
        &mut self,
        poller: &Arc<Poller>,
        event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        if !matches!(mode, PollMode::Level) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!("{} supports only PollMode::Level", self.label),
            ));
        }

        let wait_event = self.wait_event.as_ref().ok_or_else(|| {
            io::Error::other(format!(
                "{} wait event handle unavailable during register",
                self.label
            ))
        })?;

        if self.waitable_registered {
            poller.modify_waitable(wait_event, event, mode)?;
        } else {
            // SAFETY: `wait_event` is a valid handle to a manual-reset event associated with
            // the overlapped I/O. The poller needs to wait on this event to detect completion.
            unsafe { poller.add_waitable(wait_event, event, mode)? };
            self.waitable_registered = true;
        }

        self.interest = Some(Interest { event, poller: poller.clone() });
        Ok(())
    }

    fn deregister(&mut self) {
        if self.waitable_registered {
            if let (Some(interest), Some(wait_event)) =
                (self.interest.as_ref(), self.wait_event.as_ref())
            {
                let _ = interest.poller.remove_waitable(wait_event);
            }
            self.waitable_registered = false;
        }

        self.interest = None;
    }

    /// If a pending I/O operation did not complete within the cancellation
    /// timeout, leak the overlapped struct, buffer, and event handle as a unit
    /// to prevent use-after-free in the kernel.
    fn drop_pending(&mut self) {
        if !self.pending {
            return;
        }

        if matches!(
            drain_pending_overlapped_io(&self.pipe, &mut self.overlapped),
            DrainResult::StillInFlight
        ) {
            // `OVERLAPPED`, the backing buffer, and the event handle form one
            // lifetime domain from the kernel's perspective. If completion is
            // still in flight after cancellation, we must keep all three alive
            // together until the reclamation thread can prove the operation has
            // finished.
            //
            // The pipe `OwnedHandle` is intentionally *not* leaked here.
            // Closing the pipe handle causes the kernel to cancel any pending
            // I/O and mark it as completed with `ERROR_OPERATION_ABORTED`.
            // The kernel writes the completion status into the `OVERLAPPED`
            // struct (which *is* leaked), but does not access the pipe handle
            // after the close returns. This is safe because `CancelIoEx` was
            // already called above - the close merely accelerates the
            // cancellation that is already in progress.
            //
            // Win32 references:
            // https://learn.microsoft.com/windows/win32/api/ioapiset/nf-ioapiset-cancelioex
            // https://learn.microsoft.com/windows/win32/fileio/canceling-pending-i-o-operations
            let overlapped = std::mem::replace(&mut self.overlapped, Box::new(Overlapped::zero()));
            let buf = std::mem::take(&mut self.buf);
            let wait_event = self.wait_event.take();

            submit_for_reclamation(LeakedResources {
                label: self.label,
                overlapped,
                buf,
                wait_event,
            });
        }
        self.pending = false;
    }
}

impl Drop for IocpPipeState {
    fn drop(&mut self) {
        self.deregister();
        self.drop_pending();
    }
}

/// Non-blocking reader using an overlapped read operation and a waitable event.
pub struct IocpReader {
    state: IocpPipeState,
    /// Number of bytes valid in the buffer from the last completed read.
    read_len: usize,
    /// Current cursor position as we drain into user buffers.
    read_pos: usize,
    /// Deferred fatal read error discovered while re-arming in the background.
    ///
    /// This is surfaced on the next `read()` call and accompanied by a
    /// synthetic readable wakeup so the event loop cannot lose the failure if
    /// it exits its current read cycle before calling `read()` again.
    read_error: Option<io::Error>,
    eof: bool,
    /// Tracks whether the EOF condition has been communicated to the poller.
    ///
    /// When `eof` becomes true, `eof_notified` is set to false so that
    /// `post_readable_if_needed` fires one synthetic readable event. The flag
    /// is then set to true inside `consume_staged` once the caller has drained
    /// remaining staged data at EOF, preventing repeated event posting.
    eof_notified: bool,
}

impl IocpReader {
    pub fn new(pipe: OwnedHandle, read_capacity: usize) -> io::Result<Self> {
        Ok(Self {
            state: IocpPipeState::new(pipe, read_capacity, "IocpReader")?,
            read_len: 0,
            read_pos: 0,
            read_error: None,
            eof: false,
            eof_notified: false,
        })
    }

    pub fn register(
        &mut self,
        poller: &Arc<Poller>,
        event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        self.state.register_waitable(poller, event, mode)?;
        self.arm_read()?;
        self.post_readable_if_needed();

        Ok(())
    }

    pub fn deregister(&mut self) {
        self.state.deregister();
    }

    fn has_staged_data(&self) -> bool {
        self.read_pos < self.read_len
    }

    fn has_deferred_error(&self) -> bool {
        self.read_error.is_some()
    }

    fn post_readable_if_needed(&self) {
        let Some(interest) = self.state.interest.as_ref() else {
            return;
        };

        if !interest.event.readable {
            return;
        }

        if self.has_staged_data() || self.has_deferred_error() || (self.eof && !self.eof_notified) {
            if let Err(err) = interest.poller.post(CompletionPacket::new(interest.event)) {
                log::error!(
                    "Failed to post readable event to poller for {}: {err}",
                    self.state.label
                );
            }
        }
    }

    fn defer_read_error(&mut self, err: io::Error) {
        self.read_error = Some(err);
    }

    fn mark_eof(&mut self) {
        self.eof = true;
        self.eof_notified = false;
        self.state.pending = false;
        self.read_len = 0;
        self.read_pos = 0;
    }

    fn arm_read(&mut self) -> io::Result<()> {
        if self.state.pending || self.has_staged_data() || self.eof {
            return Ok(());
        }

        debug_assert!(
            !self.state.overlapped.event().is_null(),
            "arm_read called on sentinel overlapped (event handle is null)"
        );
        clear_overlapped_signal(&self.state.overlapped, self.state.label);

        // SAFETY: `buf` is a `Vec` whose capacity is fixed at construction, so its heap
        // allocation will not move. `overlapped` is heap-allocated inside a `Box` in
        // `state`. Neither is moved until the operation completes or is cancelled in `drop`.
        let result = unsafe {
            self.state.pipe.read_overlapped(&mut self.state.buf, self.state.overlapped.raw())
        };

        match result {
            Ok(Some(0)) => {
                clear_overlapped_signal(&self.state.overlapped, self.state.label);
                self.mark_eof();
            },
            Ok(Some(n)) => {
                self.read_len = n;
                self.read_pos = 0;
            },
            Ok(None) => self.state.pending = true,
            Err(err) => {
                clear_overlapped_signal(&self.state.overlapped, self.state.label);
                if is_eof_error(&err) {
                    self.mark_eof();
                } else {
                    return Err(err);
                }
            },
        }

        Ok(())
    }

    fn complete_pending_read(&mut self) -> io::Result<()> {
        if !self.state.pending {
            return Ok(());
        }

        // SAFETY: We are checking the status of the overlapped operation initiated in `arm_read`.
        let result = unsafe { self.state.pipe.result(self.state.overlapped.raw()) };

        if let Err(err) = &result {
            if err.raw_os_error() == Some(ERROR_IO_INCOMPLETE as i32) {
                return Err(io::Error::from(ErrorKind::WouldBlock));
            }
        }

        clear_overlapped_signal(&self.state.overlapped, self.state.label);

        match result {
            Ok(0) => {
                self.mark_eof();
            },
            Ok(n) => {
                self.state.pending = false;
                self.read_len = n;
                self.read_pos = 0;
            },
            Err(err) => {
                self.state.pending = false;
                if is_eof_error(&err) {
                    self.mark_eof();
                } else {
                    return Err(err);
                }
            },
        }

        Ok(())
    }

    fn consume_staged(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut total_read = 0;
        let mut buf_slice = buf;

        while !buf_slice.is_empty() && self.has_staged_data() {
            let available = self.read_len - self.read_pos;
            let n = cmp::min(available, buf_slice.len());
            let staged = initialized_range(&self.state.buf, self.read_pos, self.read_pos + n);
            buf_slice[..n].copy_from_slice(staged);
            self.read_pos += n;
            total_read += n;

            let (_, rest) = buf_slice.split_at_mut(n);
            buf_slice = rest;

            if self.read_pos == self.read_len {
                self.read_pos = 0;
                self.read_len = 0;

                // When the staged buffer is fully drained, immediately arm the next
                // overlapped read so the waitable event fires as soon as data arrives,
                // avoiding a latency stall until the next caller-driven `read()`.
                //
                // If `arm_read` completes synchronously (the pipe already has data),
                // `read_len`/`read_pos` are updated in place and the outer `while`
                // loop continues copying from the *new* read directly into the
                // caller's buffer - no round-trip through the poller required.
                if !self.eof {
                    if let Err(err) = self.arm_read() {
                        log::trace!("Background arm_read failed after partial consume: {err}");
                        self.defer_read_error(err);
                        break;
                    }
                }
            }
        }

        // Keep level behavior if we still have staged bytes, a deferred error,
        // or immediate EOF.
        if self.has_staged_data() {
            // Clearing the event here is safe even if arm_read started a new
            // pending overlapped read: post_readable_if_needed posts a
            // synthetic IOCP completion packet (independent of the event
            // handle), so the caller will wake up to drain remaining staged
            // data. The pending read's eventual completion will re-signal
            // the event and trigger a separate waitable wakeup.
            clear_overlapped_signal(&self.state.overlapped, self.state.label);
            self.post_readable_if_needed();
        } else if self.has_deferred_error() {
            self.post_readable_if_needed();
        } else if self.eof && !self.eof_notified {
            self.post_readable_if_needed();
            self.eof_notified = true;
        }

        Ok(total_read)
    }
}

impl Read for IocpReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        if let Some(err) = self.read_error.take() {
            return Err(err);
        }

        if self.state.pending {
            self.complete_pending_read()?;
        } else if !self.has_staged_data() && !self.eof {
            self.arm_read()?;
        }

        if self.has_staged_data() {
            self.consume_staged(buf)
        } else if self.eof {
            Ok(0)
        } else {
            Err(io::Error::from(ErrorKind::WouldBlock))
        }
    }
}

/// Non-blocking writer using overlapped writes and a waitable event.
pub struct IocpWriter {
    state: IocpPipeState,
    write_len: usize,
    write_pos: usize,
    broken_pipe: bool,
}

impl IocpWriter {
    pub fn new(pipe: OwnedHandle, write_capacity: usize) -> io::Result<Self> {
        Ok(Self {
            state: IocpPipeState::new(pipe, write_capacity, "IocpWriter")?,
            write_len: 0,
            write_pos: 0,
            broken_pipe: false,
        })
    }

    pub fn register(
        &mut self,
        poller: &Arc<Poller>,
        event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        self.state.register_waitable(poller, event, mode)?;
        self.post_writable_if_needed();

        Ok(())
    }

    pub fn deregister(&mut self) {
        self.state.deregister();
    }

    pub fn has_pending_io(&self) -> bool {
        self.state.pending || self.write_pos < self.write_len
    }

    pub fn advance(&mut self) -> io::Result<()> {
        // Distinguish between caller-visible queued input and backend-owned
        // progress. The event loop may have no additional bytes to submit while
        // the writer still needs writable wakeups to retire an overlapped write
        // or to drain the remainder of a partially accepted buffer.
        if self.broken_pipe {
            return Err(io::Error::from(ErrorKind::BrokenPipe));
        }

        if self.state.pending {
            self.complete_pending_write()
        } else if self.write_pos < self.write_len {
            self.drain_immediate_writes(false)
        } else {
            Ok(())
        }
    }

    fn mark_broken_pipe(&mut self) {
        self.state.pending = false;
        self.broken_pipe = true;
        self.write_len = 0;
        self.write_pos = 0;
    }

    fn writable_now(&self) -> bool {
        !self.broken_pipe && !self.state.pending && self.write_pos >= self.write_len
    }

    fn post_writable_if_needed(&self) {
        let Some(interest) = self.state.interest.as_ref() else {
            return;
        };

        if interest.event.writable && self.writable_now() {
            if let Err(err) = interest.poller.post(CompletionPacket::new(interest.event)) {
                log::error!(
                    "Failed to post writable event to poller for {}: {err}",
                    self.state.label
                );
            }
        }
    }

    fn drain_immediate_writes(&mut self, post_on_completion: bool) -> io::Result<()> {
        debug_assert!(
            !self.state.overlapped.event().is_null(),
            "drain_immediate_writes called on sentinel overlapped (event handle is null)"
        );

        while !self.state.pending && self.write_pos < self.write_len {
            clear_overlapped_signal(&self.state.overlapped, self.state.label);
            let slice = initialized_range(&self.state.buf, self.write_pos, self.write_len);
            // SAFETY: `buf` is a `Vec` whose capacity is fixed at construction, so its
            // heap allocation will not move. We do not modify this slice until completion.
            let result =
                unsafe { self.state.pipe.write_overlapped(slice, self.state.overlapped.raw()) };

            match result {
                Ok(Some(0)) => {
                    clear_overlapped_signal(&self.state.overlapped, self.state.label);
                    self.mark_broken_pipe();
                    return Err(io::Error::from(ErrorKind::BrokenPipe));
                },
                Ok(Some(n)) => {
                    self.write_pos += n;

                    if self.write_pos < self.write_len {
                        clear_overlapped_signal(&self.state.overlapped, self.state.label);
                        // Yield to the poller loop on partial synchronous writes
                        // so the event loop can service reads and channel events
                        // between write chunks.
                        if let Some(interest) = self.state.interest.as_ref() {
                            if interest.event.writable {
                                let _ = interest.poller.post(CompletionPacket::new(interest.event));
                            }
                        }
                        break;
                    }
                },
                Ok(None) => self.state.pending = true,
                Err(err) => {
                    clear_overlapped_signal(&self.state.overlapped, self.state.label);
                    if is_eof_error(&err) {
                        self.mark_broken_pipe();
                        return Err(io::Error::from(ErrorKind::BrokenPipe));
                    } else {
                        return Err(err);
                    }
                },
            }
        }

        if self.write_pos >= self.write_len {
            self.write_len = 0;
            self.write_pos = 0;
            clear_overlapped_signal(&self.state.overlapped, self.state.label);
            if post_on_completion {
                self.post_writable_if_needed();
            }
        }

        Ok(())
    }

    fn complete_pending_write(&mut self) -> io::Result<()> {
        if !self.state.pending {
            return Ok(());
        }

        // SAFETY: Checking result of operation initiated in `drain_immediate_writes`.
        let result = unsafe { self.state.pipe.result(self.state.overlapped.raw()) };

        if let Err(err) = &result {
            if err.raw_os_error() == Some(ERROR_IO_INCOMPLETE as i32) {
                return Err(io::Error::from(ErrorKind::WouldBlock));
            }
        }

        clear_overlapped_signal(&self.state.overlapped, self.state.label);

        match result {
            Ok(0) => {
                self.mark_broken_pipe();
                return Err(io::Error::from(ErrorKind::BrokenPipe));
            },
            Ok(n) => {
                self.state.pending = false;
                self.write_pos += n;
            },
            Err(err) => {
                self.mark_broken_pipe();
                if is_eof_error(&err) {
                    return Err(io::Error::from(ErrorKind::BrokenPipe));
                } else {
                    return Err(err);
                }
            },
        }

        self.drain_immediate_writes(true)
    }
}

impl Write for IocpWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        match self.advance() {
            Ok(()) => {},
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                return Err(io::Error::from(ErrorKind::WouldBlock));
            },
            Err(err) => return Err(err),
        }

        if self.has_pending_io() {
            return Err(io::Error::from(ErrorKind::WouldBlock));
        }

        let accepted = cmp::min(buf.len(), self.state.buf.len());
        copy_into_maybe_uninit_slice(&mut self.state.buf, &buf[..accepted]);
        self.write_pos = 0;
        self.write_len = accepted;
        if let Err(err) = self.drain_immediate_writes(false) {
            // Reset internal buffer state so subsequent calls (if any) do not
            // attempt to drain stale data from the failed write.
            // `mark_broken_pipe` already handles BrokenPipe/EOF cleanup; this
            // covers unexpected non-pipe errors where write_pos may have
            // advanced past zero from a partial synchronous write.
            if !self.broken_pipe {
                self.write_pos = 0;
                self.write_len = 0;
            }
            return Err(err);
        }

        Ok(accepted)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "iocp_tests.rs"]
mod tests;
