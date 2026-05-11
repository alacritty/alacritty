use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
    /// Latched attention flag, cleared when the user views this session.
    pub needs_attention: bool,
    /// Shell pid spawned for this PTY.  Used to walk to the foreground
    /// process group when identifying which agent is running.  None on
    /// platforms where we don't yet capture it (Windows).
    shell_pid: Option<u32>,
    /// Cached result of the last foreground-process probe — refreshed on a
    /// timer instead of polling `/proc` every frame.  `Cell` is enough since
    /// `Session` isn't `Sync` and the values are `Copy`.
    agent_cache: Cell<AgentCache>,
    notifier: Notifier,
    sender: EventLoopSender,
    exited: bool,
}

#[derive(Clone, Copy, Default)]
struct AgentCache {
    polled_at: Option<Instant>,
    /// Static glyph for the foreground process if it's a recognized agent.
    process_glyph: Option<char>,
}

const AGENT_CACHE_TTL: Duration = Duration::from_millis(1000);

/// Map a foreground process name (from `/proc/<pid>/comm`) to its static
/// sidebar glyph.  `comm` is kernel-truncated to 15 bytes, so we compare with
/// `starts_with` — `cursor-agent` would otherwise miss.
const AGENT_PROCESS_GLYPHS: &[(&str, char)] = &[
    ("claude", '✳'),
    ("codex", '◇'),
    ("gemini", '✦'),
    ("aider", '▲'),
    ("cursor-agent", '❖'),
    ("continue", '⊕'),
];

#[derive(Default)]
pub struct DrainOutcome {
    /// Set if any event in this batch warrants flagging the session: BEL, or
    /// a title transitioning out of a spinner state.
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

/// `<glyph> <text>` titles are the universal agent-CLI shape: a non-ASCII
/// leading glyph followed by whitespace.  Plain titles (`~/foo`, `bash`)
/// fail both checks.
fn title_decorative_glyph(title: &str) -> Option<char> {
    let trimmed = title.trim_start();
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    if (first as u32) < 0x80 {
        return None;
    }
    if !chars.next().is_some_and(|c| c.is_whitespace()) {
        return None;
    }
    Some(first)
}

#[cfg(unix)]
fn pty_shell_pid(pty: &alacritty_terminal::tty::Pty) -> Option<u32> {
    Some(pty.child().id())
}

#[cfg(not(unix))]
fn pty_shell_pid(_pty: &alacritty_terminal::tty::Pty) -> Option<u32> {
    None
}

#[cfg(target_os = "linux")]
fn foreground_process_glyph(shell_pid: u32) -> Option<char> {
    let tpgid = read_tpgid(shell_pid)?;
    if tpgid <= 0 {
        return None;
    }
    let comm = std::fs::read_to_string(format!("/proc/{tpgid}/comm")).ok();
    let cmdline = read_cmdline(tpgid as u32);
    let comm_trim = comm.as_deref().map(str::trim).unwrap_or("");

    // Match `comm` first (cheap), then anywhere in `cmdline` — picks up
    // `node /path/to/agent-cli.js`-style wrappers that hide behind their
    // runtime's name.
    let by_comm =
        AGENT_PROCESS_GLYPHS.iter().find(|(name, _)| comm_trim.starts_with(name)).map(|(_, g)| *g);
    if by_comm.is_some() {
        return by_comm;
    }
    if let Some(cmd) = &cmdline {
        let glyph =
            AGENT_PROCESS_GLYPHS.iter().find(|(name, _)| cmd.contains(name)).map(|(_, g)| *g);
        if glyph.is_some() {
            return glyph;
        }
        log::debug!("foreground process not matched: comm={comm_trim:?} cmdline={cmd:?}");
    }
    None
}

#[cfg(target_os = "linux")]
fn read_cmdline(pid: u32) -> Option<String> {
    // `cmdline` is NUL-separated argv; rendering with spaces is good enough
    // for substring matching and human-readable logging.
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let s: String = bytes.iter().map(|&b| if b == 0 { ' ' } else { b as char }).collect();
    Some(s.trim().to_string())
}

/// `/proc/<pid>/stat` is `pid (comm) state ppid pgrp session tty_nr tpgid …`.
/// `comm` may contain spaces and unmatched parens, so split on the *last* `)`
/// before tokenizing the rest.
#[cfg(target_os = "linux")]
fn read_tpgid(shell_pid: u32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{shell_pid}/stat")).ok()?;
    let close = stat.rfind(')')?;
    let after = &stat[close + 1..];
    // After `comm`: state(0) ppid(1) pgrp(2) session(3) tty_nr(4) tpgid(5).
    after.split_whitespace().nth(5)?.parse::<i32>().ok()
}

#[cfg(not(target_os = "linux"))]
fn foreground_process_glyph(_shell_pid: u32) -> Option<char> {
    // macOS would use `libproc::proc_pidfdinfo` / `tcgetpgrp` on the master
    // FD; Windows is its own world.  Not wired up yet.
    None
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
        let shell_pid = pty_shell_pid(&pty);

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
            shell_pid,
            agent_cache: Cell::new(AgentCache::default()),
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

    /// Sidebar glyph for the agent running here.  Identity comes from the
    /// PTY's foreground process (`/proc` on Linux); the displayed glyph
    /// prefers the title's current leading char so the agent's own spinner
    /// frames animate for free, falling back to a per-agent static glyph
    /// when the title is plain ASCII.  When proc identification yields
    /// nothing, accept a decorative title as a permissive fallback so
    /// agents we don't have in the process map still show *something*.
    pub fn agent_glyph(&self) -> Option<char> {
        let proc_glyph = self.process_agent_glyph();
        let title_glyph = title_decorative_glyph(&self.title);
        if proc_glyph.is_some() {
            return title_glyph.or(proc_glyph);
        }
        title_glyph
    }

    fn process_agent_glyph(&self) -> Option<char> {
        let cached = self.agent_cache.get();
        let fresh = cached.polled_at.is_some_and(|t| t.elapsed() < AGENT_CACHE_TTL);
        if fresh {
            return cached.process_glyph;
        }
        let glyph = self.shell_pid.and_then(foreground_process_glyph);
        self.agent_cache.set(AgentCache { polled_at: Some(Instant::now()), process_glyph: glyph });
        glyph
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
