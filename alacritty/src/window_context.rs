//! Terminal window context.

use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::mem;
#[cfg(not(windows))]
use std::os::unix::io::{AsRawFd, RawFd};
#[cfg(not(windows))]
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use glutin::config::Config as GlutinConfig;
use glutin::display::GetGlDisplay;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use glutin::platform::x11::X11GlConfigExt;
use log::info;
use serde_json as json;
use winit::event::{Event as WinitEvent, Modifiers, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::WindowId;

use alacritty_terminal::event::Event as TerminalEvent;
use alacritty_terminal::event_loop::{EventLoop as PtyEventLoop, Msg, Notifier};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::Direction;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::tty;

use crate::cli::{ParsedOptions, WindowOptions};
use crate::clipboard::Clipboard;
use crate::config::UiConfig;
use crate::config::tabs::{TabBarEdge, TabSwitchStrategy};
#[cfg(not(windows))]
use crate::daemon::foreground_process_path;
use crate::display::Display;
use crate::display::window::Window;
use crate::event::{
    ActionContext, Event, EventProxy, EventType, InlineSearchState, Mouse, SearchState, TabAction,
    TabId, TouchPurpose,
};
#[cfg(unix)]
use crate::logging::LOG_TARGET_IPC_CONFIG;
use crate::message_bar::MessageBuffer;
use crate::scheduler::Scheduler;
use crate::{input, renderer};

#[cfg(not(windows))]
fn foreground_process_name(master_fd: RawFd, shell_pid: u32) -> Option<String> {
    let mut pid = unsafe { libc::tcgetpgrp(master_fd) };
    if pid < 0 {
        pid = shell_pid as i32;
    }

    let process_name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let process_name = process_name.trim();
    (!process_name.is_empty()).then(|| process_name.to_owned())
}

#[cfg(not(windows))]
fn process_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(not(windows))]
fn format_tab_cwd(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if home.as_deref().is_some_and(|home| home == path) {
        return String::from("~");
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(not(windows))]
fn is_shell_process(process: &str) -> bool {
    matches!(process, "bash" | "dash" | "fish" | "nu" | "sh" | "zsh" | "tmux")
}

/// Event context for one individual Alacritty window.
struct TerminalTab {
    id: TabId,
    terminal: Arc<FairMutex<Term<EventProxy>>>,
    notifier: Notifier,
    terminal_title: Option<String>,
    detected_title: String,
    custom_title: Option<String>,
    message_buffer: MessageBuffer,
    cursor_blink_timed_out: bool,
    prev_bell_cmd: Option<Instant>,
    inline_search_state: InlineSearchState,
    search_state: SearchState,
    #[cfg(not(windows))]
    master_fd: RawFd,
    #[cfg(not(windows))]
    shell_pid: u32,
}

impl TerminalTab {
    fn new(
        id: TabId,
        window_id: WindowId,
        size_info: crate::display::SizeInfo,
        config: &UiConfig,
        options: &WindowOptions,
        proxy: &EventLoopProxy<Event>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut pty_config = config.pty_config();
        options.terminal_options.override_pty_config(&mut pty_config);

        let event_proxy = EventProxy::new(proxy.clone(), window_id, id);

        let terminal = Term::new(config.term_options(), &size_info, event_proxy.clone());
        let terminal = Arc::new(FairMutex::new(terminal));

        let pty = tty::new(&pty_config, size_info.into(), window_id.into())?;

        #[cfg(not(windows))]
        let master_fd = pty.file().as_raw_fd();
        #[cfg(not(windows))]
        let shell_pid = pty.child().id();

        let event_loop = PtyEventLoop::new(
            Arc::clone(&terminal),
            event_proxy.clone(),
            pty,
            pty_config.drain_on_exit,
            config.debug.ref_test,
        )?;

        let loop_tx = event_loop.channel();
        let _io_thread = event_loop.spawn();

        if config.cursor.style().blinking {
            event_proxy.send_event(TerminalEvent::CursorBlinkingChange.into());
        }

        Ok(Self {
            id,
            terminal,
            #[cfg(not(windows))]
            master_fd,
            #[cfg(not(windows))]
            shell_pid,
            notifier: Notifier(loop_tx),
            terminal_title: None,
            detected_title: Self::detected_title(
                #[cfg(not(windows))]
                master_fd,
                #[cfg(not(windows))]
                shell_pid,
                &config.window.identity.title,
            ),
            custom_title: None,
            cursor_blink_timed_out: false,
            prev_bell_cmd: None,
            inline_search_state: Default::default(),
            message_buffer: Default::default(),
            search_state: Default::default(),
        })
    }

    fn detected_title(
        #[cfg(not(windows))] master_fd: RawFd,
        #[cfg(not(windows))] shell_pid: u32,
        default_title: &str,
    ) -> String {
        #[cfg(not(windows))]
        {
            let process = foreground_process_name(master_fd, shell_pid);
            let cwd = foreground_process_path(master_fd, shell_pid)
                .ok()
                .map(|path| format_tab_cwd(&path));

            if let Some(process) = process.filter(|process| !is_shell_process(process)) {
                process
            } else if let Some(cwd) = cwd {
                cwd
            } else {
                default_title.to_owned()
            }
        }

        #[cfg(windows)]
        {
            default_title.to_owned()
        }
    }

    fn refresh_detected_title(&mut self, config: &UiConfig) -> bool {
        let detected_title = Self::detected_title(
            #[cfg(not(windows))]
            self.master_fd,
            #[cfg(not(windows))]
            self.shell_pid,
            &config.window.identity.title,
        );

        if self.detected_title == detected_title {
            false
        } else {
            self.detected_title = detected_title;
            true
        }
    }

    fn display_title(&self) -> &str {
        self.custom_title
            .as_deref()
            .or(self.terminal_title.as_deref())
            .unwrap_or(&self.detected_title)
    }
}

struct TabTitleEditor {
    tab_id: TabId,
    value: String,
}

pub struct WindowContext {
    pub display: Display,
    pub dirty: bool,
    event_queue: Vec<WinitEvent<Event>>,
    tabs: Vec<TerminalTab>,
    active_tab: usize,
    next_tab_id: u64,
    last_active_tab_id: Option<TabId>,
    tab_title_editor: Option<TabTitleEditor>,
    focused: bool,
    modifiers: Modifiers,
    mouse: Mouse,
    touch: TouchPurpose,
    occluded: bool,
    preserve_title: bool,
    window_config: ParsedOptions,
    config: Rc<UiConfig>,
    event_proxy: EventLoopProxy<Event>,
}

impl WindowContext {
    fn active_tab(&self) -> &TerminalTab {
        &self.tabs[self.active_tab]
    }

    fn active_tab_mut(&mut self) -> &mut TerminalTab {
        &mut self.tabs[self.active_tab]
    }

    fn tab_index(&self, tab_id: Option<TabId>) -> Option<usize> {
        match tab_id {
            Some(tab_id) => self.tabs.iter().position(|tab| tab.id == tab_id),
            None => Some(self.active_tab),
        }
    }

    fn tab_bar_lines(&self) -> usize {
        usize::from(self.config.tabs.display_tab_bar(self.tabs.len()))
    }

    fn tab_bar_at_top(&self) -> bool {
        self.tab_bar_lines() != 0 && self.config.tabs.tab_bar_edge == TabBarEdge::Top
    }

    fn refresh_window_title(&mut self) {
        let title = self.active_tab().display_title().to_owned();
        self.display.window.set_title(title);
    }

    fn start_tab_title_editor(&mut self) {
        let tab = self.active_tab();
        self.tab_title_editor = Some(TabTitleEditor {
            tab_id: tab.id,
            value: tab.custom_title.clone().unwrap_or_default(),
        });
        self.display.pending_update.dirty = true;
        self.dirty = true;
    }

    fn confirm_tab_title_editor(&mut self) {
        let Some(editor) = self.tab_title_editor.take() else {
            return;
        };
        let Some(index) = self.tabs.iter().position(|tab| tab.id == editor.tab_id) else {
            return;
        };

        let title = editor.value.trim().to_owned();
        self.tabs[index].custom_title = (!title.is_empty()).then_some(title);
        if index == self.active_tab {
            self.refresh_window_title();
        }
        self.display.pending_update.dirty = true;
        self.dirty = true;
    }

    fn cancel_tab_title_editor(&mut self) {
        if self.tab_title_editor.take().is_some() {
            self.display.pending_update.dirty = true;
            self.dirty = true;
        }
    }

    fn tab_title_input(&mut self, c: char) {
        let Some(editor) = self.tab_title_editor.as_mut() else {
            return;
        };

        match c {
            '\x08' | '\x7f' => {
                let _ = editor.value.pop();
            },
            ' '..='~' | '\u{a0}'..='\u{10ffff}' => editor.value.push(c),
            _ => return,
        }

        self.display.pending_update.dirty = true;
        self.dirty = true;
    }

    fn tab_title_pop_word(&mut self) {
        let Some(editor) = self.tab_title_editor.as_mut() else {
            return;
        };

        editor.value = editor.value.trim_end().to_owned();
        editor.value.truncate(editor.value.rfind(' ').map_or(0, |index| index + 1));
        self.display.pending_update.dirty = true;
        self.dirty = true;
    }

    fn render_tab_title(&self, index: usize, tab: &TerminalTab, active: bool) -> String {
        let template =
            if active { self.config.tabs.active_tab_title_template.as_deref() } else { None }
                .unwrap_or(&self.config.tabs.tab_title_template);

        let title = tab.display_title();
        let rendered = template
            .replace("{title}", title)
            .replace("{index}", &(index + 1).to_string())
            .replace("{num_windows}", "1");

        if rendered.is_empty() { title.to_owned() } else { rendered }
    }

    fn sync_focus(&mut self) {
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            let mut terminal = tab.terminal.lock();
            terminal.is_focused = self.focused && index == self.active_tab;
        }
    }

    fn set_active_tab(&mut self, index: usize) {
        if self.tabs.is_empty() {
            return;
        }

        let index = index.min(self.tabs.len() - 1);
        if self.active_tab == index {
            return;
        }

        self.last_active_tab_id = Some(self.tabs[self.active_tab].id);
        self.active_tab = index;
        let config = self.config.clone();
        self.active_tab_mut().refresh_detected_title(&config);
        self.cancel_tab_title_editor();
        self.sync_focus();
        self.refresh_window_title();
        self.display.damage_tracker.frame().mark_fully_damaged();
        self.display.damage_tracker.next_frame().mark_fully_damaged();
        self.display.pending_update.dirty = true;
        self.dirty = true;
    }

    fn create_tab(&mut self) -> Result<(), Box<dyn Error>> {
        let tab_id = TabId(self.next_tab_id);
        self.next_tab_id += 1;

        let mut options = WindowOptions::default();
        #[cfg(not(windows))]
        if let Some(working_directory) = process_cwd(self.active_tab().shell_pid)
            .filter(|path| path.is_dir())
            .or_else(|| {
                foreground_process_path(self.active_tab().master_fd, self.active_tab().shell_pid)
                    .ok()
                    .filter(|path| path.is_dir())
            })
        {
            options.terminal_options.working_directory = Some(working_directory);
        }
        let tab = match TerminalTab::new(
            tab_id,
            self.display.window.id(),
            self.display.size_info,
            &self.config,
            &options,
            &self.event_proxy,
        ) {
            Ok(tab) => tab,
            Err(err) if options.terminal_options.working_directory.is_some() => {
                log::warn!(
                    "Retrying tab creation without inherited working directory after error: {err:?}"
                );
                let fallback_options = WindowOptions::default();
                TerminalTab::new(
                    tab_id,
                    self.display.window.id(),
                    self.display.size_info,
                    &self.config,
                    &fallback_options,
                    &self.event_proxy,
                )?
            },
            Err(err) => return Err(err),
        };

        self.tabs.push(tab);
        self.set_active_tab(self.tabs.len() - 1);
        self.display.damage_tracker.frame().mark_fully_damaged();
        self.display.damage_tracker.next_frame().mark_fully_damaged();
        self.display.pending_update.dirty = true;
        self.dirty = true;

        Ok(())
    }

    fn close_active_tab(&mut self) {
        let tab = self.active_tab_mut();
        tab.terminal.lock().exit();
    }

    fn move_active_tab(&mut self, delta: isize) {
        if self.tabs.len() < 2 {
            return;
        }

        let old_index = self.active_tab;
        let new_index = if delta < 0 {
            old_index.saturating_sub(delta.unsigned_abs())
        } else {
            (old_index + delta as usize).min(self.tabs.len() - 1)
        };

        if new_index == old_index {
            return;
        }

        self.tabs.swap(old_index, new_index);
        self.active_tab = new_index;
        self.display.damage_tracker.frame().mark_fully_damaged();
        self.display.damage_tracker.next_frame().mark_fully_damaged();
        self.dirty = true;
    }

    pub fn handle_tab_wakeup(&mut self, tab_id: Option<TabId>) {
        if let Some(index) = self.tab_index(tab_id) {
            let title_changed = self.tabs[index].refresh_detected_title(&self.config);
            if index == self.active_tab && title_changed {
                self.refresh_window_title();
            }
        }

        if self.tab_index(tab_id) == Some(self.active_tab) {
            self.dirty = true;
        }
    }

    pub fn handle_tab_exit(&mut self, tab_id: Option<TabId>) -> bool {
        if self.display.window.hold {
            return false;
        }

        let Some(index) = self.tab_index(tab_id) else {
            return false;
        };
        let closing_tab_id = self.tabs[index].id;
        let next_active_id = match self.config.tabs.tab_switch_strategy {
            TabSwitchStrategy::Previous => self.last_active_tab_id.filter(|tab_id| {
                self.tabs.iter().any(|tab| tab.id == *tab_id && tab.id != closing_tab_id)
            }),
            TabSwitchStrategy::Left => {
                index.checked_sub(1).and_then(|index| self.tabs.get(index)).map(|tab| tab.id)
            },
            TabSwitchStrategy::Right => self.tabs.get(index + 1).map(|tab| tab.id),
            TabSwitchStrategy::Last => self.tabs.last().map(|tab| tab.id),
        };

        self.tabs.remove(index);
        if self.tab_title_editor.as_ref().is_some_and(|editor| editor.tab_id == closing_tab_id) {
            self.tab_title_editor = None;
        }

        if self.tabs.is_empty() {
            return true;
        }

        self.active_tab = next_active_id
            .and_then(|tab_id| self.tabs.iter().position(|tab| tab.id == tab_id))
            .unwrap_or_else(|| {
                self.active_tab
                    .min(self.tabs.len() - 1)
                    .saturating_sub(usize::from(index < self.active_tab))
            });

        let config = self.config.clone();
        self.active_tab_mut().refresh_detected_title(&config);

        self.sync_focus();
        self.refresh_window_title();
        self.display.damage_tracker.frame().mark_fully_damaged();
        self.display.damage_tracker.next_frame().mark_fully_damaged();
        self.display.pending_update.dirty = true;
        self.dirty = true;

        false
    }

    pub fn clear_config_messages(&mut self, target: &str) {
        for tab in &mut self.tabs {
            tab.message_buffer.remove_target(target);
        }
    }

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

        let display = Display::new(window, gl_context, &config, false)?;

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

        let display = Display::new(window, gl_context, &config, tabbed)?;

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
        let preserve_title = options.window_identity.title.is_some();

        info!(
            "PTY dimensions: {:?} x {:?}",
            display.size_info.screen_lines(),
            display.size_info.columns()
        );

        let first_tab = TerminalTab::new(
            TabId(0),
            display.window.id(),
            display.size_info,
            &config,
            &options,
            &proxy,
        )?;

        // Create context for the Alacritty window.
        Ok(WindowContext {
            preserve_title,
            display,
            config,
            window_config: Default::default(),
            event_queue: Default::default(),
            modifiers: Default::default(),
            occluded: Default::default(),
            mouse: Default::default(),
            touch: Default::default(),
            dirty: Default::default(),
            tabs: vec![first_tab],
            active_tab: 0,
            next_tab_id: 1,
            last_active_tab_id: None,
            tab_title_editor: None,
            focused: false,
            event_proxy: proxy,
        })
    }

    /// Update the terminal window to the latest config.
    pub fn update_config(&mut self, new_config: Rc<UiConfig>) {
        let old_config = mem::replace(&mut self.config, new_config);

        // Apply ipc config if there are overrides.
        self.config = self.window_config.override_config_rc(self.config.clone());

        self.display.update_config(&self.config);
        for tab in &mut self.tabs {
            tab.terminal.lock().set_options(self.config.term_options());
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
        if !self.preserve_title
            && (!self.config.window.dynamic_title
                || self.display.window.title() == old_config.window.identity.title)
        {
            self.refresh_window_title();
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
        let event =
            Event::new(TerminalEvent::CursorBlinkingChange.into(), self.display.window.id());
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
        for tab in &mut self.tabs {
            tab.message_buffer.remove_target(LOG_TARGET_IPC_CONFIG);
        }

        self.window_config.clear();

        // Reload current config to pull new IPC config.
        self.update_config(config);
    }

    /// Add new window config overrides.
    #[cfg(unix)]
    pub fn add_window_config(&mut self, config: Rc<UiConfig>, options: &ParsedOptions) {
        // Clear previous window errors.
        for tab in &mut self.tabs {
            tab.message_buffer.remove_target(LOG_TARGET_IPC_CONFIG);
        }

        self.window_config.extend_from_slice(options);

        // Reload current config to pull new IPC config.
        self.update_config(config);
    }

    /// Draw the window.
    pub fn draw(&mut self, scheduler: &mut Scheduler) {
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

        // Redraw the window.
        let tab_titles: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let active = index == self.active_tab;
                (self.render_tab_title(index, tab, active), active)
            })
            .collect();
        let active_tab = self.active_tab;
        let (display, tabs, config) = (&mut self.display, &mut self.tabs, &self.config);
        let active_tab = &mut tabs[active_tab];
        let terminal = active_tab.terminal.lock();
        display.draw(
            terminal,
            scheduler,
            &active_tab.message_buffer,
            config,
            &mut active_tab.search_state,
            &tab_titles,
            self.tab_title_editor.as_ref().map(|editor| editor.value.as_str()),
        );
    }

    /// Process events for this terminal window.
    pub fn handle_event(
        &mut self,
        #[cfg(target_os = "macos")] event_loop: &ActiveEventLoop,
        _event_proxy: &EventLoopProxy<Event>,
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

        let old_is_searching = self.active_tab().search_state.history_index.is_some();
        let queued_events: Vec<_> = self.event_queue.drain(..).collect();

        for event in queued_events {
            match &event {
                WinitEvent::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    self.display.window.hold = false;
                    for tab in &mut self.tabs {
                        tab.terminal.lock().exit();
                    }
                    continue;
                },
                WinitEvent::WindowEvent { event: WindowEvent::Focused(is_focused), .. } => {
                    self.focused = *is_focused;
                },
                WinitEvent::UserEvent(Event { payload: EventType::Tab(action), .. }) => {
                    match action {
                        TabAction::Create => {
                            if let Err(err) = self.create_tab() {
                                log::error!("Could not create tab: {err:?}");
                            }
                        },
                        TabAction::Close => self.close_active_tab(),
                        TabAction::SelectNext => {
                            if !self.tabs.is_empty() {
                                self.set_active_tab((self.active_tab + 1) % self.tabs.len());
                            }
                        },
                        TabAction::SelectPrevious => {
                            if !self.tabs.is_empty() {
                                let next =
                                    (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
                                self.set_active_tab(next);
                            }
                        },
                        TabAction::Select(index) => self.set_active_tab(*index),
                        TabAction::SelectLast => {
                            if !self.tabs.is_empty() {
                                self.set_active_tab(self.tabs.len() - 1);
                            }
                        },
                        TabAction::MoveForward => self.move_active_tab(1),
                        TabAction::MoveBackward => self.move_active_tab(-1),
                        TabAction::SetTitle => self.start_tab_title_editor(),
                        TabAction::ConfirmTitle => self.confirm_tab_title_editor(),
                        TabAction::CancelTitle => self.cancel_tab_title_editor(),
                        TabAction::TitleInput(c) => self.tab_title_input(*c),
                        TabAction::TitlePopWord => self.tab_title_pop_word(),
                    }
                    continue;
                },
                _ => (),
            }

            let tab_index = match &event {
                WinitEvent::UserEvent(Event { tab_id, .. }) => {
                    self.tab_index(*tab_id).unwrap_or(self.active_tab)
                },
                _ => self.active_tab,
            };
            let is_active_tab = tab_index == self.active_tab;
            let tab = &mut self.tabs[tab_index];
            let mut terminal = tab.terminal.lock();

            let context = ActionContext {
                cursor_blink_timed_out: &mut tab.cursor_blink_timed_out,
                prev_bell_cmd: &mut tab.prev_bell_cmd,
                message_buffer: &mut tab.message_buffer,
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
                tab_terminal_title: &mut tab.terminal_title,
                tab_detected_title: &tab.detected_title,
                tab_custom_title: &mut tab.custom_title,
                tab_title_editor_active: self.tab_title_editor.is_some(),
                is_active_tab,
                #[cfg(not(windows))]
                master_fd: tab.master_fd,
                #[cfg(not(windows))]
                shell_pid: tab.shell_pid,
                preserve_title: self.preserve_title,
                config: &self.config,
                event_proxy: &self.event_proxy,
                #[cfg(target_os = "macos")]
                event_loop,
                clipboard,
                scheduler,
            };
            let mut processor = input::Processor::new(context);
            processor.handle_event(event);
        }

        self.sync_focus();

        // Process DisplayUpdate events.
        if self.display.pending_update.dirty {
            let tab_bar_lines = self.tab_bar_lines();
            let tab_bar_at_top = usize::from(self.tab_bar_at_top());
            let tab_title_editor_lines = usize::from(self.tab_title_editor.is_some());
            let active_index = self.active_tab;
            let (display, tabs, config) = (&mut self.display, &mut self.tabs, &self.config);
            let active_tab = &mut tabs[active_index];
            let mut terminal = active_tab.terminal.lock();
            Self::submit_display_update(
                &mut terminal,
                display,
                &mut active_tab.notifier,
                &active_tab.message_buffer,
                &mut active_tab.search_state,
                old_is_searching,
                config,
                tab_bar_lines,
                tab_bar_at_top,
                tab_title_editor_lines,
            );
            self.dirty = true;
        }

        if self.dirty || self.mouse.hint_highlight_dirty {
            let active_index = self.active_tab;
            let (display, tabs, config) = (&mut self.display, &mut self.tabs, &self.config);
            let active_tab = &mut tabs[active_index];
            let terminal = active_tab.terminal.lock();
            self.dirty |= display.update_highlighted_hints(
                &terminal,
                config,
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
    fn submit_display_update(
        terminal: &mut Term<EventProxy>,
        display: &mut Display,
        notifier: &mut Notifier,
        message_buffer: &MessageBuffer,
        search_state: &mut SearchState,
        old_is_searching: bool,
        config: &UiConfig,
        tab_bar_lines: usize,
        top_tab_bar_lines: usize,
        tab_title_editor_lines: usize,
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
            top_tab_bar_lines,
            tab_title_editor_lines,
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

impl Drop for WindowContext {
    fn drop(&mut self) {
        for tab in &mut self.tabs {
            let _ = tab.notifier.0.send(Msg::Shutdown);
        }
    }
}
