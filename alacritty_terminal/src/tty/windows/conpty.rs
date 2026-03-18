use log::{debug, info, warn};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, c_void};
use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind, Result};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{mem, ptr};

use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS,
    ERROR_IO_PENDING, ERROR_NOT_FOUND, ERROR_OPERATION_ABORTED, ERROR_PIPE_CONNECTED, HANDLE,
    INVALID_HANDLE_VALUE, LocalFree, S_OK, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_INBOUND, PIPE_ACCESS_OUTBOUND,
};
use windows_sys::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE,
};
use windows_sys::core::{HRESULT, PWSTR};
use windows_sys::{s, w};

use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateEventW, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, InitializeProcThreadAttributeList,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    STARTUPINFOW, UpdateProcThreadAttribute, WaitForSingleObject,
};

use crate::event::{OnResize, WindowSize};
use crate::tty::Options;
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::windows::iocp::{IocpReader, IocpWriter};
use crate::tty::windows::{Pty, cmdline, win32_string};

const PIPE_CAPACITY: usize = crate::event_loop::READ_BUFFER_SIZE;
const CONNECT_NAMED_PIPE_TIMEOUT_MS: u32 = 2_000;
const CONNECT_NAMED_PIPE_CANCEL_TIMEOUT_MS: u32 = 100;
const CONNECT_NAMED_PIPE_RECLAIM_TIMEOUT_MS: u32 = 30_000;
static PIPE_COUNTER: AtomicU64 = AtomicU64::new(0);
use super::wait_reclaim::ReclaimCounters;

static CONNECT_RECLAIM: ReclaimCounters = ReclaimCounters::new();

#[cfg(test)]
fn connect_reclaim_stats() -> super::wait_reclaim::ReclaimStats {
    CONNECT_RECLAIM.snapshot()
}

#[derive(Copy, Clone)]
enum PipeKind {
    Conout,
    Conin,
}

struct NamedPipeSecurity {
    descriptor: *mut c_void,
}

impl NamedPipeSecurity {
    fn new() -> Result<Self> {
        // Restrict access to the creating user (owner rights) and LocalSystem.
        const PIPE_SDDL: windows_sys::core::PCWSTR = w!("D:P(A;;GA;;;SY)(A;;GA;;;OW)");
        let mut descriptor = ptr::null_mut();
        let success = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PIPE_SDDL,
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if success == 0 {
            // Fall back to default security rather than failing to launch.
            // This can happen in restricted environments (e.g. certain
            // enterprise configurations or sandboxed execution contexts).
            // The randomized pipe name, FILE_FLAG_FIRST_PIPE_INSTANCE, and
            // the narrow connection window still make unrelated collisions
            // extremely unlikely, so startup should continue.
            log::warn!(
                "Failed to create restricted SDDL for named pipe, falling back to default \
                 security: {}",
                Error::last_os_error()
            );
            return Ok(Self { descriptor: ptr::null_mut() });
        }

        Ok(Self { descriptor })
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor,
            bInheritHandle: 0,
        }
    }
}

impl Drop for NamedPipeSecurity {
    fn drop(&mut self) {
        if !self.descriptor.is_null() {
            unsafe {
                let _ = LocalFree(self.descriptor as _);
            }
        }
    }
}

struct ProcThreadAttributeListGuard {
    ptr: *mut c_void,
}

impl ProcThreadAttributeListGuard {
    fn new(ptr: *mut c_void) -> Self {
        Self { ptr }
    }
}

impl Drop for ProcThreadAttributeListGuard {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { DeleteProcThreadAttributeList(self.ptr.cast()) };
        }
    }
}

struct PendingConnectResources {
    overlapped: Box<OVERLAPPED>,
    event: OwnedHandle,
}

// SAFETY: The `OVERLAPPED` is heap-allocated and paired with its event handle.
// This bundle is only moved to a background thread so it can stay alive until
// the kernel is done referencing it after a timed-out `ConnectNamedPipe`.
unsafe impl Send for PendingConnectResources {}

impl PendingConnectResources {
    fn new() -> Result<Self> {
        let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event.is_null() {
            return Err(Error::last_os_error());
        }

        let event = unsafe { OwnedHandle::from_raw_handle(event as _) };
        let mut overlapped = Box::new(unsafe { mem::zeroed::<OVERLAPPED>() });
        overlapped.hEvent = event.as_raw_handle() as HANDLE;

        Ok(Self { overlapped, event })
    }

    fn event_handle(&self) -> HANDLE {
        self.event.as_raw_handle() as HANDLE
    }

    fn overlapped_ptr(&mut self) -> *mut OVERLAPPED {
        self.overlapped.as_mut()
    }
}

/// Load the pseudoconsole API from conpty.dll if possible, otherwise use the
/// standard Windows API.
///
/// The conpty.dll from the Windows Terminal project
/// supports loading OpenConsole.exe, which offers many improvements and
/// bugfixes compared to the standard conpty that ships with Windows.
///
/// The conpty.dll and OpenConsole.exe files will be searched in PATH and in
/// the directory where Alacritty's executable is located.
type CreatePseudoConsoleFn =
    unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> HRESULT;
type ResizePseudoConsoleFn = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type ClosePseudoConsoleFn = unsafe extern "system" fn(HPCON);

struct ConptyApi {
    create: CreatePseudoConsoleFn,
    resize: ResizePseudoConsoleFn,
    close: ClosePseudoConsoleFn,
}

impl ConptyApi {
    fn new() -> Self {
        match Self::load_conpty() {
            Some(conpty) => {
                info!("Using conpty.dll for pseudoconsole");
                conpty
            },
            None => {
                // Cannot load conpty.dll - use the standard Windows API.
                info!("Using Windows API for pseudoconsole");
                Self {
                    create: CreatePseudoConsole,
                    resize: ResizePseudoConsole,
                    close: ClosePseudoConsole,
                }
            },
        }
    }

    /// Try loading ConptyApi from conpty.dll library.
    fn load_conpty() -> Option<Self> {
        type LoadedFn = unsafe extern "system" fn() -> isize;
        unsafe {
            let hmodule = LoadLibraryW(w!("conpty.dll"));
            if hmodule.is_null() {
                return None;
            }
            let create_fn = GetProcAddress(hmodule, s!("CreatePseudoConsole"))?;
            let resize_fn = GetProcAddress(hmodule, s!("ResizePseudoConsole"))?;
            let close_fn = GetProcAddress(hmodule, s!("ClosePseudoConsole"))?;

            Some(Self {
                create: mem::transmute::<LoadedFn, CreatePseudoConsoleFn>(create_fn),
                resize: mem::transmute::<LoadedFn, ResizePseudoConsoleFn>(resize_fn),
                close: mem::transmute::<LoadedFn, ClosePseudoConsoleFn>(close_fn),
            })
        }
    }
}

/// RAII Pseudoconsole.
pub struct Conpty {
    pub handle: HPCON,
    api: ConptyApi,
}

impl Drop for Conpty {
    fn drop(&mut self) {
        // This blocks until the conout pipe is drained, so `Pty::drop` in
        // `windows/mod.rs` must keep the reader alive until after
        // `ClosePseudoConsole` returns.
        //
        // See PR #3084 and https://docs.microsoft.com/en-us/windows/console/closepseudoconsole.
        unsafe { (self.api.close)(self.handle) }
    }
}

// The ConPTY handle can be sent between threads.
unsafe impl Send for Conpty {}

pub fn new(config: &Options, window_size: WindowSize) -> Result<Pty> {
    let api = ConptyApi::new();
    let mut conpty_handle: HPCON = 0;

    let (conout, conout_pty_handle) =
        overlapped_named_pipe_end(PIPE_CAPACITY as u32, PipeKind::Conout)?;
    let (conin, conin_pty_handle) =
        overlapped_named_pipe_end(PIPE_CAPACITY as u32, PipeKind::Conin)?;

    // Create the Pseudo Console, using the pipes.
    let coord: COORD = window_size.into();
    let result = unsafe {
        (api.create)(
            coord,
            conin_pty_handle.as_raw_handle() as HANDLE,
            conout_pty_handle.as_raw_handle() as HANDLE,
            0,
            &mut conpty_handle as *mut _,
        )
    };

    if result != S_OK {
        return Err(Error::other(format!(
            "CreatePseudoConsole failed with HRESULT 0x{result:08X}"
        )));
    }

    // Keep ConPTY handle under RAII so early returns after this point cannot leak it.
    let conpty = Conpty { handle: conpty_handle as HPCON, api };

    // Allocate the IOCP sidecars before spawning the shell, so post-spawn
    // failure handling is limited to process-handle registration.
    let conin = IocpWriter::new(conin, PIPE_CAPACITY)?;
    let conout = IocpReader::new(conout, PIPE_CAPACITY)?;

    let mut success;

    // Prepare child process startup info.

    let mut size: usize = 0;

    let mut startup_info_ex: STARTUPINFOEXW = unsafe { mem::zeroed() };

    startup_info_ex.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;

    // Setting this flag but leaving all the handles as default (null) ensures the
    // PTY process does not inherit any handles from this Alacritty process.
    startup_info_ex.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;

    // Create the appropriately sized thread attribute list.
    unsafe {
        let failure =
            InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size as *mut usize) > 0;

        // This call was expected to return false.
        if failure {
            return Err(Error::last_os_error());
        }
    }

    let mut attr_list: Box<[u8]> = vec![0; size].into_boxed_slice();

    // Set startup info's attribute list & initialize it
    //
    // Lint failure is spurious; it's because winapi's definition of PROC_THREAD_ATTRIBUTE_LIST
    // implies it is one pointer in size (32 or 64 bits) but really this is just a dummy value.
    // Casting a *mut u8 (pointer to 8 bit type) might therefore not be aligned correctly in
    // the compiler's eyes.
    #[allow(clippy::cast_ptr_alignment)]
    {
        startup_info_ex.lpAttributeList = attr_list.as_mut_ptr() as _;
    }

    unsafe {
        success = InitializeProcThreadAttributeList(
            startup_info_ex.lpAttributeList,
            1,
            0,
            &mut size as *mut usize,
        ) > 0;

        if !success {
            return Err(Error::last_os_error());
        }
    }
    let _attr_list_guard =
        ProcThreadAttributeListGuard::new(startup_info_ex.lpAttributeList.cast());

    // Set thread attribute list's Pseudo Console to the specified ConPTY.
    unsafe {
        success = UpdateProcThreadAttribute(
            startup_info_ex.lpAttributeList,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            conpty.handle as *mut std::ffi::c_void,
            mem::size_of::<HPCON>(),
            ptr::null_mut(),
            ptr::null_mut(),
        ) > 0;

        if !success {
            return Err(Error::last_os_error());
        }
    }

    // Prepare child process creation arguments.
    let cmdline = win32_string(&cmdline(config));
    let cwd = config.working_directory.as_ref().map(win32_string);
    let mut creation_flags = EXTENDED_STARTUPINFO_PRESENT;
    let custom_env_block = convert_custom_env(&config.env);
    let custom_env_block_pointer = match &custom_env_block {
        Some(custom_env_block) => {
            creation_flags |= CREATE_UNICODE_ENVIRONMENT;
            custom_env_block.as_ptr() as *mut std::ffi::c_void
        },
        None => ptr::null_mut(),
    };

    let mut proc_info: PROCESS_INFORMATION = unsafe { mem::zeroed() };
    let create_process_error = unsafe {
        success = CreateProcessW(
            ptr::null(),
            cmdline.as_ptr() as PWSTR,
            ptr::null_mut(),
            ptr::null_mut(),
            false as i32,
            creation_flags,
            custom_env_block_pointer,
            cwd.as_ref().map_or_else(ptr::null, |s| s.as_ptr()),
            &mut startup_info_ex.StartupInfo as *mut STARTUPINFOW,
            &mut proc_info as *mut PROCESS_INFORMATION,
        ) > 0;

        if success { None } else { Some(Error::last_os_error()) }
    };

    // Per Microsoft ConPTY guidance, close these references after `CreateProcessW`
    // returns (success or failure). Failure to drop these handles will prevent EOF
    // from being triggered, leading to hard-to-debug deadlocks.
    drop(conin_pty_handle);
    drop(conout_pty_handle);

    if let Some(err) = create_process_error {
        return Err(err);
    }

    // Close the child thread handle immediately; Alacritty doesn't use it.
    if !proc_info.hThread.is_null() {
        unsafe {
            let _ = CloseHandle(proc_info.hThread);
        }
        proc_info.hThread = ptr::null_mut();
    }

    if proc_info.hProcess.is_null() {
        return Err(Error::other("CreateProcessW succeeded without a process handle"));
    }
    let child_process = unsafe { OwnedHandle::from_raw_handle(proc_info.hProcess as _) };
    let child_process = super::SpawnedProcessGuard::new(child_process);
    let child_watcher_handle = duplicate_process_handle(child_process.raw_handle())?;
    let child_watcher = ChildExitWatcher::new_owned(child_watcher_handle)?;

    Ok(Pty::new(conpty, conout, conin, child_watcher, child_process))
}

fn duplicate_process_handle(handle: HANDLE) -> Result<OwnedHandle> {
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

    if success == 0 {
        Err(Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(duplicated as _) })
    }
}

/// Create a named pipe server and connect a client `File` to it.
///
/// Uses `FILE_FLAG_FIRST_PIPE_INSTANCE` to ensure no other process has raced
/// to create a pipe with our randomly generated name. When available, the
/// restricted SDDL (SY+OW only) further narrows who may connect before our
/// own client does.
fn overlapped_named_pipe_end(buffer_size: u32, kind: PipeKind) -> Result<(OwnedHandle, File)> {
    const NAME_TRIES: usize = 8;
    let kind_name = match kind {
        PipeKind::Conout => "conout",
        PipeKind::Conin => "conin",
    };
    let open_mode = match kind {
        PipeKind::Conout => PIPE_ACCESS_INBOUND,
        PipeKind::Conin => PIPE_ACCESS_OUTBOUND,
    } | FILE_FLAG_OVERLAPPED
        | FILE_FLAG_FIRST_PIPE_INSTANCE;

    let security = NamedPipeSecurity::new()?;
    let mut collision_error = None;

    for attempt in 0..NAME_TRIES {
        let random = random_pipe_suffix()?;
        let pipe_name = format!(
            r"\\.\pipe\alacritty-{}-{}-{}-{:032x}",
            kind_name,
            std::process::id(),
            PIPE_COUNTER.fetch_add(1, Ordering::Relaxed),
            random,
        );

        let mut security_attributes = security.attributes();
        let pipe_name_w = win32_string(&pipe_name);
        let pipe_handle = unsafe {
            CreateNamedPipeW(
                pipe_name_w.as_ptr(),
                open_mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                buffer_size,
                buffer_size,
                0,
                (&mut security_attributes as *mut SECURITY_ATTRIBUTES).cast(),
            )
        };

        if pipe_handle == INVALID_HANDLE_VALUE {
            let err = Error::last_os_error();
            let is_collision = err.raw_os_error() == Some(ERROR_ALREADY_EXISTS as i32);
            let is_access_denied = err.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32);
            if is_collision || is_access_denied {
                if is_access_denied {
                    log::warn!(
                        "named pipe ACCESS_DENIED on {pipe_name} (may indicate a permissions \
                         problem rather than a name collision): {err}"
                    );
                } else if attempt == 0 {
                    log::warn!("named pipe collision on {pipe_name}: {err}");
                } else {
                    log::debug!("named pipe collision on {pipe_name}: {err}");
                }
                collision_error = Some(err);
                continue;
            }
            return Err(err);
        }

        let pipe = unsafe { OwnedHandle::from_raw_handle(pipe_handle as _) };

        // Open the client end before calling ConnectNamedPipe. The client File::open
        // connects immediately; ConnectNamedPipe then returns ERROR_PIPE_CONNECTED,
        // or occasionally completes through the overlapped path. This ordering is
        // safe because the randomized pipe name and FILE_FLAG_FIRST_PIPE_INSTANCE
        // already make off-process races extremely unlikely, and the restricted
        // SDDL narrows that window further when available.
        let peer = match kind {
            PipeKind::Conout => OpenOptions::new().write(true).open(&pipe_name)?,
            PipeKind::Conin => OpenOptions::new().read(true).open(&pipe_name)?,
        };

        connect_named_pipe_overlapped(&pipe)?;

        return Ok((pipe, peer));
    }

    let err = collision_error.unwrap_or_else(|| {
        Error::new(ErrorKind::AlreadyExists, "failed to allocate unique named pipe endpoint")
    });
    warn!("exhausted {NAME_TRIES} named pipe allocation attempts for {kind_name}: {err}");
    Err(err)
}

/// Complete a named-pipe server connection on an overlapped handle.
///
/// `ConnectNamedPipe` requires a valid `OVERLAPPED` when the pipe was created
/// with `FILE_FLAG_OVERLAPPED`. We still accept `ERROR_PIPE_CONNECTED` because
/// the peer may connect in the small window between `CreateNamedPipeW` and
/// this call.
fn connect_named_pipe_overlapped(pipe: &OwnedHandle) -> Result<()> {
    let mut resources = PendingConnectResources::new()?;
    let connected =
        unsafe { ConnectNamedPipe(pipe.as_raw_handle() as _, resources.overlapped_ptr()) };
    if connected != 0 {
        return Ok(());
    }

    let err = Error::last_os_error();
    match err.raw_os_error() {
        Some(code) if code == ERROR_PIPE_CONNECTED as i32 => Ok(()),
        Some(code) if code == ERROR_IO_PENDING as i32 => {
            let wait_res = unsafe {
                WaitForSingleObject(resources.event_handle(), CONNECT_NAMED_PIPE_TIMEOUT_MS)
            };
            match wait_res {
                WAIT_OBJECT_0 => complete_named_pipe_connect(pipe, resources.overlapped_ptr()),
                WAIT_TIMEOUT => cancel_or_reclaim_timed_out_connect(pipe, resources),
                other => {
                    let wait_err = Error::last_os_error();
                    fail_after_unexpected_connect_wait_with_cleanup(
                        pipe,
                        resources,
                        other,
                        wait_err,
                        |pipe, resources| {
                            cleanup_pending_connect(
                                pipe,
                                resources,
                                "cleaning up ConnectNamedPipe after unexpected wait failure",
                            )
                        },
                    )
                },
            }
        },
        _ => Err(err),
    }
}

fn complete_named_pipe_connect(pipe: &OwnedHandle, overlapped: *mut OVERLAPPED) -> Result<()> {
    let mut transferred = 0;
    let completed =
        unsafe { GetOverlappedResult(pipe.as_raw_handle() as _, overlapped, &mut transferred, 0) };
    if completed != 0 { Ok(()) } else { Err(Error::last_os_error()) }
}

enum PendingConnectCleanup {
    Connected,
    Cancelled,
    Deferred,
}

fn cleanup_pending_connect(
    pipe: &OwnedHandle,
    mut resources: PendingConnectResources,
    context: &'static str,
) -> Result<PendingConnectCleanup> {
    // Best-effort cancellation. If this races with successful completion, the
    // follow-up `GetOverlappedResult` below will surface that and we can still
    // continue startup instead of turning a narrow race into a hard failure.
    let cancel_result =
        unsafe { CancelIoEx(pipe.as_raw_handle() as _, resources.overlapped_ptr()) };
    if cancel_result == 0 {
        let cancel_err = Error::last_os_error();
        if cancel_err.raw_os_error() != Some(ERROR_NOT_FOUND as i32) {
            warn!("CancelIoEx failed after {context}: {cancel_err}");
        }
    }

    let wait_res = unsafe {
        WaitForSingleObject(resources.event_handle(), CONNECT_NAMED_PIPE_CANCEL_TIMEOUT_MS)
    };
    match wait_res {
        WAIT_OBJECT_0 => match complete_named_pipe_connect(pipe, resources.overlapped_ptr()) {
            Ok(()) => Ok(PendingConnectCleanup::Connected),
            Err(err) if err.raw_os_error() == Some(ERROR_OPERATION_ABORTED as i32) => {
                Ok(PendingConnectCleanup::Cancelled)
            },
            Err(err) => Err(err),
        },
        WAIT_TIMEOUT => {
            spawn_pending_connect_reclaim(resources);
            Ok(PendingConnectCleanup::Deferred)
        },
        other => {
            let wait_err = Error::last_os_error();
            warn!(
                "WaitForSingleObject returned unexpected value {other} while {context} (last \
                 error: {wait_err}); continuing cleanup asynchronously"
            );
            spawn_pending_connect_reclaim(resources);
            Ok(PendingConnectCleanup::Deferred)
        },
    }
}

fn fail_after_unexpected_connect_wait_with_cleanup<F>(
    pipe: &OwnedHandle,
    resources: PendingConnectResources,
    wait_result: u32,
    wait_error: Error,
    cleanup: F,
) -> Result<()>
where
    F: FnOnce(&OwnedHandle, PendingConnectResources) -> Result<PendingConnectCleanup>,
{
    warn!(
        "WaitForSingleObject returned unexpected value {wait_result} while waiting for \
         ConnectNamedPipe (last error: {wait_error}); attempting cancellation"
    );

    if let Err(cleanup_err) = cleanup(pipe, resources) {
        warn!("ConnectNamedPipe cleanup after unexpected wait result failed: {cleanup_err}");
    }

    Err(Error::other(format!(
        "WaitForSingleObject returned unexpected value {wait_result} while waiting for \
         ConnectNamedPipe: {wait_error}"
    )))
}

fn cancel_or_reclaim_timed_out_connect(
    pipe: &OwnedHandle,
    resources: PendingConnectResources,
) -> Result<()> {
    let stats = CONNECT_RECLAIM.note_timeout();
    debug!(
        "ConnectNamedPipe exceeded {}ms startup timeout; attempting cancellation ({})",
        CONNECT_NAMED_PIPE_TIMEOUT_MS, stats,
    );

    match cleanup_pending_connect(pipe, resources, "draining timed out ConnectNamedPipe")? {
        PendingConnectCleanup::Connected => Ok(()),
        PendingConnectCleanup::Cancelled | PendingConnectCleanup::Deferred => {
            Err(Error::new(ErrorKind::TimedOut, "ConnectNamedPipe timed out"))
        },
    }
}

fn spawn_pending_connect_reclaim(resources: PendingConnectResources) {
    let stats = CONNECT_RECLAIM.note_submission();
    debug!("submitted timed out ConnectNamedPipe resources for background reclaim ({})", stats);

    let wait_event_handle = resources.event_handle();

    super::wait_reclaim::register_wait_once(
        wait_event_handle,
        CONNECT_NAMED_PIPE_RECLAIM_TIMEOUT_MS,
        "conpty connect reclaim",
        Box::new(move |timed_out| {
            if timed_out {
                let stats = CONNECT_RECLAIM.note_leak();
                warn!(
                    "timed out waiting {}ms to reclaim timed out ConnectNamedPipe resources; \
                     leaking OVERLAPPED state to avoid use-after-free ({})",
                    CONNECT_NAMED_PIPE_RECLAIM_TIMEOUT_MS, stats,
                );
                mem::forget(resources);
            } else {
                let stats = CONNECT_RECLAIM.note_completion();
                debug!("completed timed out ConnectNamedPipe background reclaim ({})", stats,);
                drop(resources);
            }
        }),
    );
}

fn random_pipe_suffix() -> Result<u128> {
    let mut bytes = [0u8; 16];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };

    if status < 0 {
        return Err(Error::other(format!("BCryptGenRandom failed with NTSTATUS=0x{status:08x}")));
    }

    Ok(u128::from_le_bytes(bytes))
}

// Windows environment variables are case-insensitive, so deduplicate them here while converting
// the environment block for `CreateProcessW`.
//
// https://learn.microsoft.com/en-us/previous-versions/troubleshoot/windows/win32/createprocess-cannot-eliminate-duplicate-variables#environment-variables
fn convert_custom_env(custom_env: &HashMap<String, String>) -> Option<Vec<u16>> {
    // Windows inherits parent's env when no `lpEnvironment` parameter is specified.
    if custom_env.is_empty() {
        return None;
    }

    let mut env_vars = Vec::new();
    let mut all_env_keys = HashSet::new();

    // Sort first so case-insensitive duplicates collapse deterministically:
    // the uppercase key is the primary sort key, and the original-case key
    // is the tiebreaker, so the first `insert()` into `all_env_keys` always
    // picks the lexicographically smallest original-case variant.
    let mut custom_entries: Vec<_> = custom_env
        .iter()
        .map(|(k, v)| {
            let k_os = std::ffi::OsString::from(k);
            let k_upper = k_os.to_ascii_uppercase();
            (k_upper, k_os, std::ffi::OsString::from(v), k, v)
        })
        .collect();
    custom_entries.sort_unstable_by(|(upper_a, _, _, key_a, _), (upper_b, _, _, key_b, _)| {
        upper_a.cmp(upper_b).then_with(|| key_a.cmp(key_b))
    });

    for (custom_key_upper, custom_key_os, custom_val_os, custom_key, custom_value) in custom_entries
    {
        if all_env_keys.insert(custom_key_upper.clone()) {
            env_vars.push((custom_key_upper, custom_key_os, custom_val_os));
        } else {
            warn!(
                "Omitting environment variable pair with duplicate key: \
                 '{custom_key}={custom_value}'"
            );
        }
    }

    // Pull the current process environment after, to avoid overwriting the user provided one.
    let mut inherited_entries: Vec<_> = std::env::vars_os()
        .map(|(k, v)| {
            let k_upper = k.to_ascii_uppercase();
            (k_upper, k, v)
        })
        .collect();
    inherited_entries.sort_unstable_by(|(upper_a, key_a, _), (upper_b, key_b, _)| {
        upper_a.cmp(upper_b).then_with(|| key_a.cmp(key_b))
    });

    for (inherited_key_upper, inherited_key, inherited_value) in inherited_entries {
        if all_env_keys.insert(inherited_key_upper.clone()) {
            env_vars.push((inherited_key_upper, inherited_key, inherited_value));
        }
    }

    // Windows requires the environment block to be sorted alphabetically by key.
    env_vars.sort_by(|(upper_a, ..), (upper_b, ..)| upper_a.cmp(upper_b));

    let mut converted_block = Vec::new();
    for (_, key, value) in env_vars {
        add_windows_env_key_value_to_block(&mut converted_block, &key, &value);
    }

    converted_block.push(0);
    Some(converted_block)
}

// According to the `lpEnvironment` parameter description:
// https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessa#parameters
//
// > An environment block consists of a null-terminated block of null-terminated strings. Each
// string is in the following form:
// >
// > name=value\0
fn add_windows_env_key_value_to_block(block: &mut Vec<u16>, key: &OsStr, value: &OsStr) {
    block.extend(key.encode_wide());
    block.push('=' as u16);
    block.extend(value.encode_wide());
    block.push(0);
}

impl OnResize for Conpty {
    fn on_resize(&mut self, window_size: WindowSize) {
        let coord: COORD = window_size.into();
        let result = unsafe { (self.api.resize)(self.handle, coord) };
        if result != S_OK {
            log::warn!("ResizePseudoConsole failed with HRESULT 0x{result:08X}");
        }
    }
}
impl From<WindowSize> for COORD {
    fn from(window_size: WindowSize) -> Self {
        let max_coord = i16::MAX - 1;
        let lines = std::cmp::min(window_size.num_lines, max_coord as u16) as i16;
        let columns = std::cmp::min(window_size.num_cols, max_coord as u16) as i16;
        COORD { X: columns, Y: lines }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::io::{Error, ErrorKind, Read};
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::process::Command;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, MutexGuard};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_HANDLE, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    use windows_sys::core::PWSTR;

    use super::{NamedPipeSecurity, convert_custom_env, new};
    use crate::event::WindowSize;
    use crate::tty::{ChildEvent, EventedPty, EventedReadWrite, Options, Shell};

    // Serialize ConPTY tests to avoid exhausting system pseudoconsole
    // resources and to keep global reclaim-stat assertions deterministic.
    static CONPTY_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_conpty_tests() -> MutexGuard<'static, ()> {
        CONPTY_TEST_LOCK.lock().unwrap()
    }

    fn pwstr_to_string(ptr: PWSTR) -> String {
        if ptr.is_null() {
            return String::new();
        }

        unsafe {
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let wide = std::slice::from_raw_parts(ptr, len);
            OsString::from_wide(wide).to_string_lossy().into_owned()
        }
    }

    fn decode_env_block(block: &[u16]) -> Vec<(String, String)> {
        let mut entries = Vec::new();
        let mut start = 0;

        while start < block.len() {
            let mut end = start;
            while end < block.len() && block[end] != 0 {
                end += 1;
            }

            if end == start {
                break;
            }

            let entry = OsString::from_wide(&block[start..end]).to_string_lossy().into_owned();
            let (key, value) =
                entry.split_once('=').unwrap_or_else(|| panic!("invalid env block entry: {entry}"));
            entries.push((key.to_string(), value.to_string()));
            start = end + 1;
        }

        entries
    }

    fn env_block_to_uppercase_map(block: &[u16]) -> HashMap<String, String> {
        decode_env_block(block)
            .into_iter()
            .map(|(key, value)| (key.to_ascii_uppercase(), value))
            .collect()
    }

    fn existing_env_entry() -> (String, String) {
        let (key, value) = std::env::vars_os()
            .find(|(key, _)| !key.to_string_lossy().starts_with('='))
            .expect("process environment unexpectedly missing a non-drive inherited variable");
        (key.to_string_lossy().into_owned(), value.to_string_lossy().into_owned())
    }

    fn test_window_size() -> WindowSize {
        WindowSize { num_lines: 24, num_cols: 80, cell_width: 8, cell_height: 16 }
    }

    fn create_unconnected_overlapped_pipe() -> OwnedHandle {
        let security = NamedPipeSecurity::new().expect("failed to create test pipe security");
        let mut security_attributes = security.attributes();
        let pipe_name = format!(
            r"\\.\pipe\alacritty-connect-timeout-test-{}-{:032x}",
            std::process::id(),
            super::random_pipe_suffix().expect("failed to generate random pipe suffix"),
        );
        let pipe_name_w = super::win32_string(&pipe_name);
        let pipe_handle = unsafe {
            super::CreateNamedPipeW(
                pipe_name_w.as_ptr(),
                super::PIPE_ACCESS_INBOUND
                    | super::FILE_FLAG_OVERLAPPED
                    | super::FILE_FLAG_FIRST_PIPE_INSTANCE,
                super::PIPE_TYPE_BYTE
                    | super::PIPE_READMODE_BYTE
                    | super::PIPE_REJECT_REMOTE_CLIENTS,
                1,
                super::PIPE_CAPACITY as u32,
                super::PIPE_CAPACITY as u32,
                0,
                (&mut security_attributes as *mut super::SECURITY_ATTRIBUTES).cast(),
            )
        };
        assert_ne!(
            pipe_handle,
            super::INVALID_HANDLE_VALUE,
            "failed to create unconnected overlapped named pipe: {}",
            Error::last_os_error()
        );

        unsafe { OwnedHandle::from_raw_handle(pipe_handle as _) }
    }

    fn wait_for_child_exit(
        child_handle: &std::os::windows::io::OwnedHandle,
        timeout_ms: u32,
        context: &str,
    ) -> Result<(), String> {
        let wait_result =
            unsafe { WaitForSingleObject(child_handle.as_raw_handle() as HANDLE, timeout_ms) };
        match wait_result {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_TIMEOUT => {
                Err(format!("timed out waiting {timeout_ms}ms for ConPTY child to exit {context}"))
            },
            other => Err(format!(
                "WaitForSingleObject returned unexpected value {other} while waiting for ConPTY \
                 child to exit {context}: {}",
                std::io::Error::last_os_error()
            )),
        }
    }

    #[test]
    fn unexpected_connect_wait_result_runs_cleanup_before_returning_error() {
        let _guard = lock_conpty_tests();

        let pipe_event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        assert!(!pipe_event.is_null(), "failed to create dummy event handle for test");
        let pipe = unsafe { OwnedHandle::from_raw_handle(pipe_event as _) };
        let resources = super::PendingConnectResources::new().unwrap();
        let cleanup_called = AtomicBool::new(false);

        let err = super::fail_after_unexpected_connect_wait_with_cleanup(
            &pipe,
            resources,
            u32::MAX,
            Error::from_raw_os_error(ERROR_INVALID_HANDLE as i32),
            |_, resources| {
                cleanup_called.store(true, Ordering::Release);
                drop(resources);
                Ok(super::PendingConnectCleanup::Deferred)
            },
        )
        .unwrap_err();

        assert!(cleanup_called.load(Ordering::Acquire), "unexpected wait branch must run cleanup");
        assert_eq!(err.kind(), ErrorKind::Other);
        assert!(
            err.to_string().contains("unexpected value"),
            "unexpected wait branch should preserve the wait failure in the returned error: {err}"
        );
    }

    #[test]
    fn timed_out_connect_named_pipe_updates_reclaim_stats() {
        const STATS_TIMEOUT: Duration = Duration::from_secs(2);

        let _guard = lock_conpty_tests();
        let baseline = super::connect_reclaim_stats();
        let pipe = create_unconnected_overlapped_pipe();

        let err = super::connect_named_pipe_overlapped(&pipe).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::TimedOut);
        assert_eq!(err.to_string(), "ConnectNamedPipe timed out");

        let stats = crate::tty::windows::wait_reclaim::wait_for_reclaim_stats_change(
            baseline,
            super::connect_reclaim_stats,
            |stats| stats.timeouts == baseline.timeouts + 1,
            STATS_TIMEOUT,
            "connect",
        );
        assert_eq!(stats.leaked, baseline.leaked, "timeout path should not leak resources");

        let submitted_delta = stats.submitted - baseline.submitted;
        assert!(
            submitted_delta <= 1,
            "timed out connect should submit at most one background reclaim job: \
             baseline={baseline:?}, current={stats:?}"
        );

        drop(pipe);

        if submitted_delta == 1 {
            crate::tty::windows::wait_reclaim::wait_for_reclaim_stats_change(
                baseline,
                super::connect_reclaim_stats,
                |stats| {
                    stats.timeouts == baseline.timeouts + 1
                        && stats.submitted == baseline.submitted + 1
                        && stats.completed == baseline.completed + 1
                        && stats.leaked == baseline.leaked
                },
                STATS_TIMEOUT,
                "connect",
            );
        }
    }

    #[test]
    fn spawned_process_guard_terminates_child_on_drop() {
        let _guard = lock_conpty_tests();

        let mut child = Command::new("cmd.exe")
            .args(["/Q", "/D", "/C", "timeout /t 30 /nobreak >nul"])
            .spawn()
            .unwrap();

        let guard = crate::tty::windows::SpawnedProcessGuard::new(
            super::duplicate_process_handle(child.as_raw_handle() as HANDLE).unwrap(),
        );
        drop(guard);

        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(1));
    }

    /// Spawn a ConPTY that runs `echo <marker>`, poll-drain its output until
    /// the marker appears and the child exits, then return the collected
    /// output, exit flag, and exit code.
    fn poll_conpty_echo(marker: &str) -> (String, bool, Option<i32>) {
        const DEADLINE: Duration = Duration::from_secs(10);
        const POLL_INTERVAL: Duration = Duration::from_millis(10);

        let mut options = Options {
            shell: Some(Shell::new("cmd.exe".into(), vec![
                "/Q".into(),
                "/D".into(),
                "/C".into(),
                format!("echo {marker}"),
            ])),
            ..Options::default()
        };
        options.escape_args = true;

        let window_size = test_window_size();
        let mut pty = new(&options, window_size).unwrap();

        let mut output = Vec::new();
        let mut child_exited = false;
        let mut exit_status = None;
        let deadline = Instant::now() + DEADLINE;

        while Instant::now() < deadline {
            loop {
                let mut buf = [0u8; 4096];
                match pty.reader().read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => output.extend_from_slice(&buf[..n]),
                    Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                    Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(err) => panic!("failed to read from ConPTY: {err}"),
                }
            }

            while let Some(event) = pty.next_child_event() {
                match event {
                    ChildEvent::Exited(status) => {
                        child_exited = true;
                        exit_status = status;
                    },
                }
            }

            if child_exited && String::from_utf8_lossy(&output).contains(marker) {
                break;
            }

            std::thread::sleep(POLL_INTERVAL);
        }

        let output = String::from_utf8_lossy(&output).into_owned();
        drop(pty);
        (output, child_exited, exit_status.and_then(|status| status.code()))
    }

    #[test]
    fn named_pipe_security_descriptor_is_restricted() {
        let _guard = lock_conpty_tests();

        let security = NamedPipeSecurity::new().unwrap();

        // Restricted environments may force the production fallback to default
        // pipe security. That is supported behavior and should not fail tests.
        if security.descriptor.is_null() {
            let attributes = security.attributes();
            assert!(
                attributes.lpSecurityDescriptor.is_null(),
                "fallback security attributes should keep a null descriptor"
            );
            return;
        }

        let mut sddl_ptr: PWSTR = ptr::null_mut();
        let mut sddl_len = 0;
        let success = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                security.descriptor.cast(),
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut sddl_ptr,
                &mut sddl_len,
            )
        };
        assert_ne!(success, 0, "failed to convert named pipe security descriptor to SDDL");
        assert!(!sddl_ptr.is_null(), "SDDL conversion returned a null pointer");

        let sddl = pwstr_to_string(sddl_ptr);
        unsafe {
            let _ = windows_sys::Win32::Foundation::LocalFree(sddl_ptr as _);
        }

        // The descriptor should use a protected DACL granting full access only to
        // LocalSystem and owner rights.
        assert!(sddl.contains("D:P"), "expected protected DACL in SDDL, got: {sddl}");
        assert!(sddl.contains("(A;;GA;;;SY)"), "expected LocalSystem ACE, got: {sddl}");
        assert!(sddl.contains("(A;;GA;;;OW)"), "expected owner-rights ACE, got: {sddl}");
        assert!(sddl_len > 0, "SDDL length should be non-zero");
    }

    #[test]
    fn oversized_window_size_is_clamped_before_conpty_startup() {
        let _guard = lock_conpty_tests();

        let window_size =
            WindowSize { num_lines: u16::MAX, num_cols: 80, cell_width: 8, cell_height: 16 };
        let coord: super::COORD = window_size.into();
        assert_eq!(coord.Y, i16::MAX - 1);
        assert_eq!(coord.X, 80);

        let window_size =
            WindowSize { num_lines: 24, num_cols: u16::MAX, cell_width: 8, cell_height: 16 };
        let coord: super::COORD = window_size.into();
        assert_eq!(coord.Y, 24);
        assert_eq!(coord.X, i16::MAX - 1);
    }

    #[test]
    fn empty_custom_env_uses_parent_process_environment() {
        let _guard = lock_conpty_tests();

        assert!(convert_custom_env(&HashMap::new()).is_none());
    }

    #[test]
    fn custom_env_block_is_double_nul_terminated_and_deduplicates_case_insensitively() {
        let _guard = lock_conpty_tests();

        let mut custom_env = HashMap::new();
        custom_env.insert("AlacrittyCaseKey".to_string(), "first".to_string());
        custom_env.insert("ALACRITTYCASEKEY".to_string(), "second".to_string());

        let block = convert_custom_env(&custom_env).expect("custom env should build a block");
        assert!(block.len() >= 2, "environment block should contain a double-NUL terminator");
        assert_eq!(&block[block.len() - 2..], &[0, 0], "environment block must end with two NULs");

        let entries = env_block_to_uppercase_map(&block);
        let value = entries
            .get("ALACRITTYCASEKEY")
            .expect("expected one case-insensitive entry for duplicated custom key");
        assert_eq!(value, "second");

        let duplicate_entries: Vec<_> = decode_env_block(&block)
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case("ALACRITTYCASEKEY"))
            .cloned()
            .collect();
        assert_eq!(
            duplicate_entries.len(),
            1,
            "duplicate custom keys should collapse into one entry"
        );
        assert_eq!(duplicate_entries[0].0, "ALACRITTYCASEKEY");
    }

    #[test]
    fn custom_env_block_includes_unique_inherited_variables() {
        let _guard = lock_conpty_tests();

        let (inherited_key, inherited_value) = existing_env_entry();

        let mut custom_env = HashMap::new();
        custom_env.insert("ALACRITTY_TEST_CUSTOM_ONLY".to_string(), "custom".to_string());

        let block = convert_custom_env(&custom_env).expect("custom env should build a block");
        let entries = env_block_to_uppercase_map(&block);

        assert_eq!(entries.get("ALACRITTY_TEST_CUSTOM_ONLY").map(String::as_str), Some("custom"));
        assert_eq!(
            entries.get(&inherited_key.to_ascii_uppercase()).map(String::as_str),
            Some(inherited_value.as_str())
        );
    }

    #[test]
    fn custom_env_block_overrides_inherited_variables_case_insensitively() {
        let _guard = lock_conpty_tests();

        let (inherited_key, _inherited_value) = existing_env_entry();

        let mut custom_env = HashMap::new();
        custom_env.insert(inherited_key.to_ascii_lowercase(), "custom".to_string());

        let block = convert_custom_env(&custom_env).expect("custom env should build a block");
        let entries = decode_env_block(&block);
        let matching_entries: Vec<_> =
            entries.iter().filter(|(key, _)| key.eq_ignore_ascii_case(&inherited_key)).collect();

        assert_eq!(matching_entries.len(), 1, "inherited value should be suppressed by custom env");
        assert_eq!(matching_entries[0].1, "custom");
    }

    #[test]
    fn custom_env_block_sorts_keys_case_insensitively() {
        let _guard = lock_conpty_tests();

        let mut custom_env = HashMap::new();
        custom_env.insert("alacritty_sort_test_z".to_string(), "z".to_string());
        custom_env.insert("Alacritty_Sort_Test_A".to_string(), "a".to_string());
        custom_env.insert("ALACRITTY_SORT_TEST_m".to_string(), "m".to_string());

        let block = convert_custom_env(&custom_env).expect("custom env should build a block");
        let entries = decode_env_block(&block);

        let a_index = entries
            .iter()
            .position(|(key, _)| key.eq_ignore_ascii_case("ALACRITTY_SORT_TEST_A"))
            .expect("sorted environment block missing test key A");
        let m_index = entries
            .iter()
            .position(|(key, _)| key.eq_ignore_ascii_case("ALACRITTY_SORT_TEST_M"))
            .expect("sorted environment block missing test key M");
        let z_index = entries
            .iter()
            .position(|(key, _)| key.eq_ignore_ascii_case("ALACRITTY_SORT_TEST_Z"))
            .expect("sorted environment block missing test key Z");

        assert!(a_index < m_index, "A key should sort before M key");
        assert!(m_index < z_index, "M key should sort before Z key");
    }

    #[test]
    fn conpty_repeated_lifecycle_preserves_output_eof_and_child_exit() {
        let _guard = lock_conpty_tests();

        // Keep a small repetition count as a smoke test for lifecycle flakiness
        // without making the Windows ConPTY suite disproportionately expensive.
        const ITERATIONS: usize = 3;

        for iteration in 0..ITERATIONS {
            let marker = format!("__ALACRITTY_CONPTY_REPEAT_{iteration}__");
            let (output, child_exited, exit_code) = poll_conpty_echo(&marker);

            assert!(
                output.contains(&marker),
                "ConPTY output did not contain iteration marker {marker:?}: {output:?}"
            );
            assert!(child_exited, "ConPTY child did not report exit on iteration {iteration}");
            assert_eq!(exit_code, Some(0), "unexpected exit code on iteration {iteration}");
        }
    }

    #[test]
    fn normal_conpty_lifecycle_does_not_touch_connect_reclaim_stats() {
        const STABILITY_WINDOW: Duration = Duration::from_millis(100);

        let _guard = lock_conpty_tests();
        let baseline = super::connect_reclaim_stats();
        let marker = "__ALACRITTY_CONPTY_CONNECT_RECLAIM_BASELINE__";
        let (output, child_exited, exit_code) = poll_conpty_echo(marker);

        assert!(output.contains(marker), "ConPTY output did not contain marker: {output:?}");
        assert!(child_exited, "ConPTY child did not report exit");
        assert_eq!(exit_code, Some(0), "unexpected exit code");

        crate::tty::windows::wait_reclaim::assert_reclaim_stats_stable(
            baseline,
            super::connect_reclaim_stats,
            STABILITY_WINDOW,
            "normal ConPTY lifecycle",
        );
    }

    #[test]
    fn conpty_repeated_spawn_and_immediate_drop_terminates_child_promptly() {
        let _guard = lock_conpty_tests();

        const ITERATIONS: usize = 5;
        const DROP_EXIT_TIMEOUT_MS: u32 = 5_000;
        const TEST_TIMEOUT: Duration = Duration::from_secs(30);

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                for iteration in 0..ITERATIONS {
                    let mut options = Options {
                        shell: Some(Shell::new("cmd.exe".into(), vec![
                            "/Q".into(),
                            "/D".into(),
                            "/C".into(),
                            "timeout /t 30 /nobreak >nul".into(),
                        ])),
                        ..Options::default()
                    };
                    options.escape_args = true;

                    let window_size = test_window_size();
                    let pty = new(&options, window_size).map_err(|err| {
                        format!("failed to create ConPTY on iteration {iteration}: {err}")
                    })?;
                    let child_handle = super::duplicate_process_handle(
                        pty.child_watcher().raw_handle(),
                    )
                    .map_err(|err| {
                        format!(
                            "failed to duplicate ConPTY child handle on iteration {iteration}: \
                             {err}"
                        )
                    })?;

                    drop(pty);

                    wait_for_child_exit(
                        &child_handle,
                        DROP_EXIT_TIMEOUT_MS,
                        &format!("after drop on iteration {iteration}"),
                    )?;
                }

                Ok(())
            })();

            done_tx.send(result).ok();
        });

        match done_rx.recv_timeout(TEST_TIMEOUT) {
            Ok(Ok(())) => (),
            Ok(Err(err)) => panic!("{err}"),
            Err(_) => panic!("timed out waiting for repeated ConPTY spawn/drop stress test"),
        }
    }

    #[test]
    fn conpty_drop_with_large_pending_output_terminates_child_promptly() {
        let _guard = lock_conpty_tests();

        const DROP_EXIT_TIMEOUT_MS: u32 = 5_000;
        const PRE_DROP_SETTLE: Duration = Duration::from_millis(100);
        const TEST_TIMEOUT: Duration = Duration::from_secs(30);

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let mut options = Options {
                    shell: Some(Shell::new("cmd.exe".into(), vec![
                        "/Q".into(),
                        "/D".into(),
                        "/C".into(),
                        "for /L %i in (1,1,50000) do @echo \
                         __ALACRITTY_CONPTY_DROP_PENDING_OUTPUT_STRESS_LINE_%\
                         i__XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
                            .into(),
                    ])),
                    ..Options::default()
                };
                options.escape_args = true;

                let pty = new(&options, test_window_size()).map_err(|err| {
                    format!("failed to create ConPTY for large-output drop test: {err}")
                })?;
                let child_handle = super::duplicate_process_handle(
                    pty.child_watcher().raw_handle(),
                )
                .map_err(|err| {
                    format!(
                        "failed to duplicate ConPTY child handle for large-output drop test: {err}"
                    )
                })?;

                // The command emits several megabytes of output, far more than
                // the PTY pipe capacity. Give it a brief head start so drop
                // happens while unread conout data is still queued.
                std::thread::sleep(PRE_DROP_SETTLE);

                let pre_drop_wait =
                    unsafe { WaitForSingleObject(child_handle.as_raw_handle() as HANDLE, 0) };
                if pre_drop_wait != WAIT_TIMEOUT {
                    return Err(format!(
                        "large-output child exited too early before drop stress could run (wait \
                         result {pre_drop_wait})"
                    ));
                }

                drop(pty);

                wait_for_child_exit(
                    &child_handle,
                    DROP_EXIT_TIMEOUT_MS,
                    "after dropping PTY with unread large output",
                )
            })();

            done_tx.send(result).ok();
        });

        match done_rx.recv_timeout(TEST_TIMEOUT) {
            Ok(Ok(())) => (),
            Ok(Err(err)) => panic!("{err}"),
            Err(_) => panic!("timed out waiting for ConPTY large-output drop stress test"),
        }
    }
}
