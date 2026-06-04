//! Terminal window context.

use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::mem;
#[cfg(not(windows))]
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use glutin::config::Config as GlutinConfig;
use glutin::display::GetGlDisplay;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use glutin::platform::x11::X11GlConfigExt;
use log::{error, info};
use serde_json as json;
use winit::dpi::PhysicalSize;
use winit::event::{Event as WinitEvent, Modifiers, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::WindowId;

use alacritty_terminal::event::{Event as TerminalEvent, Notify, OnResize};
use alacritty_terminal::event_loop::{EventLoop as PtyEventLoop, Msg, Notifier};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::Direction;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{ClipboardType, Term, TermMode};
use alacritty_terminal::tty;

use crate::cli::{ParsedOptions, WindowOptions};
use crate::clipboard::Clipboard;
use crate::config::UiConfig;
use crate::display::window::Window;
use crate::display::{Display, SizeInfo, TabBarInfo};
use crate::event::{
    ActionContext, Event, EventProxy, EventType, InlineSearchState, Mouse, SearchState,
    TabSelection, TouchPurpose,
};
#[cfg(unix)]
use crate::logging::LOG_TARGET_IPC_CONFIG;
use crate::message_bar::MessageBuffer;
use crate::scheduler::Scheduler;
use crate::session::{SavedProject, SavedTab, SavedWindow};
use crate::{input, renderer};

/// Monotonic counter used to assign every terminal (tab) a process-wide unique id.
static NEXT_TERMINAL_ID: AtomicU64 = AtomicU64::new(0);

/// State of a single terminal (tab) hosted inside a window.
///
/// Everything that belongs to one running shell — the terminal grid, the PTY notifier and the
/// per-terminal UI state (search, title) — lives here. The window-level rendering resources are
/// shared across all tabs and stay in [`WindowContext`].
pub struct Tab {
    /// Process-wide unique id, used to route PTY events to this tab.
    terminal_id: u64,
    terminal: Arc<FairMutex<Term<EventProxy>>>,
    notifier: Notifier,
    inline_search_state: InlineSearchState,
    search_state: SearchState,
    /// Latest title reported by this terminal, shown on the tab bar.
    title: String,
    preserve_title: bool,
    #[cfg(not(windows))]
    master_fd: RawFd,
    #[cfg(not(windows))]
    shell_pid: u32,
}

impl Tab {
    /// Spawn a new terminal with its own PTY and I/O thread.
    fn new(
        config: &UiConfig,
        options: &WindowOptions,
        size_info: &SizeInfo,
        window_id: WindowId,
        proxy: EventLoopProxy<Event>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut pty_config = config.pty_config();
        options.terminal_options.override_pty_config(&mut pty_config);

        let preserve_title = options.window_identity.title.is_some();

        let terminal_id = NEXT_TERMINAL_ID.fetch_add(1, Ordering::Relaxed);
        let event_proxy = EventProxy::new(proxy, window_id, terminal_id);

        // Create the terminal.
        //
        // This object contains all of the state about what's being displayed. It's
        // wrapped in a clonable mutex since both the I/O loop and display need to
        // access it.
        let terminal = Term::new(config.term_options(), size_info, event_proxy.clone());
        let terminal = Arc::new(FairMutex::new(terminal));

        // Create the PTY.
        //
        // The PTY forks a process to run the shell on the slave side of the
        // pseudoterminal. A file descriptor for the master side is retained for
        // reading/writing to the shell.
        let pty = tty::new(&pty_config, (*size_info).into(), window_id.into())?;

        #[cfg(not(windows))]
        let master_fd = pty.file().as_raw_fd();
        #[cfg(not(windows))]
        let shell_pid = pty.child().id();

        // Create the pseudoterminal I/O loop.
        //
        // PTY I/O is ran on another thread as to not occupy cycles used by the
        // renderer and input processing. Note that access to the terminal state is
        // synchronized since the I/O loop updates the state, and the display
        // consumes it periodically.
        let event_loop = PtyEventLoop::new(
            Arc::clone(&terminal),
            event_proxy.clone(),
            pty,
            pty_config.drain_on_exit,
            config.debug.ref_test,
        )?;

        // The event loop channel allows write requests from the event processor
        // to be sent to the pty loop and ultimately written to the pty.
        let loop_tx = event_loop.channel();

        // Kick off the I/O thread.
        let _io_thread = event_loop.spawn();

        // Start cursor blinking, in case `Focused` isn't sent on startup.
        if config.cursor.style().blinking {
            event_proxy.send_event(TerminalEvent::CursorBlinkingChange.into());
        }

        Ok(Tab {
            terminal_id,
            terminal,
            notifier: Notifier(loop_tx),
            inline_search_state: Default::default(),
            search_state: Default::default(),
            title: String::new(),
            preserve_title,
            #[cfg(not(windows))]
            master_fd,
            #[cfg(not(windows))]
            shell_pid,
        })
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        // Shutdown the terminal's PTY.
        let _ = self.notifier.0.send(Msg::Shutdown);
    }
}

/// A project groups a set of tabs bound to a local folder.
///
/// Tabs spawned inside a project default their working directory to [`Self::root`]. A window owns
/// one or more projects and shows the active project's tabs in the tab bar.
struct Project {
    /// Display name shown in the project sidebar.
    name: String,
    /// Folder new tabs in this project are rooted at. `None` for the default (home) project.
    root: Option<PathBuf>,
    /// Terminals belonging to this project. Always contains at least one tab.
    tabs: Vec<Tab>,
    /// Index of the focused tab within [`Self::tabs`].
    active_tab: usize,
}

impl Project {
    /// Derive a sidebar label from a project root (folder basename, or `~` for the default).
    fn name_for(root: &Option<PathBuf>) -> String {
        match root {
            Some(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            None => "~".to_owned(),
        }
    }
}

/// Event context for one individual Alacritty window.
///
/// A window owns one or more [`Tab`]s (terminals) and the shared rendering resources used to draw
/// the currently active tab.
pub struct WindowContext {
    pub message_buffer: MessageBuffer,
    pub display: Display,
    pub dirty: bool,
    event_queue: Vec<WinitEvent<Event>>,
    /// Projects hosted in this window. Always contains at least one project.
    projects: Vec<Project>,
    /// Index of the currently active project within [`Self::projects`].
    active_project: usize,
    /// Stable key used to persist this window's layout across restarts.
    pub session_key: i64,
    /// Whether this window participates in session persistence.
    ///
    /// One-off windows launched with an explicit command (`alacritty -e ...`) are ephemeral and
    /// must never be written to or restored from the session store.
    pub persistent: bool,
    cursor_blink_timed_out: bool,
    prev_bell_cmd: Option<Instant>,
    modifiers: Modifiers,
    mouse: Mouse,
    touch: TouchPurpose,
    occluded: bool,
    window_config: ParsedOptions,
    config: Rc<UiConfig>,
}

impl WindowContext {
    /// Create initial window context that does bootstrapping the graphics API we're going to use.
    pub fn initial(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
        config: Rc<UiConfig>,
        mut options: WindowOptions,
    ) -> Result<Self, Box<dyn Error>> {
        let raw_display_handle = event_loop.display_handle().unwrap().as_raw();

        let mut identity = config.window.identity.clone();
        options.window_identity.override_identity_config(&mut identity);

        // Windows has different order of GL platform initialization compared to any other platform;
        // it requires the window first.
        #[cfg(windows)]
        let window = Window::new(event_loop, &config, &identity, &mut options)?;
        #[cfg(windows)]
        let raw_window_handle = Some(window.raw_window_handle());

        #[cfg(not(windows))]
        let raw_window_handle = None;

        let gl_display = renderer::platform::create_gl_display(
            raw_display_handle,
            raw_window_handle,
            config.debug.prefer_egl,
        )?;
        let gl_config = renderer::platform::pick_gl_config(&gl_display, raw_window_handle)?;

        #[cfg(not(windows))]
        let window = Window::new(
            event_loop,
            &config,
            &identity,
            &mut options,
            #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
            gl_config.x11_visual(),
        )?;

        // Create context.
        let gl_context =
            renderer::platform::create_gl_context(&gl_display, &gl_config, raw_window_handle)?;

        let display = Display::new(window, gl_context, &config, false, event_loop)?;

        Self::new(display, config, options, proxy)
    }

    /// Create additional context with the graphics platform other windows are using.
    pub fn additional(
        gl_config: &GlutinConfig,
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
        config: Rc<UiConfig>,
        mut options: WindowOptions,
        config_overrides: ParsedOptions,
    ) -> Result<Self, Box<dyn Error>> {
        let gl_display = gl_config.display();

        let mut identity = config.window.identity.clone();
        options.window_identity.override_identity_config(&mut identity);

        // Check if new window will be opened as a tab.
        // This must be done before `Window::new()`, which unsets `window_tabbing_id`.
        #[cfg(target_os = "macos")]
        let tabbed = options.window_tabbing_id.is_some();
        #[cfg(not(target_os = "macos"))]
        let tabbed = false;

        let window = Window::new(
            event_loop,
            &config,
            &identity,
            &mut options,
            #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
            gl_config.x11_visual(),
        )?;

        // Create context.
        let raw_window_handle = window.raw_window_handle();
        let gl_context =
            renderer::platform::create_gl_context(&gl_display, gl_config, Some(raw_window_handle))?;

        let display = Display::new(window, gl_context, &config, tabbed, event_loop)?;

        let mut window_context = Self::new(display, config, options, proxy)?;

        // Set the config overrides at startup.
        //
        // These are already applied to `config`, so no update is necessary.
        window_context.window_config = config_overrides;

        Ok(window_context)
    }

    /// Create a new terminal window context.
    fn new(
        display: Display,
        config: Rc<UiConfig>,
        options: WindowOptions,
        proxy: EventLoopProxy<Event>,
    ) -> Result<Self, Box<dyn Error>> {
        info!(
            "PTY dimensions: {:?} x {:?}",
            display.size_info.screen_lines(),
            display.size_info.columns()
        );

        // Windows launched to run a specific command are one-offs and never persisted.
        let persistent = options.terminal_options.command().is_none();

        // Create the initial tab (terminal) for this window.
        let tab = Tab::new(&config, &options, &display.size_info, display.window.id(), proxy)?;

        // The initial window is itself a project, rooted at its launch directory (if any).
        let root = options.terminal_options.working_directory.clone();
        let project =
            Project { name: Project::name_for(&root), root, tabs: vec![tab], active_tab: 0 };

        // Create context for the Alacritty window.
        Ok(WindowContext {
            display,
            config,
            projects: vec![project],
            active_project: 0,
            session_key: 0,
            persistent,
            cursor_blink_timed_out: Default::default(),
            prev_bell_cmd: Default::default(),
            message_buffer: Default::default(),
            window_config: Default::default(),
            event_queue: Default::default(),
            modifiers: Default::default(),
            occluded: Default::default(),
            mouse: Default::default(),
            touch: Default::default(),
            dirty: Default::default(),
        })
    }

    /// Reference to the currently active project.
    #[inline]
    fn active_project(&self) -> &Project {
        &self.projects[self.active_project]
    }

    /// Reference to the currently active tab (active tab of the active project).
    #[inline]
    fn active_tab(&self) -> &Tab {
        let project = self.active_project();
        &project.tabs[project.active_tab]
    }

    /// Working directory of the active tab's foreground process, if it can be resolved.
    #[cfg(not(windows))]
    pub fn active_working_directory(&self) -> Option<PathBuf> {
        let tab = self.active_tab();
        crate::daemon::foreground_process_path(tab.master_fd, tab.shell_pid).ok()
    }

    /// Working directory of a specific tab's foreground process, if it can be resolved.
    #[cfg(not(windows))]
    fn tab_working_directory(&self, tab: &Tab) -> Option<PathBuf> {
        crate::daemon::foreground_process_path(tab.master_fd, tab.shell_pid).ok()
    }

    #[cfg(windows)]
    fn tab_working_directory(&self, _tab: &Tab) -> Option<PathBuf> {
        None
    }

    /// Capture this window's layout (projects, tabs, geometry) for session persistence.
    pub fn session_snapshot(&self) -> SavedWindow {
        let projects = self
            .projects
            .iter()
            .map(|project| {
                let tabs = project
                    .tabs
                    .iter()
                    .map(|tab| SavedTab {
                        working_directory: self.tab_working_directory(tab),
                        title: tab.title.clone(),
                    })
                    .collect();
                SavedProject {
                    name: project.name.clone(),
                    root: project.root.clone(),
                    active_tab: project.active_tab,
                    tabs,
                }
            })
            .collect();

        let size = self.display.window.winit_window().inner_size();

        SavedWindow {
            key: self.session_key,
            width: size.width,
            height: size.height,
            active_project: self.active_project,
            projects,
        }
    }

    /// Recreate the remaining tabs of a restored window.
    ///
    /// The window is created with its first tab already spawned (in `tabs[0]`'s directory), so
    /// this adds `tabs[1..]`, restores the saved titles and focuses the previously active tab.
    /// Recreate a restored window's projects and tabs.
    ///
    /// The window is created with one bootstrap project holding one tab (spawned in the first
    /// saved tab's directory). This configures that project, spawns the remaining tabs, then
    /// rebuilds the other projects with their tabs and finally focuses the saved active project.
    pub fn restore_projects(
        &mut self,
        proxy: EventLoopProxy<Event>,
        saved: &[SavedProject],
        active_project: usize,
    ) {
        if saved.is_empty() {
            return;
        }

        // Configure the bootstrap project (index 0, already holding tab 0) from `saved[0]`.
        self.projects[0].name = saved[0].name.clone();
        self.projects[0].root = saved[0].root.clone();

        // Add placeholder projects for the rest; tabs are spawned below.
        for sp in saved.iter().skip(1) {
            self.projects.push(Project {
                name: sp.name.clone(),
                root: sp.root.clone(),
                tabs: Vec::new(),
                active_tab: 0,
            });
        }

        // Spawn each project's tabs. Project 0 already has its first tab, so skip it there.
        let window_id = self.display.window.id();
        for (pi, sp) in saved.iter().enumerate() {
            let start = usize::from(pi == 0);
            for tab in sp.tabs.iter().skip(start) {
                let mut options = WindowOptions::default();
                options.terminal_options.working_directory =
                    tab.working_directory.clone().or_else(|| sp.root.clone());
                match Tab::new(
                    &self.config,
                    &options,
                    &self.display.size_info,
                    window_id,
                    proxy.clone(),
                ) {
                    Ok(tab) => self.projects[pi].tabs.push(tab),
                    Err(err) => error!("Unable to restore tab: {err}"),
                }
            }
        }

        // Seed saved titles and clamp each project's active tab.
        for (pi, sp) in saved.iter().enumerate() {
            let project = &mut self.projects[pi];
            for (tab, st) in project.tabs.iter_mut().zip(&sp.tabs) {
                if !st.title.is_empty() {
                    tab.title = st.title.clone();
                }
            }
            project.active_tab = sp.active_tab.min(project.tabs.len().saturating_sub(1));
        }

        // Drop any project whose tabs all failed to spawn, then focus the saved active project.
        self.projects.retain(|p| !p.tabs.is_empty());
        self.active_project = active_project.min(self.projects.len().saturating_sub(1));
        self.sync_tabs();
    }

    /// Resize the window to the given physical pixel dimensions (used when restoring).
    pub fn set_pixel_size(&self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let _ =
            self.display.window.winit_window().request_inner_size(PhysicalSize::new(width, height));
    }

    /// Spawn a new tab in the active project and focus it.
    ///
    /// When the active project has a root folder it takes precedence, so tabs opened inside a
    /// project always start in the project directory.
    pub fn add_tab(&mut self, proxy: EventLoopProxy<Event>, working_directory: Option<PathBuf>) {
        let project_index = self.active_project;
        let working_directory = self.projects[project_index].root.clone().or(working_directory);

        let mut options = WindowOptions::default();
        options.terminal_options.working_directory = working_directory;

        let window_id = self.display.window.id();
        match Tab::new(&self.config, &options, &self.display.size_info, window_id, proxy) {
            Ok(tab) => {
                let project = &mut self.projects[project_index];
                project.tabs.push(tab);
                project.active_tab = project.tabs.len() - 1;
                // The tab bar may have just appeared; recompute layout and redraw.
                self.display.pending_update.dirty = true;
                self.dirty = true;
            },
            Err(err) => error!("Unable to create new tab: {err}"),
        }
    }

    /// Create a new project rooted at `root` (with one tab) and switch to it.
    pub fn add_project(&mut self, proxy: EventLoopProxy<Event>, root: PathBuf) {
        let mut options = WindowOptions::default();
        options.terminal_options.working_directory = Some(root.clone());

        let window_id = self.display.window.id();
        match Tab::new(&self.config, &options, &self.display.size_info, window_id, proxy) {
            Ok(tab) => {
                let name = Project::name_for(&Some(root.clone()));
                self.projects.push(Project {
                    name,
                    root: Some(root),
                    tabs: vec![tab],
                    active_tab: 0,
                });
                self.select_project(self.projects.len() - 1);
                self.display.pending_update.dirty = true;
                self.dirty = true;
            },
            Err(err) => error!("Unable to create new project: {err}"),
        }
    }

    /// Switch the active project, carrying focus and updating the window title.
    pub fn select_project(&mut self, index: usize) {
        if index >= self.projects.len() || index == self.active_project {
            return;
        }

        // Carry the focus state from the old active tab to the new project's active tab.
        let old_project = self.active_project;
        let old_tab = self.projects[old_project].active_tab;
        let focused = self.projects[old_project].tabs[old_tab].terminal.lock().is_focused;

        self.active_project = index;

        let project = &self.projects[index];
        let new_tab = project.active_tab;
        project.tabs[new_tab].terminal.lock().is_focused = focused;

        // Reflect the newly-focused tab's title in the window title.
        let title = project.tabs[new_tab].title.clone();
        let preserve_title = project.tabs[new_tab].preserve_title;
        if !preserve_title && self.config.window.dynamic_title && !title.is_empty() {
            self.display.window.set_title(title);
        }

        self.display.pending_update.dirty = true;
        self.dirty = true;
    }

    /// Close the tab with the given terminal id, wherever it lives.
    ///
    /// The exiting tab may belong to a background project, so all projects are searched. When a
    /// project's last tab closes the project itself is removed. Returns `true` only when the
    /// window's very last tab (of its last project) is gone and the window should close.
    pub fn close_tab(&mut self, terminal_id: u64) -> bool {
        // Locate the tab across all projects.
        let located = self.projects.iter().enumerate().find_map(|(pi, project)| {
            project.tabs.iter().position(|t| t.terminal_id == terminal_id).map(|ti| (pi, ti))
        });
        let (project_index, tab_index) = match located {
            Some(loc) => loc,
            None => return self.projects.is_empty(),
        };

        // Dropping the tab shuts down its PTY.
        self.projects[project_index].tabs.remove(tab_index);

        if self.projects[project_index].tabs.is_empty() {
            // The project is now empty; remove it.
            self.projects.remove(project_index);
            if self.projects.is_empty() {
                return true;
            }
            // Keep `active_project` pointing at a valid project.
            if project_index < self.active_project {
                self.active_project -= 1;
            } else if self.active_project >= self.projects.len() {
                self.active_project = self.projects.len() - 1;
            }
        } else {
            // Keep the project's `active_tab` pointing at a valid tab.
            let project = &mut self.projects[project_index];
            if tab_index < project.active_tab {
                project.active_tab -= 1;
            } else if project.active_tab >= project.tabs.len() {
                project.active_tab = project.tabs.len() - 1;
            }
        }

        self.display.pending_update.dirty = true;
        self.dirty = true;
        false
    }

    /// Focus the tab at `index` in the active project, if it exists.
    pub fn select_tab(&mut self, index: usize) {
        if index < self.active_project().tabs.len() {
            self.set_active_tab(index);
        }
    }

    /// Focus the tab with the given terminal id within the active project, if it still exists.
    pub fn select_tab_by_id(&mut self, terminal_id: u64) {
        let index =
            self.active_project().tabs.iter().position(|tab| tab.terminal_id == terminal_id);
        if let Some(index) = index {
            self.set_active_tab(index);
        }
    }

    /// Request that the tab with the given terminal id close, if it still exists.
    ///
    /// Exiting the terminal emits an `Exit` event tagged with the tab's id, which the event loop
    /// turns into a [`Self::close_tab`] (closing the window if it was the last tab). Targeting by
    /// id (rather than index) keeps this correct even if tabs shift between click and handling.
    pub fn request_close_tab(&mut self, terminal_id: u64) {
        for project in &self.projects {
            if let Some(tab) = project.tabs.iter().find(|tab| tab.terminal_id == terminal_id) {
                tab.terminal.lock().exit();
                return;
            }
        }
    }

    /// Focus the last tab of the active project.
    pub fn select_last_tab(&mut self) {
        self.set_active_tab(self.active_project().tabs.len() - 1);
    }

    /// Focus the next tab in the active project, wrapping around.
    pub fn select_next_tab(&mut self) {
        let project = self.active_project();
        let next = (project.active_tab + 1) % project.tabs.len();
        self.set_active_tab(next);
    }

    /// Focus the previous tab in the active project, wrapping around.
    pub fn select_previous_tab(&mut self) {
        let project = self.active_project();
        let prev = (project.active_tab + project.tabs.len() - 1) % project.tabs.len();
        self.set_active_tab(prev);
    }

    /// Focus the tab at `index` within the active project.
    fn set_active_tab(&mut self, index: usize) {
        let project_index = self.active_project;
        if index == self.projects[project_index].active_tab {
            return;
        }

        // Carry the focus state to the newly-active tab so its cursor renders correctly.
        let project = &mut self.projects[project_index];
        let active = project.active_tab;
        let focused = project.tabs[active].terminal.lock().is_focused;
        project.tabs[index].terminal.lock().is_focused = focused;
        project.active_tab = index;

        // Reflect the newly-focused tab's title in the window title.
        let title = project.tabs[index].title.clone();
        let preserve_title = project.tabs[index].preserve_title;
        if !preserve_title && self.config.window.dynamic_title && !title.is_empty() {
            self.display.window.set_title(title);
        }

        // The newly-active terminal must be resized to the current viewport and redrawn.
        self.display.pending_update.dirty = true;
        self.dirty = true;
    }

    /// Apply layout changes after a tab was created, closed, or switched.
    ///
    /// Resizes the now-active tab to the current viewport — covering tab switches where the
    /// window size itself did not change, which the regular resize path would miss — and
    /// requests a redraw.
    pub fn sync_tabs(&mut self) {
        let project_index = self.active_project;
        let tab_bar_lines = usize::from(self.projects[project_index].tabs.len() >= 2);
        let idx = self.projects[project_index].active_tab;
        let tab = &mut self.projects[project_index].tabs[idx];
        let mut terminal = tab.terminal.lock();

        if self.display.pending_update.dirty {
            let old_is_searching = tab.search_state.history_index.is_some();
            Self::submit_display_update(
                &mut terminal,
                &mut self.display,
                &mut tab.notifier,
                &self.message_buffer,
                &mut tab.search_state,
                old_is_searching,
                &self.config,
                tab_bar_lines,
            );
        }

        // Resize the active tab to match the current viewport, even when the window size is
        // unchanged (e.g. when switching to a background tab sized for an older layout).
        let size_info = self.display.size_info;
        if terminal.columns() != size_info.columns()
            || terminal.screen_lines() != size_info.screen_lines()
        {
            tab.notifier.on_resize(size_info.into());
            terminal.resize(size_info);
        }

        drop(terminal);

        self.dirty = true;
        if self.display.window.has_frame {
            self.display.window.request_redraw();
        }
    }

    /// Update the terminal window to the latest config.
    pub fn update_config(&mut self, new_config: Rc<UiConfig>) {
        let old_config = mem::replace(&mut self.config, new_config);

        // Apply ipc config if there are overrides.
        self.config = self.window_config.override_config_rc(self.config.clone());

        self.display.update_config(&self.config);
        for project in &self.projects {
            for tab in &project.tabs {
                tab.terminal.lock().set_options(self.config.term_options());
            }
        }

        // Reload cursor if its thickness has changed.
        if (old_config.cursor.thickness() - self.config.cursor.thickness()).abs() > f32::EPSILON {
            self.display.pending_update.set_cursor_dirty();
        }

        if old_config.font != self.config.font {
            let scale_factor = self.display.window.scale_factor as f32;
            // Do not update font size if it has been changed at runtime.
            if self.display.font_size == old_config.font.size().scale(scale_factor) {
                self.display.font_size = self.config.font.size().scale(scale_factor);
            }

            let font = self.config.font.clone().with_size(self.display.font_size);
            self.display.pending_update.set_font(font);
        }

        // Always reload the theme to account for auto-theme switching.
        self.display.window.set_theme(self.config.window.theme());

        // Update display if either padding options or resize increments were changed.
        let window_config = &old_config.window;
        if window_config.padding(1.) != self.config.window.padding(1.)
            || window_config.dynamic_padding != self.config.window.dynamic_padding
            || window_config.resize_increments != self.config.window.resize_increments
        {
            self.display.pending_update.dirty = true;
        }

        // Update title on config reload according to the following table.
        //
        // │cli │ dynamic_title │ current_title == old_config ││ set_title │
        // │ Y  │       _       │              _              ││     N     │
        // │ N  │       Y       │              Y              ││     Y     │
        // │ N  │       Y       │              N              ││     N     │
        // │ N  │       N       │              _              ││     Y     │
        if !self.active_tab().preserve_title
            && (!self.config.window.dynamic_title
                || self.display.window.title() == old_config.window.identity.title)
        {
            self.display.window.set_title(self.config.window.identity.title.clone());
        }

        let opaque = self.config.window_opacity() >= 1.;

        // Disable shadows for transparent windows on macOS.
        #[cfg(target_os = "macos")]
        self.display.window.set_has_shadow(opaque);

        #[cfg(target_os = "macos")]
        self.display.window.set_option_as_alt(self.config.window.option_as_alt());

        // Change opacity and blur state.
        self.display.window.set_transparent(!opaque);
        self.display.window.set_blur(self.config.window.blur);

        // Update hint keys.
        self.display.hint_state.update_alphabet(self.config.hints.alphabet());

        // Update cursor blinking.
        let event = Event::new(TerminalEvent::CursorBlinkingChange.into(), None);
        self.event_queue.push(event.into());

        self.dirty = true;
    }

    /// Get reference to the window's configuration.
    #[cfg(unix)]
    pub fn config(&self) -> &UiConfig {
        &self.config
    }

    /// Clear the window config overrides.
    #[cfg(unix)]
    pub fn reset_window_config(&mut self, config: Rc<UiConfig>) {
        // Clear previous window errors.
        self.message_buffer.remove_target(LOG_TARGET_IPC_CONFIG);

        self.window_config.clear();

        // Reload current config to pull new IPC config.
        self.update_config(config);
    }

    /// Add new window config overrides.
    #[cfg(unix)]
    pub fn add_window_config(&mut self, config: Rc<UiConfig>, options: &ParsedOptions) {
        // Clear previous window errors.
        self.message_buffer.remove_target(LOG_TARGET_IPC_CONFIG);

        self.window_config.extend_from_slice(options);

        // Reload current config to pull new IPC config.
        self.update_config(config);
    }

    /// Draw the window.
    pub fn draw(&mut self, scheduler: &mut Scheduler, proxy: &EventLoopProxy<Event>) {
        self.display.window.requested_redraw = false;

        if self.occluded {
            return;
        }

        self.dirty = false;

        // Force the display to process any pending display update.
        self.display.process_renderer_update();

        // Request immediate re-draw if visual bell animation is not finished yet.
        if !self.display.visual_bell.completed() {
            // We can get an OS redraw which bypasses alacritty's frame throttling, thus
            // marking the window as dirty when we don't have frame yet.
            if self.display.window.has_frame {
                self.display.window.request_redraw();
            } else {
                self.dirty = true;
            }
        }

        // Redraw the window using the currently active tab.
        let tab_bar = self.tab_bar_info();
        let project_index = self.active_project;
        let idx = self.projects[project_index].active_tab;
        let tab = &mut self.projects[project_index].tabs[idx];
        let terminal = tab.terminal.lock();
        self.display.draw(
            terminal,
            scheduler,
            &self.message_buffer,
            &self.config,
            &mut tab.search_state,
            &tab_bar,
        );

        // Dispatch tab-bar actions egui produced this frame through the normal event path (so they
        // reuse the existing create/select/close handling).
        let actions = self.display.take_chrome_actions();
        let window_id = self.display.window.id();
        if actions.create {
            let _ = proxy.send_event(Event::new(EventType::CreateTab, window_id));
        }
        if let Some(id) = actions.select {
            let event = Event::new(EventType::SelectTab(TabSelection::Id(id)), window_id);
            let _ = proxy.send_event(event);
        }
        if let Some(id) = actions.close {
            let _ = proxy.send_event(Event::new(EventType::CloseTab(id), window_id));
        }
        if actions.copy {
            let _ = proxy.send_event(Event::new(EventType::Copy, window_id));
        }
        if actions.paste {
            let _ = proxy.send_event(Event::new(EventType::Paste, window_id));
        }
        if let Some(index) = actions.select_project {
            let _ = proxy.send_event(Event::new(EventType::SelectProject(index), window_id));
        }
        if actions.create_project {
            // We're on the main thread inside the winit handler, so a blocking native folder
            // dialog is fine (it's modal). Dispatch the result through the event path.
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("选择项目文件夹")
                .set_parent(self.display.window.winit_window())
                .pick_folder()
            {
                let _ = proxy.send_event(Event::new(EventType::CreateProject(dir), window_id));
            }
        }

        // The chrome's reserved size changed; recompute the terminal layout so it fits beside it.
        if actions.layout_changed {
            self.sync_tabs();
        }
    }

    /// Copy the active tab's selection to the system clipboard.
    pub fn copy_active_selection(&self, clipboard: &mut Clipboard) {
        if let Some(text) =
            self.active_tab().terminal.lock().selection_to_string().filter(|s| !s.is_empty())
        {
            clipboard.store(ClipboardType::Clipboard, text);
        }
    }

    /// Paste `text` into the active tab, using bracketed paste when the terminal requests it.
    pub fn paste_to_active(&mut self, text: &str) {
        let project_index = self.active_project;
        let tab_index = self.projects[project_index].active_tab;
        let tab = &mut self.projects[project_index].tabs[tab_index];
        let bracketed = tab.terminal.lock().mode().contains(TermMode::BRACKETED_PASTE);
        if bracketed {
            tab.notifier.notify(b"\x1b[200~"[..].to_vec());
            // Filter escapes that could break out of bracketed paste, and normalize newlines.
            let payload: String = text
                .chars()
                .filter(|c| *c != '\x1b' && *c != '\x03')
                .collect::<String>()
                .replace("\r\n", "\r")
                .replace('\n', "\r");
            tab.notifier.notify(payload.into_bytes());
            tab.notifier.notify(b"\x1b[201~"[..].to_vec());
        } else {
            let payload = text.replace("\r\n", "\r").replace('\n', "\r");
            tab.notifier.notify(payload.into_bytes());
        }
    }

    /// Snapshot of the chrome contents for rendering (project list + active project's tabs).
    fn tab_bar_info(&self) -> TabBarInfo {
        let project = self.active_project();
        TabBarInfo {
            titles: project.tabs.iter().map(|tab| tab.title.clone()).collect(),
            ids: project.tabs.iter().map(|tab| tab.terminal_id).collect(),
            active: project.active_tab,
            project_names: self.projects.iter().map(|p| p.name.clone()).collect(),
            active_project: self.active_project,
        }
    }

    /// Process events for this terminal window.
    pub fn handle_event(
        &mut self,
        #[cfg(target_os = "macos")] event_loop: &ActiveEventLoop,
        event_proxy: &EventLoopProxy<Event>,
        clipboard: &mut Clipboard,
        scheduler: &mut Scheduler,
        event: WinitEvent<Event>,
    ) {
        match event {
            WinitEvent::AboutToWait
            | WinitEvent::WindowEvent { event: WindowEvent::RedrawRequested, .. } => {
                // Skip further event handling with no staged updates.
                if self.event_queue.is_empty() {
                    return;
                }

                // Continue to process all pending events.
            },
            event => {
                self.event_queue.push(event);
                return;
            },
        }

        // Remember whether the active tab was searching before processing events.
        let old_is_searching = self.active_tab().search_state.history_index.is_some();

        // Process each staged event against its target tab. PTY-generated events are routed to the
        // originating tab (which may be in the background); everything else targets the active tab.
        let events: Vec<_> = self.event_queue.drain(..).collect();
        for queued in events {
            // Offer window events to the egui chrome first. If egui consumes one (e.g. a click on
            // the tab bar), it must not also reach the terminal.
            if let WinitEvent::WindowEvent { event: window_event, .. } = &queued {
                let response = self.display.feed_egui_event(window_event);
                if response.repaint {
                    self.dirty = true;
                }
                if response.consumed {
                    continue;
                }
            }

            let target = self.event_target_tab(&queued);
            self.process_event(
                target,
                #[cfg(target_os = "macos")]
                event_loop,
                event_proxy,
                clipboard,
                scheduler,
                queued,
            );
        }

        // Post-processing (display updates, hint highlighting, redraw) operates on the active tab
        // together with the shared display.
        let project_index = self.active_project;
        let tab_bar_lines = usize::from(self.projects[project_index].tabs.len() >= 2);
        let idx = self.projects[project_index].active_tab;
        let tab = &mut self.projects[project_index].tabs[idx];
        let mut terminal = tab.terminal.lock();

        // Process DisplayUpdate events.
        if self.display.pending_update.dirty {
            Self::submit_display_update(
                &mut terminal,
                &mut self.display,
                &mut tab.notifier,
                &self.message_buffer,
                &mut tab.search_state,
                old_is_searching,
                &self.config,
                tab_bar_lines,
            );
            self.dirty = true;
        }

        if self.dirty || self.mouse.hint_highlight_dirty {
            self.dirty |= self.display.update_highlighted_hints(
                &terminal,
                &self.config,
                &self.mouse,
                self.modifiers.state(),
            );
            self.mouse.hint_highlight_dirty = false;
        }

        // Don't call `request_redraw` when event is `RedrawRequested` since the `dirty` flag
        // represents the current frame, but redraw is for the next frame.
        if self.dirty
            && self.display.window.has_frame
            && !self.occluded
            && !matches!(event, WinitEvent::WindowEvent { event: WindowEvent::RedrawRequested, .. })
        {
            self.display.window.request_redraw();
        }
    }

    /// Determine which `(project, tab)` a staged event should be dispatched to.
    ///
    /// PTY-generated events carry the id of their originating terminal so they reach the right
    /// tab even when its project is not active. All other events target the active project's
    /// active tab.
    fn event_target_tab(&self, event: &WinitEvent<Event>) -> (usize, usize) {
        if let WinitEvent::UserEvent(user_event) = event {
            if let Some(terminal_id) = user_event.terminal_id() {
                for (pi, project) in self.projects.iter().enumerate() {
                    if let Some(ti) = project.tabs.iter().position(|t| t.terminal_id == terminal_id)
                    {
                        return (pi, ti);
                    }
                }
            }
        }
        (self.active_project, self.active_project().active_tab)
    }

    /// Process a single event against the tab at `(project_index, tab_index)`.
    fn process_event(
        &mut self,
        target: (usize, usize),
        #[cfg(target_os = "macos")] event_loop: &ActiveEventLoop,
        event_proxy: &EventLoopProxy<Event>,
        clipboard: &mut Clipboard,
        scheduler: &mut Scheduler,
        event: WinitEvent<Event>,
    ) {
        let (project_index, tab_index) = target;
        let is_active_tab = project_index == self.active_project
            && tab_index == self.projects[self.active_project].active_tab;
        let tab = &mut self.projects[project_index].tabs[tab_index];
        let mut terminal = tab.terminal.lock();

        let context = ActionContext {
            cursor_blink_timed_out: &mut self.cursor_blink_timed_out,
            prev_bell_cmd: &mut self.prev_bell_cmd,
            message_buffer: &mut self.message_buffer,
            inline_search_state: &mut tab.inline_search_state,
            search_state: &mut tab.search_state,
            modifiers: &mut self.modifiers,
            notifier: &mut tab.notifier,
            display: &mut self.display,
            mouse: &mut self.mouse,
            touch: &mut self.touch,
            dirty: &mut self.dirty,
            occluded: &mut self.occluded,
            terminal: &mut terminal,
            #[cfg(not(windows))]
            master_fd: tab.master_fd,
            #[cfg(not(windows))]
            shell_pid: tab.shell_pid,
            preserve_title: tab.preserve_title,
            tab_title: &mut tab.title,
            is_active_tab,
            config: &self.config,
            event_proxy,
            #[cfg(target_os = "macos")]
            event_loop,
            clipboard,
            scheduler,
        };
        let mut processor = input::Processor::new(context);
        processor.handle_event(event);
    }

    /// ID of this terminal context.
    pub fn id(&self) -> WindowId {
        self.display.window.id()
    }

    /// Write the ref test results to the disk.
    pub fn write_ref_test_results(&self) {
        // Dump grid state.
        let mut grid = self.active_tab().terminal.lock().grid().clone();
        grid.initialize_all();
        grid.truncate();

        let serialized_grid = json::to_string(&grid).expect("serialize grid");

        let size_info = &self.display.size_info;
        let size = TermSize::new(size_info.columns(), size_info.screen_lines());
        let serialized_size = json::to_string(&size).expect("serialize size");

        let serialized_config = format!("{{\"history_size\":{}}}", grid.history_size());

        File::create("./grid.json")
            .and_then(|mut f| f.write_all(serialized_grid.as_bytes()))
            .expect("write grid.json");

        File::create("./size.json")
            .and_then(|mut f| f.write_all(serialized_size.as_bytes()))
            .expect("write size.json");

        File::create("./config.json")
            .and_then(|mut f| f.write_all(serialized_config.as_bytes()))
            .expect("write config.json");
    }

    /// Submit the pending changes to the `Display`.
    #[allow(clippy::too_many_arguments)]
    fn submit_display_update(
        terminal: &mut Term<EventProxy>,
        display: &mut Display,
        notifier: &mut Notifier,
        message_buffer: &MessageBuffer,
        search_state: &mut SearchState,
        old_is_searching: bool,
        config: &UiConfig,
        tab_bar_lines: usize,
    ) {
        // Compute cursor positions before resize.
        let num_lines = terminal.screen_lines();
        let cursor_at_bottom = terminal.grid().cursor.point.line + 1 == num_lines;
        let origin_at_bottom = if terminal.mode().contains(TermMode::VI) {
            terminal.vi_mode_cursor.point.line == num_lines - 1
        } else {
            search_state.direction == Direction::Left
        };

        display.handle_update(
            terminal,
            notifier,
            message_buffer,
            search_state,
            config,
            tab_bar_lines,
        );

        let new_is_searching = search_state.history_index.is_some();
        if !old_is_searching && new_is_searching {
            // Scroll on search start to make sure origin is visible with minimal viewport motion.
            let display_offset = terminal.grid().display_offset();
            if display_offset == 0 && cursor_at_bottom && !origin_at_bottom {
                terminal.scroll_display(Scroll::Delta(1));
            } else if display_offset != 0 && origin_at_bottom {
                terminal.scroll_display(Scroll::Delta(-1));
            }
        }
    }
}
