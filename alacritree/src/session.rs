use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use alacritty_terminal::event::{Event as TermEvent, EventListener, Notify, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::tty::{self, Options as PtyOptions, Shell};

use crate::config::Config;

#[derive(Clone)]
pub struct EventProxy {
    ctx: egui::Context,
    sender: mpsc::Sender<TermEvent>,
}

impl EventProxy {
    pub fn new(ctx: egui::Context) -> (Self, mpsc::Receiver<TermEvent>) {
        let (sender, receiver) = mpsc::channel();
        (Self { ctx, sender }, receiver)
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: TermEvent) {
        let _ = self.sender.send(event);
        self.ctx.request_repaint();
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TermSize {
    pub columns: usize,
    pub screen_lines: usize,
}

impl TermSize {
    pub fn new(columns: usize, screen_lines: usize) -> Self {
        Self { columns: columns.max(1), screen_lines: screen_lines.max(1) }
    }
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

pub type SessionId = u64;

/// PTY child + parsed terminal state.  The read/write loop is on its own
/// thread and survives workspace switches, so running processes aren't killed.
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub working_directory: Option<PathBuf>,
    pub size: TermSize,
    pub cell_size: (f32, f32),
    pub term: Arc<FairMutex<Term<EventProxy>>>,
    pub events: mpsc::Receiver<TermEvent>,
    /// Session asked for the user's attention (BEL, or title transitioning
    /// out of a working spinner) while they weren't looking.  Surfaced as a
    /// sidebar indicator until they switch to or refocus this session.
    pub needs_attention: bool,
    notifier: Notifier,
    sender: EventLoopSender,
    exited: bool,
}

/// Outcome of draining one session's pending PTY events for a single frame.
#[derive(Default)]
pub struct DrainOutcome {
    /// Something this session emitted suggests it wants the user's attention.
    /// Sources:
    /// - `Event::Bell` (the universal signal — any CLI ringing BEL).
    /// - Title transitioning out of a braille-spinner state (the pattern
    ///   Claude Code uses to indicate "done thinking" since it doesn't ring
    ///   BEL).  Caller still decides whether the session is currently
    ///   visible+focused (in which case the signal is suppressed).
    pub attention: bool,
}

/// Heuristic for "this title looks like a working/spinner state".  Matches
/// any title containing a Braille glyph (`U+2800..=U+28FF`), which is the
/// near-universal spinner alphabet (Claude Code, oh-my-posh, ollama, cargo's
/// progress indicator, etc.).
fn is_spinner_title(title: &str) -> bool {
    title.chars().any(|c| {
        let n = c as u32;
        (0x2800..=0x28FF).contains(&n)
    })
}

impl Session {
    pub fn spawn(
        ctx: egui::Context,
        config: &Config,
        working_directory: Option<PathBuf>,
        size: TermSize,
        cell_size: (f32, f32),
    ) -> std::io::Result<Self> {
        let window_size = window_size(size, cell_size);

        let (proxy, events) = EventProxy::new(ctx);

        let term_config = TermConfig {
            scrolling_history: config.scrolling.history,
            default_cursor_style: config.cursor_style(),
            semantic_escape_chars: config.selection.semantic_escape_chars.clone(),
            ..TermConfig::default()
        };
        let term = Term::new(term_config, &size, proxy.clone());
        let term = Arc::new(FairMutex::new(term));

        let pty_options = PtyOptions {
            shell: config.shell.as_ref().map(|s| Shell::new(s.program.clone(), s.args.clone())),
            working_directory: working_directory.clone(),
            drain_on_exit: false,
            env: config.env.clone(),
            // `Options` has a Windows-only `escape_args` field; leaning on
            // `Default` keeps the literal compilable on every target.
            ..Default::default()
        };

        // alacritty routes OSC 7 / signals by this id, so each session needs its own.
        let window_id = next_window_id();
        let pty = tty::new(&pty_options, window_size, window_id)?;

        let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false)?;
        let sender = event_loop.channel();
        event_loop.spawn();

        let title = working_directory
            .as_ref()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "shell".to_string());

        Ok(Self {
            id: next_session_id(),
            title,
            working_directory,
            size,
            cell_size,
            term,
            events,
            needs_attention: false,
            notifier: Notifier(sender.clone()),
            sender,
            exited: false,
        })
    }

    pub fn write(&self, bytes: Vec<u8>) {
        self.notifier.notify(bytes);
    }

    /// Pull every pending event out of the PTY channel.  Called once per frame
    /// for every session — including background ones — so bells, title
    /// changes, and child-exits from non-visible sessions don't pile up.
    pub fn drain_events(&mut self) -> DrainOutcome {
        let mut outcome = DrainOutcome::default();
        while let Ok(event) = self.events.try_recv() {
            match event {
                TermEvent::PtyWrite(s) => self.write(s.into_bytes()),
                TermEvent::Title(t) => {
                    // A spinner-shaped title transitioning to a non-spinner one
                    // is how Claude Code (and similar tools that don't ring
                    // BEL) signal "done — your turn".  Treat it like a bell.
                    if is_spinner_title(&self.title) && !is_spinner_title(&t) {
                        outcome.attention = true;
                    }
                    self.title = t;
                },
                TermEvent::ChildExit(_) => self.exited = true,
                TermEvent::Bell => outcome.attention = true,
                _ => {},
            }
        }
        outcome
    }

    pub fn resize(&mut self, size: TermSize, cell_size: (f32, f32)) {
        if size.columns == self.size.columns
            && size.screen_lines == self.size.screen_lines
            && cell_size == self.cell_size
        {
            return;
        }
        self.size = size;
        self.cell_size = cell_size;
        let ws = window_size(size, cell_size);
        let _ = self.sender.send(Msg::Resize(ws));
        self.term.lock().resize(size);
    }

    pub fn is_exited(&self) -> bool {
        self.exited
    }

    pub fn shutdown(&self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn window_size(size: TermSize, cell_size: (f32, f32)) -> WindowSize {
    WindowSize {
        num_lines: size.screen_lines as u16,
        num_cols: size.columns as u16,
        cell_width: cell_size.0.max(1.0) as u16,
        cell_height: cell_size.1.max(1.0) as u16,
    }
}

fn next_window_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn next_session_id() -> SessionId {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
