//! Windows PTY backend wiring.
//!
//! Lifecycle overview:
//!
//! - `conpty::new` creates the named-pipe endpoints, starts the pseudoconsole, and wraps the local
//!   pipe ends in the IOCP-backed reader/writer types.
//! - `child::ChildExitWatcher` owns a duplicated process handle and converts process exit into a
//!   readable poll event.
//! - `Pty::drop` tears the pieces down in a strict order: pseudoconsole first, then the
//!   conout/conin pipes, then the child watcher, and finally the spawned-process guard.
//!
//! That teardown ordering matters because `ClosePseudoConsole` may keep
//! draining output through conout while closing, and the IOCP layer may defer
//! final buffer and `OVERLAPPED` reclamation to background cleanup if a
//! cancelled operation does not retire immediately. The spawned-process guard
//! stays last so forced child termination only happens after the pseudoconsole,
//! pipe I/O, and exit-event wiring are already being torn down.
use log::warn;
use std::ffi::OsStr;
use std::io::{self, ErrorKind, Result};
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::sync::Arc;

use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};

use crate::event::{OnResize, WindowSize};
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite, Options, Shell};

mod child;
mod conpty;
mod iocp;
pub(crate) mod wait_reclaim;

use conpty::Conpty as Backend;
use iocp::{IocpReader, IocpWriter};
use polling::{Event, Poller};

pub const PTY_CHILD_EVENT_TOKEN: usize = 1;
pub const PTY_READ_WRITE_TOKEN: usize = 2;

const CHILD_CLEANUP_TIMEOUT_MS: u32 = 2_000;

pub struct Pty {
    // `backend` must be dropped first: `ClosePseudoConsole` drains remaining
    // output through the conout pipe, so `conout` must still be alive to
    // accept those writes. Reversing this order can deadlock when ConPTY
    // is mid-write into a full pipe buffer whose reader is already gone.
    backend: Backend,
    conout: IocpReader,
    conin: IocpWriter,
    child_watcher: ChildExitWatcher,
    _child_process: SpawnedProcessGuard,
    read_interest: bool,
    write_interest: bool,
}

pub(super) struct SpawnedProcessGuard {
    process: Option<OwnedHandle>,
}

impl SpawnedProcessGuard {
    pub fn new(process: OwnedHandle) -> Self {
        Self { process: Some(process) }
    }

    pub fn raw_handle(&self) -> HANDLE {
        self.process.as_ref().unwrap().as_raw_handle() as HANDLE
    }
}

impl Drop for SpawnedProcessGuard {
    fn drop(&mut self) {
        let Some(process) = self.process.as_ref() else {
            return;
        };

        let handle = process.as_raw_handle() as HANDLE;

        match unsafe { WaitForSingleObject(handle, 0) } {
            WAIT_OBJECT_0 => return,
            WAIT_TIMEOUT => (),
            other => {
                let err = io::Error::last_os_error();
                warn!(
                    "WaitForSingleObject returned unexpected value {other} while checking child \
                     cleanup (last error: {err}); attempting termination"
                );
            },
        }

        if unsafe { TerminateProcess(handle, 1) } == 0 {
            let err = io::Error::last_os_error();
            if unsafe { WaitForSingleObject(handle, 0) } != WAIT_OBJECT_0 {
                warn!("Failed to terminate child process: {err}");
                return;
            }
        }

        match unsafe { WaitForSingleObject(handle, CHILD_CLEANUP_TIMEOUT_MS) } {
            WAIT_OBJECT_0 => (),
            WAIT_TIMEOUT => {
                warn!("Timed out waiting {}ms for child cleanup", CHILD_CLEANUP_TIMEOUT_MS)
            },
            other => {
                let err = io::Error::last_os_error();
                warn!(
                    "WaitForSingleObject returned unexpected value {other} while waiting for \
                     child cleanup (last error: {err})"
                );
            },
        }
    }
}

pub fn new(config: &Options, window_size: WindowSize, _window_id: u64) -> Result<Pty> {
    conpty::new(config, window_size)
}

impl Pty {
    fn new(
        backend: impl Into<Backend>,
        conout: impl Into<IocpReader>,
        conin: impl Into<IocpWriter>,
        child_watcher: ChildExitWatcher,
        child_process: SpawnedProcessGuard,
    ) -> Self {
        Self {
            backend: backend.into(),
            conout: conout.into(),
            conin: conin.into(),
            child_watcher,
            _child_process: child_process,
            read_interest: false,
            write_interest: false,
        }
    }

    pub fn child_watcher(&self) -> &ChildExitWatcher {
        &self.child_watcher
    }
}

fn read_event(mut event: Event, key: usize) -> Event {
    event.key = key;
    event.writable = false;
    event
}

fn write_event(mut event: Event, key: usize) -> Event {
    event.key = key;
    event.readable = false;
    event
}

fn child_event(mut event: Event, key: usize) -> Event {
    event.key = key;
    event.readable = true;
    event.writable = false;
    event
}

fn validate_readable_interest(interest: Event) -> io::Result<()> {
    if interest.readable {
        Ok(())
    } else {
        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Windows PTY child-exit notifications require readable interest",
        ))
    }
}

impl EventedReadWrite for Pty {
    type Reader = IocpReader;
    type Writer = IocpWriter;

    #[inline]
    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        interest: polling::Event,
        poll_opts: polling::PollMode,
    ) -> io::Result<()> {
        validate_readable_interest(interest)?;
        self.conin.register(poll, write_event(interest, PTY_READ_WRITE_TOKEN), poll_opts)?;
        self.conout.register(poll, read_event(interest, PTY_READ_WRITE_TOKEN), poll_opts)?;
        self.child_watcher.register(poll, child_event(interest, PTY_CHILD_EVENT_TOKEN));
        self.read_interest = interest.readable;
        self.write_interest = interest.writable;

        Ok(())
    }

    #[inline]
    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        interest: polling::Event,
        poll_opts: polling::PollMode,
    ) -> io::Result<()> {
        validate_readable_interest(interest)?;

        if self.write_interest != interest.writable {
            self.conin.register(poll, write_event(interest, PTY_READ_WRITE_TOKEN), poll_opts)?;
            self.write_interest = interest.writable;
        }

        if self.read_interest != interest.readable {
            self.conout.register(poll, read_event(interest, PTY_READ_WRITE_TOKEN), poll_opts)?;
            self.read_interest = interest.readable;
        }

        // The child watcher interest is always (readable=true, writable=false)
        // with a fixed key, set once in `register`. No refresh needed here.

        Ok(())
    }

    #[inline]
    fn deregister(&mut self, _poll: &Arc<Poller>) -> io::Result<()> {
        self.conin.deregister();
        self.conout.deregister();
        self.child_watcher.deregister();

        Ok(())
    }

    #[inline]
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.conout
    }

    #[inline]
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.conin
    }

    #[inline]
    fn writer_has_pending_io(&self) -> bool {
        self.conin.has_pending_io()
    }

    #[inline]
    fn advance_writer(&mut self) -> io::Result<()> {
        self.conin.advance()
    }
}

impl EventedPty for Pty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.child_watcher.next_event()
    }
}

impl OnResize for Pty {
    fn on_resize(&mut self, window_size: WindowSize) {
        self.backend.on_resize(window_size)
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use polling::Event;

    use super::{PTY_CHILD_EVENT_TOKEN, child_event, validate_readable_interest};

    #[test]
    fn child_watcher_interest_requires_readable_registration() {
        let err = validate_readable_interest(Event::writable(0)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn child_watcher_interest_accepts_readable_registration() {
        validate_readable_interest(Event::readable(0)).unwrap();
    }

    #[test]
    fn child_watcher_event_is_always_readable_only() {
        let event = child_event(Event::writable(42), PTY_CHILD_EVENT_TOKEN);
        assert_eq!(event.key, PTY_CHILD_EVENT_TOKEN);
        assert!(event.readable);
        assert!(!event.writable);
    }
}

// Modified per stdlib implementation.
// https://github.com/rust-lang/rust/blob/6707bf0f59485cf054ac1095725df43220e4be20/library/std/src/sys/args/windows.rs#L174
fn push_escaped_arg(cmd: &mut String, arg: &str) {
    let arg_bytes = arg.as_bytes();
    let quote = arg_bytes.iter().any(|c| *c == b' ' || *c == b'\t') || arg_bytes.is_empty();
    if quote {
        cmd.push('"');
    }

    let mut backslashes: usize = 0;
    for x in arg.chars() {
        if x == '\\' {
            backslashes += 1;
        } else {
            if x == '"' {
                // Add n+1 backslashes to total 2n+1 before internal '"'.
                cmd.extend((0..=backslashes).map(|_| '\\'));
            }
            backslashes = 0;
        }
        cmd.push(x);
    }

    if quote {
        // Add n backslashes to total 2n before ending '"'.
        cmd.extend((0..backslashes).map(|_| '\\'));
        cmd.push('"');
    }
}

fn cmdline(config: &Options) -> String {
    let default_shell = Shell::new("powershell".to_owned(), Vec::new());
    let shell = config.shell.as_ref().unwrap_or(&default_shell);

    let mut cmd = String::new();
    cmd.push_str(&shell.program);

    for arg in &shell.args {
        cmd.push(' ');
        if config.escape_args {
            push_escaped_arg(&mut cmd, arg);
        } else {
            cmd.push_str(arg)
        }
    }
    cmd
}

/// Converts the string slice into a Windows-standard representation for "W"-
/// suffixed function variants, which accept UTF-16 encoded string values.
pub fn win32_string<S: AsRef<OsStr> + ?Sized>(value: &S) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

#[cfg(test)]
mod test {
    use crate::tty::windows::{cmdline, push_escaped_arg};
    use crate::tty::{Options, Shell};

    #[test]
    fn test_escape() {
        let test_set = vec![
            // Basic cases - no escaping needed
            ("abc", "abc"),
            // Cases requiring quotes (space/tab)
            ("", "\"\""),
            (" ", "\" \""),
            ("ab c", "\"ab c\""),
            ("ab\tc", "\"ab\tc\""),
            // Cases with backslashes only (no spaces, no quotes) - no quotes added
            ("ab\\c", "ab\\c"),
            // Cases with quotes only (no spaces) - quotes escaped but no outer quotes
            ("ab\"c", "ab\\\"c"),
            ("\"", "\\\""),
            ("a\"b\"c", "a\\\"b\\\"c"),
            // Cases requiring both quotes and escaping (contains spaces)
            ("ab \"c", "\"ab \\\"c\""),
            ("a \"b\" c", "\"a \\\"b\\\" c\""),
            // Complex real-world cases
            ("C:\\Program Files\\", "\"C:\\Program Files\\\\\""),
            ("C:\\Program Files\\a.txt", "\"C:\\Program Files\\a.txt\""),
            (
                r#"sh -c "cd /home/user; ARG='abc' \""'${SHELL:-sh}" -i -c '"'echo hello'""#,
                r#""sh -c \"cd /home/user; ARG='abc' \\\"\"'${SHELL:-sh}\" -i -c '\"'echo hello'\"""#,
            ),
        ];

        for (input, expected) in test_set {
            let mut escaped_arg = String::new();
            push_escaped_arg(&mut escaped_arg, input);
            assert_eq!(escaped_arg, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_cmdline() {
        let mut options = Options {
            shell: Some(Shell {
                program: "echo".to_string(),
                args: vec!["hello world".to_string()],
            }),
            working_directory: None,
            drain_on_exit: true,
            env: Default::default(),
            escape_args: false,
        };
        assert_eq!(cmdline(&options), "echo hello world");

        options.escape_args = true;
        assert_eq!(cmdline(&options), "echo \"hello world\"");
    }
}
