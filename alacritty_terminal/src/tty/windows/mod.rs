use std::ffi::OsStr;
use std::io::{self, Result};
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, OnceLock};

use crate::event::{OnResize, WindowSize};
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite, Options, Shell};

mod blocking;
mod child;
mod conpty;

use blocking::{UnblockedReader, UnblockedWriter};
use conpty::Conpty as Backend;
use miow::pipe::{AnonRead, AnonWrite};
use polling::{Event, Poller};

pub const PTY_CHILD_EVENT_TOKEN: usize = 1;
pub const PTY_READ_WRITE_TOKEN: usize = 2;

type ReadPipe = UnblockedReader<AnonRead>;
type WritePipe = UnblockedWriter<AnonWrite>;

pub struct Pty {
    // XXX: Backend is required to be the first field, to ensure correct drop order. Dropping
    // `conout` before `backend` will cause a deadlock (with Conpty).
    backend: Backend,
    conout: ReadPipe,
    conin: WritePipe,
    child_watcher: ChildExitWatcher,
}

pub fn new(config: &Options, window_size: WindowSize, _window_id: u64) -> Result<Pty> {
    conpty::new(config, window_size)
}

impl Pty {
    fn new(
        backend: impl Into<Backend>,
        conout: impl Into<ReadPipe>,
        conin: impl Into<WritePipe>,
        child_watcher: ChildExitWatcher,
    ) -> Self {
        Self { backend: backend.into(), conout: conout.into(), conin: conin.into(), child_watcher }
    }

    pub fn child_watcher(&self) -> &ChildExitWatcher {
        &self.child_watcher
    }
}

fn with_key(mut event: Event, key: usize) -> Event {
    event.key = key;
    event
}

impl EventedReadWrite for Pty {
    type Reader = ReadPipe;
    type Writer = WritePipe;

    #[inline]
    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        interest: polling::Event,
        poll_opts: polling::PollMode,
    ) -> io::Result<()> {
        self.conin.register(poll, with_key(interest, PTY_READ_WRITE_TOKEN), poll_opts);
        self.conout.register(poll, with_key(interest, PTY_READ_WRITE_TOKEN), poll_opts);
        self.child_watcher.register(poll, with_key(interest, PTY_CHILD_EVENT_TOKEN));

        Ok(())
    }

    #[inline]
    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        interest: polling::Event,
        poll_opts: polling::PollMode,
    ) -> io::Result<()> {
        self.conin.register(poll, with_key(interest, PTY_READ_WRITE_TOKEN), poll_opts);
        self.conout.register(poll, with_key(interest, PTY_READ_WRITE_TOKEN), poll_opts);
        self.child_watcher.register(poll, with_key(interest, PTY_CHILD_EVENT_TOKEN));

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
}

impl EventedPty for Pty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        match self.child_watcher.event_rx().try_recv() {
            Ok(ev) => Some(ev),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(ChildEvent::Exited(None)),
        }
    }
}

impl OnResize for Pty {
    fn on_resize(&mut self, window_size: WindowSize) {
        self.backend.on_resize(window_size)
    }
}

fn cmdline(config: &Options) -> String {
    static DEFAULT_SHELL_PATH: OnceLock<String> = OnceLock::new();
    let default_shell = Shell::new(
        (*DEFAULT_SHELL_PATH.get_or_init(|| {
            find_pwsh_in_programfiles(false, false)
                .or_else(|| find_pwsh_in_programfiles(true, false))
                .or_else(|| find_pwsh_in_msix(false))
                .or_else(|| find_pwsh_in_programfiles(false, true))
                .or_else(|| find_pwsh_in_msix(true))
                .or_else(|| find_pwsh_in_programfiles(true, true))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or("powershell.exe".to_string())
        }))
        .clone(),
        Vec::new(),
    );
    let shell = config.shell.as_ref().unwrap_or(&default_shell);

    once(shell.program.as_str())
        .chain(shell.args.iter().map(|s| s.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Converts the string slice into a Windows-standard representation for "W"-
/// suffixed function variants, which accept UTF-16 encoded string values.
pub fn win32_string<S: AsRef<OsStr> + ?Sized>(value: &S) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

fn find_pwsh_in_programfiles(find_alternate: bool, preview: bool) -> Option<PathBuf> {
    #[cfg(target_pointer_width = "64")]
    let env_var = if find_alternate { "ProgramFiles(x86)" } else { "ProgramFiles" };

    #[cfg(target_pointer_width = "32")]
    let env_var = if find_alternate { "ProgramW6432" } else { "ProgramFiles" };

    let install_base_dir = PathBuf::from(std::env::var_os(env_var)?).join("PowerShell");
    install_base_dir
        .read_dir()
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| matches!(entry.file_type(), Ok(ft) if ft.is_dir()))
        .filter_map(|entry| {
            let dir_name = entry.file_name();
            let dir_name = dir_name.to_string_lossy();

            let version = if preview {
                let dash_index = dir_name.find('-')?;
                if &dir_name[dash_index + 1..] != "preview" {
                    return None;
                };
                dir_name[..dash_index].parse::<u32>().ok()?
            } else {
                dir_name.parse::<u32>().ok()?
            };

            let exe_path = entry.path().join("pwsh.exe");
            if exe_path.exists() {
                Some((version, exe_path))
            } else {
                None
            }
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, path)| path)
}

fn find_pwsh_in_msix(preview: bool) -> Option<PathBuf> {
    let msix_app_dir =
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Microsoft\\WindowsApps");
    if !msix_app_dir.exists() {
        return None;
    }

    let prefix = if preview { "Microsoft.PowerShellPreview_" } else { "Microsoft.PowerShell_" };
    msix_app_dir
        .read_dir()
        .ok()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !matches!(entry.file_type(), Ok(ft) if ft.is_dir()) {
                return None;
            }

            if !entry.file_name().to_string_lossy().starts_with(prefix) {
                return None;
            }

            let exe_path = entry.path().join("pwsh.exe");
            exe_path.exists().then_some(exe_path)
        })
        .next()
}
