use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind, Read, Write};
use std::iter::once;
use std::mem::{self, MaybeUninit};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use polling::{Event, Events, PollMode, Poller};
use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_INBOUND, PIPE_ACCESS_OUTBOUND,
};
use windows_sys::Win32::System::IO::GetOverlappedResult;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE,
};
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};

use super::reclaim::{LeakedResources, drop_reclaim_stats, submit_for_reclamation_with_timeout};
use super::{IocpReader, IocpWriter, Overlapped, WinOverlapped};

static TEST_PIPE_COUNTER: AtomicU64 = AtomicU64::new(0);
static DROP_RECLAIM_TEST_LOCK: Mutex<()> = Mutex::new(());

fn pipe_name() -> String {
    format!(
        r"\\.\pipe\alacritty-iocp-reader-test-{}-{}",
        std::process::id(),
        TEST_PIPE_COUNTER.fetch_add(1, Ordering::Relaxed),
    )
}

fn win32_string(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

fn create_test_pipe(
    name: &str,
    inbound: bool,
    outbound: bool,
    in_buf: u32,
    out_buf: u32,
) -> OwnedHandle {
    let name_w = win32_string(name);
    let open_mode = if inbound && !outbound {
        PIPE_ACCESS_INBOUND
    } else if !inbound && outbound {
        PIPE_ACCESS_OUTBOUND
    } else {
        PIPE_ACCESS_INBOUND | PIPE_ACCESS_OUTBOUND
    } | FILE_FLAG_OVERLAPPED
        | FILE_FLAG_FIRST_PIPE_INSTANCE;

    let handle = unsafe {
        CreateNamedPipeW(
            name_w.as_ptr(),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            out_buf,
            in_buf,
            0,
            ptr::null_mut(),
        )
    };
    assert_ne!(
        handle,
        windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE,
        "CreateNamedPipeW failed: {}",
        Error::last_os_error()
    );
    unsafe { OwnedHandle::from_raw_handle(handle as _) }
}

fn overlapped_read_end_pair() -> (OwnedHandle, File) {
    let name = pipe_name();
    let pipe = create_test_pipe(&name, true, false, 0, 0);
    let writer = OpenOptions::new().write(true).open(&name).unwrap();
    connect_test_pipe(&pipe);
    (pipe, writer)
}

fn overlapped_write_end_pair() -> (OwnedHandle, File) {
    let name = pipe_name();
    let pipe = create_test_pipe(&name, false, true, 0, 0);
    let client = OpenOptions::new().read(true).open(&name).unwrap();
    connect_test_pipe(&pipe);
    (pipe, client)
}

fn overlapped_write_end_pair_with_buffers(buffer_size: u32) -> (OwnedHandle, File) {
    let name = pipe_name();
    let pipe = create_test_pipe(&name, false, true, buffer_size, buffer_size);
    let client = OpenOptions::new().read(true).open(&name).unwrap();
    connect_test_pipe(&pipe);
    (pipe, client)
}

fn connect_test_pipe(pipe: &OwnedHandle) {
    let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
    assert!(!event.is_null(), "failed to create ConnectNamedPipe event");
    let event = unsafe { OwnedHandle::from_raw_handle(event as _) };

    let mut overlapped: WinOverlapped = unsafe { mem::zeroed() };
    overlapped.hEvent = event.as_raw_handle() as HANDLE;

    let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle() as _, &mut overlapped) };
    if connected != 0 {
        return;
    }

    let err = Error::last_os_error();
    match err.raw_os_error() {
        Some(code) if code == ERROR_PIPE_CONNECTED as i32 => {},
        Some(code) if code == ERROR_IO_PENDING as i32 => {
            let mut transferred = 0;
            let completed = unsafe {
                GetOverlappedResult(pipe.as_raw_handle() as _, &overlapped, &mut transferred, 1)
            };
            assert_ne!(
                completed,
                0,
                "GetOverlappedResult failed after ConnectNamedPipe pending state: {}",
                Error::last_os_error()
            );
        },
        _ => panic!("ConnectNamedPipe failed: {err}"),
    }
}

fn leaked_resources() -> LeakedResources {
    let (overlapped, wait_event) = Overlapped::initialize_with_manual_reset_event().unwrap();
    LeakedResources {
        label: "IocpReclaimTest",
        overlapped: Box::new(overlapped),
        buf: vec![MaybeUninit::uninit(); 16],
        wait_event: Some(wait_event),
    }
}

#[test]
fn reader_reposts_event_after_partial_read() {
    const TOKEN: usize = 31;
    const WAIT_TIMEOUT: Duration = Duration::from_millis(250);

    let (pipe, mut writer) = overlapped_read_end_pair();

    let mut reader = IocpReader::new(pipe, 32).unwrap();
    let poller = Arc::new(Poller::new().unwrap());
    reader.register(&poller, Event::readable(TOKEN), PollMode::Level).unwrap();

    writer.write_all(b"abcdef").unwrap();

    let mut events = Events::new();
    poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
    assert!(events.iter().any(|event| event.key == TOKEN));
    events.clear();

    let mut buf = [0u8; 1];
    assert_eq!(reader.read(&mut buf).unwrap(), 1);

    poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
    assert!(
        events.iter().any(|event| event.key == TOKEN),
        "expected another readable event after partial read"
    );
}

#[test]
fn reader_reposts_event_after_background_rearm_error() {
    const TOKEN: usize = 34;
    const WAIT_TIMEOUT: Duration = Duration::from_millis(250);

    let (pipe, mut writer) = overlapped_read_end_pair();

    let mut reader = IocpReader::new(pipe, 32).unwrap();
    let poller = Arc::new(Poller::new().unwrap());
    reader.register(&poller, Event::readable(TOKEN), PollMode::Level).unwrap();

    writer.write_all(b"abc").unwrap();

    let mut events = Events::new();
    poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
    assert!(events.iter().any(|event| event.key == TOKEN));
    events.clear();

    let mut buf = [0u8; 2];
    assert_eq!(reader.read(&mut buf).unwrap(), 2);
    assert_eq!(&buf, b"ab");

    poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
    assert!(
        events.iter().any(|event| event.key == TOKEN),
        "expected readable event while staged bytes remain"
    );
    events.clear();

    let error_handle = unsafe { CreateEventW(ptr::null(), 0, 0, ptr::null()) };
    assert!(!error_handle.is_null(), "failed to create error injection handle");
    let error_handle = unsafe { OwnedHandle::from_raw_handle(error_handle as _) };

    let original_pipe = mem::replace(&mut reader.state.pipe, error_handle);
    drop(original_pipe);
    drop(writer);

    let mut tail = [0u8; 2];
    assert_eq!(reader.read(&mut tail).unwrap(), 1);
    assert_eq!(&tail[..1], b"c");

    poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
    assert!(
        events.iter().any(|event| event.key == TOKEN),
        "expected readable event for deferred background re-arm error"
    );

    let err = reader.read(&mut tail).unwrap_err();
    assert_ne!(err.kind(), ErrorKind::WouldBlock);
    assert_ne!(err.kind(), ErrorKind::Interrupted);
}

#[test]
fn reader_register_rejects_non_level_mode() {
    let (pipe, _writer) = overlapped_read_end_pair();
    let mut reader = IocpReader::new(pipe, 32).unwrap();
    let poller = Arc::new(Poller::new().unwrap());

    let err = reader.register(&poller, Event::readable(35), PollMode::Oneshot).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
}

#[test]
fn writer_reposts_event_after_write() {
    const TOKEN: usize = 32;
    const WAIT_TIMEOUT: Duration = Duration::from_millis(250);

    let (pipe, mut client) = overlapped_write_end_pair();
    let mut writer = IocpWriter::new(pipe, 8).unwrap();
    let poller = Arc::new(Poller::new().unwrap());
    writer.register(&poller, Event::writable(TOKEN), PollMode::Level).unwrap();

    let mut events = Events::new();
    poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
    assert!(events.iter().any(|event| event.key == TOKEN));
    events.clear();

    assert_eq!(writer.write(b"ab").unwrap(), 2);

    let mut buf = [0u8; 2];
    client.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"ab");

    poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
    assert!(
        events.iter().any(|event| event.key == TOKEN),
        "expected another writable event after write"
    );
}

#[test]
fn writer_clears_stale_buffer_after_non_pipe_error() {
    let (pipe, mut client) = overlapped_write_end_pair();
    let mut writer = IocpWriter::new(pipe, 64).unwrap();

    let error_handle = unsafe { CreateEventW(ptr::null(), 0, 0, ptr::null()) };
    assert!(!error_handle.is_null(), "failed to create error injection handle");
    let error_handle = unsafe { OwnedHandle::from_raw_handle(error_handle as _) };

    let original_pipe = mem::replace(&mut writer.state.pipe, error_handle);
    let err = writer.write(b"stale").unwrap_err();
    assert_ne!(err.kind(), ErrorKind::BrokenPipe);
    assert_eq!(writer.write_pos, 0, "unexpected error should clear staged writer cursor");
    assert_eq!(writer.write_len, 0, "unexpected error should clear staged writer buffer");

    let error_handle = mem::replace(&mut writer.state.pipe, original_pipe);
    drop(error_handle);

    assert_eq!(writer.write(b"fresh").unwrap(), 5);

    let mut buf = [0u8; 5];
    client.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"fresh");
}

#[test]
fn reader_drop_with_pending_read() {
    let (pipe, writer) = overlapped_read_end_pair();
    let mut reader = IocpReader::new(pipe, 32).unwrap();

    let mut buf = [0u8; 1];
    let err = reader.read(&mut buf).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::WouldBlock);

    // Closing the peer guarantees completion if cancellation races.
    drop(writer);
    drop(reader);
}

#[test]
fn repeated_pending_writer_drop_does_not_hang() {
    const ITERATIONS: usize = 8;
    const CHUNK_LEN: usize = 128 * 1024;

    for _ in 0..ITERATIONS {
        let (pipe, client) = overlapped_write_end_pair_with_buffers(1);
        let mut writer = IocpWriter::new(pipe, CHUNK_LEN).unwrap();
        let chunk = vec![0u8; CHUNK_LEN];
        let mut saw_would_block = false;

        for _ in 0..8 {
            match writer.write(&chunk) {
                Ok(0) => panic!("expected WouldBlock on backpressure, got Ok(0)"),
                Ok(_) => continue,
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
                    saw_would_block = true;
                    break;
                },
                Err(err) => panic!("unexpected error: {err}"),
            }
        }

        assert!(saw_would_block, "failed to reach pending state for overlapped write before drop");

        // Closing the peer guarantees completion if cancellation races.
        drop(client);
        drop(writer);
    }
}

#[test]
fn writer_reports_broken_pipe_after_peer_close() {
    let (pipe, client) = overlapped_write_end_pair();
    let mut writer = IocpWriter::new(pipe, 64).unwrap();
    drop(client);

    let err = writer.write(b"x").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::BrokenPipe);
}

#[test]
fn pending_writer_reports_broken_pipe_after_peer_close() {
    const CHUNK_LEN: usize = 256 * 1024;
    const DEADLINE: Duration = Duration::from_millis(500);

    let (pipe, client) = overlapped_write_end_pair_with_buffers(1);
    let mut writer = IocpWriter::new(pipe, CHUNK_LEN).unwrap();
    let chunk = vec![0u8; CHUNK_LEN];

    let mut saw_would_block = false;
    for _ in 0..8 {
        match writer.write(&chunk) {
            Ok(0) => panic!("expected WouldBlock on backpressure, got Ok(0)"),
            Ok(_) => continue,
            Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
                saw_would_block = true;
                break;
            },
            Err(err) => panic!("unexpected error while queueing pending write: {err}"),
        }
    }

    assert!(saw_would_block, "failed to reach pending state before peer close");
    assert!(writer.has_pending_io(), "writer should retain pending work before peer close");

    drop(client);

    let deadline = std::time::Instant::now() + DEADLINE;
    loop {
        match writer.advance() {
            Err(err) if err.kind() == ErrorKind::BrokenPipe => break,
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "timed out waiting for pending overlapped write to resolve after peer close"
                );
                std::thread::sleep(Duration::from_millis(10));
            },
            Ok(()) => panic!("pending writer resolved without surfacing peer closure"),
            Err(err) => panic!("unexpected error while advancing pending write: {err}"),
        }
    }
}

#[test]
fn reader_returns_eof_after_peer_close_and_drain() {
    const TOKEN: usize = 33;
    const WAIT_TIMEOUT: Duration = Duration::from_millis(500);

    let (pipe, mut writer) = overlapped_read_end_pair();

    let mut reader = IocpReader::new(pipe, 32).unwrap();
    let poller = Arc::new(Poller::new().unwrap());
    reader.register(&poller, Event::readable(TOKEN), PollMode::Level).unwrap();

    // Write some data then close the writing end.
    writer.write_all(b"hello").unwrap();
    drop(writer);

    // Wait for readable event from the data or EOF.
    let mut events = Events::new();
    poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
    assert!(events.iter().any(|event| event.key == TOKEN));

    // Drain all staged data.
    let mut buf = [0u8; 32];
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"hello");

    // Subsequent reads after EOF should return Ok(0).
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => panic!("expected EOF (Ok(0)), got more data"),
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                if std::time::Instant::now() > deadline {
                    panic!("timed out waiting for EOF after peer close");
                }
                // Wait for the EOF event to arrive.
                events.clear();
                poller.wait(&mut events, Some(Duration::from_millis(50))).unwrap();
                continue;
            },
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}

#[test]
fn repeated_reader_lifecycle_preserves_data_and_eof() {
    const ITERATIONS: usize = 8;
    const WAIT_TIMEOUT: Duration = Duration::from_millis(500);

    for iteration in 0..ITERATIONS {
        let (pipe, mut writer) = overlapped_read_end_pair();
        let mut reader = IocpReader::new(pipe, 32).unwrap();
        let poller = Arc::new(Poller::new().unwrap());
        let token = 100 + iteration;
        reader.register(&poller, Event::readable(token), PollMode::Level).unwrap();

        let payload = format!("iteration-{iteration}");
        writer.write_all(payload.as_bytes()).unwrap();
        drop(writer);

        let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
        let mut events = Events::new();
        let mut collected = Vec::new();
        let mut buf = [0u8; 4];

        'read_until_eof: loop {
            events.clear();
            poller.wait(&mut events, Some(Duration::from_millis(50))).unwrap();
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for readable events during repeated reader lifecycle"
            );

            if !events.iter().any(|event| event.key == token) {
                continue;
            }

            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break 'read_until_eof,
                    Ok(n) => collected.extend_from_slice(&buf[..n]),
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(err) => panic!("unexpected read error: {err}"),
                }
            }
        }

        assert_eq!(collected, payload.as_bytes());
    }
}

#[test]
fn non_pending_reader_writer_drop_does_not_touch_drop_reclaim_stats() {
    const STABILITY_WINDOW: Duration = Duration::from_millis(100);

    let _guard = DROP_RECLAIM_TEST_LOCK.lock().unwrap();
    let baseline = drop_reclaim_stats();

    {
        let (pipe, _writer) = overlapped_read_end_pair();
        let reader = IocpReader::new(pipe, 32).unwrap();
        drop(reader);
    }

    {
        let (pipe, _client) = overlapped_write_end_pair();
        let writer = IocpWriter::new(pipe, 32).unwrap();
        drop(writer);
    }

    crate::tty::windows::wait_reclaim::assert_reclaim_stats_stable(
        baseline,
        drop_reclaim_stats,
        STABILITY_WINDOW,
        "normal IOCP lifecycle",
    );
}

#[test]
fn drop_reclaim_completes_without_leak_when_wait_event_is_signaled() {
    let _guard = DROP_RECLAIM_TEST_LOCK.lock().unwrap();
    let baseline = drop_reclaim_stats();
    let resources = leaked_resources();

    let wait_event = resources.wait_event.as_ref().unwrap().as_raw_handle() as HANDLE;
    let signaled = unsafe { SetEvent(wait_event) };
    assert_ne!(signaled, 0, "SetEvent failed: {}", Error::last_os_error());

    submit_for_reclamation_with_timeout(resources, 50);

    let stats = crate::tty::windows::wait_reclaim::wait_for_reclaim_stats_change(
        baseline,
        drop_reclaim_stats,
        |stats| stats.completed > baseline.completed,
        Duration::from_secs(2),
        "drop",
    );
    assert_eq!(stats.submitted, baseline.submitted + 1);
    assert_eq!(stats.completed, baseline.completed + 1);
    assert_eq!(stats.leaked, baseline.leaked);
}

#[test]
fn drop_reclaim_times_out_and_leaks_when_wait_event_never_signals() {
    let _guard = DROP_RECLAIM_TEST_LOCK.lock().unwrap();
    let baseline = drop_reclaim_stats();

    submit_for_reclamation_with_timeout(leaked_resources(), 1);

    let stats = crate::tty::windows::wait_reclaim::wait_for_reclaim_stats_change(
        baseline,
        drop_reclaim_stats,
        |stats| stats.leaked > baseline.leaked,
        Duration::from_secs(2),
        "drop",
    );
    assert_eq!(stats.submitted, baseline.submitted + 1);
    assert_eq!(stats.completed, baseline.completed);
    assert_eq!(stats.leaked, baseline.leaked + 1);
}

#[test]
fn drop_reclaim_without_wait_event_leaks_immediately() {
    let _guard = DROP_RECLAIM_TEST_LOCK.lock().unwrap();
    let baseline = drop_reclaim_stats();
    let mut resources = leaked_resources();
    resources.wait_event = None;

    submit_for_reclamation_with_timeout(resources, 50);

    let stats = crate::tty::windows::wait_reclaim::wait_for_reclaim_stats_change(
        baseline,
        drop_reclaim_stats,
        |stats| stats.leaked > baseline.leaked,
        Duration::from_secs(2),
        "drop",
    );
    assert_eq!(stats.submitted, baseline.submitted + 1);
    assert_eq!(stats.completed, baseline.completed);
    assert_eq!(stats.leaked, baseline.leaked + 1);
}
