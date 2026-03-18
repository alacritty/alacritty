use std::borrow::Cow;
use std::io::{self, ErrorKind, Read, Write};
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
#[cfg(windows)]
use std::process::ExitStatus;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
use polling::os::iocp::{CompletionPacket, PollerIocpExt};
use polling::{Event, PollMode, Poller};
#[cfg(windows)]
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::WaitForSingleObject;

use super::EventLoop;
#[cfg(windows)]
use super::Msg;
#[cfg(windows)]
use crate::event::{Event as TerminalEvent, EventListener};
use crate::event::{OnResize, VoidListener, WindowSize};
use crate::sync::FairMutex;
use crate::term::test::TermSize;
use crate::term::{Config, Term};
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite};
#[cfg(windows)]
use crate::tty::{Options, Shell};

#[cfg(windows)]
const PTY_READ_WRITE_TOKEN: usize = 2;
#[cfg(windows)]
const PTY_CHILD_EVENT_TOKEN: usize = 1;

struct MockReader;

impl Read for MockReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from(ErrorKind::WouldBlock))
    }
}

struct MockWriter {
    pending_completion: bool,
    block_once: bool,
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Write for MockWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.pending_completion {
            self.pending_completion = false;
        }

        if buf.is_empty() {
            return Ok(0);
        }

        if self.block_once {
            self.block_once = false;
            return Err(io::Error::from(ErrorKind::WouldBlock));
        }

        self.writes.lock().unwrap().push(buf.to_vec());
        self.pending_completion = true;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct MockPty {
    reader: MockReader,
    writer: MockWriter,
}

impl MockPty {
    fn new(writes: Arc<Mutex<Vec<Vec<u8>>>>, block_once: bool) -> Self {
        Self {
            reader: MockReader,
            writer: MockWriter { pending_completion: false, block_once, writes },
        }
    }
}

impl OnResize for MockPty {
    fn on_resize(&mut self, _window_size: WindowSize) {}
}

impl EventedReadWrite for MockPty {
    type Reader = MockReader;
    type Writer = MockWriter;

    unsafe fn register(
        &mut self,
        _poll: &Arc<Poller>,
        _interest: Event,
        _poll_opts: PollMode,
    ) -> io::Result<()> {
        Ok(())
    }

    fn reregister(
        &mut self,
        _poll: &Arc<Poller>,
        _interest: Event,
        _poll_opts: PollMode,
    ) -> io::Result<()> {
        Ok(())
    }

    fn deregister(&mut self, _poll: &Arc<Poller>) -> io::Result<()> {
        Ok(())
    }

    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }

    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }
}

impl EventedPty for MockPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        None
    }
}

// --- Windows-only shared mock infrastructure ---

/// Shared poller interest storage for Windows mock writers.
///
/// Handles storing and clearing the poller + event pair, and posting
/// writable completion packets. Used by multiple mock writer backends.
#[cfg(windows)]
struct MockPollerInterest {
    poller: Mutex<Option<Arc<Poller>>>,
    event: Mutex<Option<Event>>,
}

#[cfg(windows)]
impl MockPollerInterest {
    fn new() -> Self {
        Self { poller: Mutex::new(None), event: Mutex::new(None) }
    }

    fn store(&self, poller: &Arc<Poller>, mut event: Event) {
        event.key = PTY_READ_WRITE_TOKEN;
        event.readable = false;
        *self.poller.lock().unwrap() = Some(poller.clone());
        *self.event.lock().unwrap() = Some(event);
    }

    fn clear(&self) {
        *self.poller.lock().unwrap() = None;
        *self.event.lock().unwrap() = None;
    }

    fn post_writable(&self) {
        let poller = self.poller.lock().unwrap().clone();
        let event = *self.event.lock().unwrap();
        if let (Some(poller), Some(event)) = (poller, event) {
            if event.writable {
                poller.post(CompletionPacket::new(event)).ok();
            }
        }
    }
}

/// Trait for mock writers that can be plugged into `GenericMockPty`.
///
/// Provides hooks for poller interest management and optional
/// pending-I/O / child-event semantics.
#[cfg(windows)]
trait MockPtyWriter: Write {
    fn store_interest(&self, poller: &Arc<Poller>, interest: Event);
    fn clear_interest(&self);
    fn has_pending_io(&self) -> bool {
        false
    }
    fn advance(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        None
    }
}

/// Generic mock PTY that delegates all writer-specific behavior to `W`.
///
/// Eliminates the need for separate PTY struct definitions and trait
/// impls for each mock writer variant.
#[cfg(windows)]
struct GenericMockPty<W> {
    reader: MockReader,
    writer: W,
}

#[cfg(windows)]
impl<W> GenericMockPty<W> {
    fn new(writer: W) -> Self {
        Self { reader: MockReader, writer }
    }
}

#[cfg(windows)]
impl<W: MockPtyWriter> OnResize for GenericMockPty<W> {
    fn on_resize(&mut self, _window_size: WindowSize) {}
}

#[cfg(windows)]
impl<W: MockPtyWriter> EventedReadWrite for GenericMockPty<W> {
    type Reader = MockReader;
    type Writer = W;

    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        interest: Event,
        _poll_opts: PollMode,
    ) -> io::Result<()> {
        self.writer.store_interest(poll, interest);
        Ok(())
    }

    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        interest: Event,
        _poll_opts: PollMode,
    ) -> io::Result<()> {
        self.writer.store_interest(poll, interest);
        Ok(())
    }

    fn deregister(&mut self, _poll: &Arc<Poller>) -> io::Result<()> {
        self.writer.clear_interest();
        Ok(())
    }

    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }

    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }

    fn writer_has_pending_io(&self) -> bool {
        self.writer.has_pending_io()
    }

    fn advance_writer(&mut self) -> io::Result<()> {
        self.writer.advance()
    }
}

#[cfg(windows)]
impl<W: MockPtyWriter> EventedPty for GenericMockPty<W> {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.writer.next_child_event()
    }
}

// --- Async chunked writer (accepts partial writes, re-arms after delay) ---

#[cfg(windows)]
struct AsyncWritableState {
    interest: MockPollerInterest,
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
    ready: AtomicBool,
    wake_delay: Duration,
}

#[cfg(windows)]
impl AsyncWritableState {
    fn new(writes: Arc<Mutex<Vec<Vec<u8>>>>, wake_delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            interest: MockPollerInterest::new(),
            writes,
            ready: AtomicBool::new(true),
            wake_delay,
        })
    }

    fn post_writable_if_ready(&self) {
        if self.ready.load(Ordering::Acquire) {
            self.interest.post_writable();
        }
    }

    fn schedule_writable_rearm(self: &Arc<Self>) {
        let state = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(state.wake_delay);
            state.ready.store(true, Ordering::Release);
            state.post_writable_if_ready();
        });
    }
}

#[cfg(windows)]
struct AsyncMockWriter {
    state: Arc<AsyncWritableState>,
    chunk_size: usize,
}

#[cfg(windows)]
impl AsyncMockWriter {
    fn new(writes: Arc<Mutex<Vec<Vec<u8>>>>, chunk_size: usize, wake_delay: Duration) -> Self {
        Self { state: AsyncWritableState::new(writes, wake_delay), chunk_size }
    }
}

#[cfg(windows)]
impl Write for AsyncMockWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        if !self.state.ready.swap(false, Ordering::AcqRel) {
            return Err(io::Error::from(ErrorKind::WouldBlock));
        }

        let accepted = buf.len().min(self.chunk_size);
        self.state.writes.lock().unwrap().push(buf[..accepted].to_vec());

        if accepted < buf.len() {
            self.state.schedule_writable_rearm();
        } else {
            self.state.ready.store(true, Ordering::Release);
        }

        Ok(accepted)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
impl MockPtyWriter for AsyncMockWriter {
    fn store_interest(&self, poller: &Arc<Poller>, interest: Event) {
        self.state.interest.store(poller, interest);
        self.state.post_writable_if_ready();
    }

    fn clear_interest(&self) {
        self.state.interest.clear();
    }
}

// --- Hidden-pending writer (models advance_writer / writer_has_pending_io) ---

#[cfg(windows)]
struct HiddenPendingWritableState {
    interest: MockPollerInterest,
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
    busy: AtomicBool,
    ready: AtomicBool,
    wake_delay: Duration,
}

#[cfg(windows)]
impl HiddenPendingWritableState {
    fn new(writes: Arc<Mutex<Vec<Vec<u8>>>>, wake_delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            interest: MockPollerInterest::new(),
            writes,
            busy: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            wake_delay,
        })
    }

    fn schedule_completion(self: &Arc<Self>) {
        let state = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(state.wake_delay);
            state.ready.store(true, Ordering::Release);

            if state.busy.load(Ordering::Acquire) {
                state.interest.post_writable();
            }
        });
    }
}

#[cfg(windows)]
struct HiddenPendingMockWriter {
    state: Arc<HiddenPendingWritableState>,
}

#[cfg(windows)]
impl HiddenPendingMockWriter {
    fn new(writes: Arc<Mutex<Vec<Vec<u8>>>>, wake_delay: Duration) -> Self {
        Self { state: HiddenPendingWritableState::new(writes, wake_delay) }
    }
}

#[cfg(windows)]
impl Write for HiddenPendingMockWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
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

        if buf.is_empty() {
            return Ok(0);
        }

        self.state.writes.lock().unwrap().push(buf.to_vec());
        self.state.busy.store(true, Ordering::Release);
        self.state.ready.store(false, Ordering::Release);
        self.state.schedule_completion();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
impl MockPtyWriter for HiddenPendingMockWriter {
    fn store_interest(&self, poller: &Arc<Poller>, interest: Event) {
        self.state.interest.store(poller, interest);
        let event = *self.state.interest.event.lock().unwrap();
        if let Some(event) = event {
            if event.writable && !self.state.busy.load(Ordering::Acquire) {
                poller.post(CompletionPacket::new(event)).ok();
            }
        }
    }

    fn clear_interest(&self) {
        self.state.interest.clear();
    }

    fn has_pending_io(&self) -> bool {
        self.state.busy.load(Ordering::Acquire)
    }

    fn advance(&mut self) -> io::Result<()> {
        if !self.state.busy.load(Ordering::Acquire) {
            return Ok(());
        }

        if !self.state.ready.swap(false, Ordering::AcqRel) {
            return Err(io::Error::from(ErrorKind::WouldBlock));
        }

        self.state.busy.store(false, Ordering::Release);
        Ok(())
    }
}

// --- Broken-pipe writer with child exit support ---

#[cfg(windows)]
#[derive(Clone, Default)]
struct RecordingListener {
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[cfg(windows)]
impl RecordingListener {
    fn snapshot(&self) -> Vec<&'static str> {
        self.events.lock().unwrap().clone()
    }
}

#[cfg(windows)]
impl EventListener for RecordingListener {
    fn send_event(&self, event: TerminalEvent) {
        let label = match event {
            TerminalEvent::ChildExit(_) => "ChildExit",
            TerminalEvent::Exit => "Exit",
            TerminalEvent::Wakeup => "Wakeup",
            _ => return,
        };
        self.events.lock().unwrap().push(label);
    }
}

#[cfg(windows)]
struct BrokenPipeThenChildState {
    rw_interest: Mutex<Option<(Arc<Poller>, Event)>>,
    child_interest: Mutex<Option<(Arc<Poller>, Event)>>,
    child_event: Mutex<Option<ChildEvent>>,
    write_attempts: AtomicUsize,
}

#[cfg(windows)]
impl BrokenPipeThenChildState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            rw_interest: Mutex::new(None),
            child_interest: Mutex::new(None),
            child_event: Mutex::new(None),
            write_attempts: AtomicUsize::new(0),
        })
    }

    fn register_interest(&self, poller: &Arc<Poller>, mut interest: Event) {
        let mut rw_event = interest;
        rw_event.key = PTY_READ_WRITE_TOKEN;
        rw_event.readable = false;
        *self.rw_interest.lock().unwrap() = Some((poller.clone(), rw_event));
        if rw_event.writable {
            poller.post(CompletionPacket::new(rw_event)).ok();
        }

        interest.key = PTY_CHILD_EVENT_TOKEN;
        interest.writable = false;
        *self.child_interest.lock().unwrap() = Some((poller.clone(), interest));
        if interest.readable && self.child_event.lock().unwrap().is_some() {
            poller.post(CompletionPacket::new(interest)).ok();
        }
    }

    fn clear_interest(&self) {
        *self.rw_interest.lock().unwrap() = None;
        *self.child_interest.lock().unwrap() = None;
    }

    fn note_write_attempt(&self) {
        self.write_attempts.fetch_add(1, Ordering::AcqRel);
    }

    fn write_attempts(&self) -> usize {
        self.write_attempts.load(Ordering::Acquire)
    }

    fn trigger_child_exit(&self) {
        *self.child_event.lock().unwrap() = Some(ChildEvent::Exited(Some(ExitStatus::from_raw(0))));

        let child_interest = self.child_interest.lock().unwrap().clone();
        if let Some((poller, event)) = child_interest {
            if event.readable {
                poller.post(CompletionPacket::new(event)).ok();
            }
        }
    }
}

#[cfg(windows)]
struct BrokenPipeMockWriter {
    state: Arc<BrokenPipeThenChildState>,
}

#[cfg(windows)]
impl BrokenPipeMockWriter {
    fn new(state: Arc<BrokenPipeThenChildState>) -> Self {
        Self { state }
    }
}

#[cfg(windows)]
impl Write for BrokenPipeMockWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        self.state.note_write_attempt();
        Err(io::Error::from(ErrorKind::BrokenPipe))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
impl MockPtyWriter for BrokenPipeMockWriter {
    fn store_interest(&self, poller: &Arc<Poller>, interest: Event) {
        self.state.register_interest(poller, interest);
    }

    fn clear_interest(&self) {
        self.state.clear_interest();
    }

    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.state.child_event.lock().unwrap().take()
    }
}

#[test]
fn pty_write_preserves_pending_write_on_would_block() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let pty = MockPty::new(writes.clone(), true);

    let terminal_size = TermSize::new(1, 1);
    let terminal =
        Arc::new(FairMutex::new(Term::new(Config::default(), &terminal_size, VoidListener)));
    let mut event_loop = EventLoop::new(terminal, VoidListener, pty, false, false).unwrap();

    let mut state = super::State::default();
    state.write_list.push_back(Cow::Borrowed(b"payload"));
    assert!(state.needs_write());

    event_loop.pty_write(&mut state).unwrap();
    assert!(state.needs_write(), "payload should stay queued after WouldBlock");
    assert!(writes.lock().unwrap().is_empty(), "payload must not be partially consumed");

    event_loop.pty_write(&mut state).unwrap();

    assert!(!state.needs_write());
    assert_eq!(*writes.lock().unwrap(), vec![b"payload".to_vec()]);
}

#[cfg(windows)]
#[test]
fn event_loop_waits_for_child_exit_after_broken_pipe() {
    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    let state = BrokenPipeThenChildState::new();
    let pty = GenericMockPty::new(BrokenPipeMockWriter::new(state.clone()));
    let listener = RecordingListener::default();

    let terminal_size = TermSize::new(1, 1);
    let terminal =
        Arc::new(FairMutex::new(Term::new(Config::default(), &terminal_size, listener.clone())));
    let event_loop = EventLoop::new(terminal, listener.clone(), pty, false, false).unwrap();
    let sender = event_loop.channel();
    let handle = event_loop.spawn();

    sender.send(Msg::Input(Cow::Owned(b"first".to_vec()))).unwrap();

    let broken_pipe_deadline = Instant::now() + WAIT_TIMEOUT;
    while state.write_attempts() == 0 {
        assert!(
            Instant::now() < broken_pipe_deadline,
            "timed out waiting for broken-pipe write attempt"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    // Any later input should be ignored after the PTY write side is closed.
    sender.send(Msg::Input(Cow::Owned(b"second".to_vec()))).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(state.write_attempts(), 1, "writes should stop after BrokenPipe");

    state.trigger_child_exit();

    let (_event_loop, returned_state) =
        handle.join().expect("event loop should keep running until child exit arrives");

    assert!(!returned_state.needs_write(), "closed writer should drop queued input");
    assert_eq!(listener.snapshot(), vec!["ChildExit", "Exit", "Wakeup"]);
}

#[cfg(windows)]
#[test]
fn event_loop_retries_chunked_writes_after_writable_notifications() {
    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
    const WAKE_DELAY: Duration = Duration::from_millis(10);

    let writes = Arc::new(Mutex::new(Vec::new()));
    let pty = GenericMockPty::new(AsyncMockWriter::new(writes.clone(), 3, WAKE_DELAY));

    let terminal_size = TermSize::new(1, 1);
    let terminal =
        Arc::new(FairMutex::new(Term::new(Config::default(), &terminal_size, VoidListener)));
    let event_loop = EventLoop::new(terminal, VoidListener, pty, false, false).unwrap();
    let sender = event_loop.channel();
    let handle = event_loop.spawn();

    let payload = b"abcdefghij".to_vec();
    sender.send(Msg::Input(Cow::Owned(payload.clone()))).unwrap();

    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let flattened: Vec<u8> = writes.lock().unwrap().iter().flatten().copied().collect();
        if flattened == payload {
            break;
        }

        assert!(Instant::now() < deadline, "timed out waiting for chunked PTY write completion");
        std::thread::sleep(Duration::from_millis(10));
    }

    sender.send(Msg::Shutdown).unwrap();
    let (_event_loop, state) = handle.join().unwrap();

    assert!(!state.needs_write(), "event loop should drain queued PTY writes");
    assert_eq!(*writes.lock().unwrap(), vec![
        b"abc".to_vec(),
        b"def".to_vec(),
        b"ghi".to_vec(),
        b"j".to_vec()
    ]);
}

#[cfg(windows)]
#[test]
fn event_loop_drains_hidden_pending_write_before_future_input() {
    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
    const WAKE_DELAY: Duration = Duration::from_millis(10);

    let writes = Arc::new(Mutex::new(Vec::new()));
    let pty = GenericMockPty::new(HiddenPendingMockWriter::new(writes.clone(), WAKE_DELAY));

    let terminal_size = TermSize::new(1, 1);
    let terminal =
        Arc::new(FairMutex::new(Term::new(Config::default(), &terminal_size, VoidListener)));
    let event_loop = EventLoop::new(terminal, VoidListener, pty, false, false).unwrap();
    let sender = event_loop.channel();
    let handle = event_loop.spawn();

    let first = b"first".to_vec();
    sender.send(Msg::Input(Cow::Owned(first.clone()))).unwrap();

    let first_deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        if *writes.lock().unwrap() == vec![first.clone()] {
            break;
        }

        assert!(Instant::now() < first_deadline, "timed out waiting for first PTY write");
        std::thread::sleep(Duration::from_millis(10));
    }

    std::thread::sleep(WAKE_DELAY + WAKE_DELAY);

    let second = b"second".to_vec();
    let expected = [first.clone(), second.clone()].concat();
    sender.send(Msg::Input(Cow::Owned(second))).unwrap();

    let second_deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let flattened: Vec<u8> = writes.lock().unwrap().iter().flatten().copied().collect();
        if flattened == expected {
            break;
        }

        assert!(
            Instant::now() < second_deadline,
            "timed out waiting for hidden pending PTY write completion"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    sender.send(Msg::Shutdown).unwrap();
    let (_event_loop, state) = handle.join().unwrap();

    assert!(!state.needs_write(), "event loop should drain queued PTY writes");
    assert_eq!(*writes.lock().unwrap(), vec![first, b"second".to_vec()]);
}

#[cfg(windows)]
#[test]
fn event_loop_late_start_with_real_conpty_preserves_output_on_exit() {
    const CHILD_EXIT_TIMEOUT_MS: u32 = 5_000;
    const CALLBACK_SETTLE_DELAY: Duration = Duration::from_millis(50);
    const EVENT_LOOP_TIMEOUT: Duration = Duration::from_secs(10);

    let marker = "__ALACRITTY_EVENT_LOOP_DRAIN_ON_EXIT__";
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

    let window_size = WindowSize { num_lines: 24, num_cols: 80, cell_width: 8, cell_height: 16 };
    let pty = crate::tty::windows::new(&options, window_size, 0).unwrap();

    let wait_result =
        unsafe { WaitForSingleObject(pty.child_watcher().raw_handle(), CHILD_EXIT_TIMEOUT_MS) };
    assert_eq!(
        wait_result, WAIT_OBJECT_0,
        "timed out waiting for ConPTY child to exit before starting the event loop"
    );

    // Give the child-exit callback a moment to publish its pending event before
    // the event loop registers interest, so this exercises the late-register
    // path with the real ConPTY/IOCP backend.
    std::thread::sleep(CALLBACK_SETTLE_DELAY);

    let listener = RecordingListener::default();
    let terminal_size =
        TermSize::new(window_size.num_cols as usize, window_size.num_lines as usize);
    let terminal =
        Arc::new(FairMutex::new(Term::new(Config::default(), &terminal_size, listener.clone())));
    let event_loop = EventLoop::new(terminal.clone(), listener.clone(), pty, true, false).unwrap();
    let sender = event_loop.channel();
    let handle = event_loop.spawn();

    let deadline = Instant::now() + EVENT_LOOP_TIMEOUT;
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }

    if !handle.is_finished() {
        let _ = sender.send(Msg::Shutdown);
        let _ = handle.join();
        panic!("timed out waiting for real ConPTY event loop to process child exit");
    }

    let (_event_loop, state) = handle.join().unwrap();
    assert!(!state.needs_write(), "event loop should not retain queued PTY writes");

    let start = crate::index::Point::new(crate::index::Line(0), crate::index::Column(0));
    let end = crate::index::Point::new(
        crate::index::Line(terminal_size.screen_lines as i32 - 1),
        crate::index::Column(terminal_size.columns - 1),
    );
    let screen = terminal.lock().bounds_to_string(start, end);
    assert!(
        screen.contains(marker),
        "drain_on_exit should preserve output produced before the event loop starts: {screen:?}"
    );

    let events = listener.snapshot();
    assert!(events.contains(&"ChildExit"), "expected ChildExit event, got {events:?}");
    assert!(events.contains(&"Exit"), "expected Exit event, got {events:?}");
}
