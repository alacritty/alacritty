//! Process window events.

use crate::ConfigMonitor;
use glutin::config::GetGlConfig;
use std::borrow::Cow;
use std::cmp::min;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::ffi::OsStr;
use std::fmt::Debug;
#[cfg(not(windows))]
use std::os::unix::io::RawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::rc::Rc;
#[cfg(unix)]
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{env, f32, mem};

use ahash::RandomState;
use crossfont::Size as FontSize;
use glutin::config::Config as GlutinConfig;
use glutin::display::GetGlDisplay;
use log::{debug, error, info, warn};
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, Event as WinitEvent, Ime, Modifiers, MouseButton, StartCause,
    Touch as TouchEvent, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop, EventLoopProxy};
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::WindowId;

use alacritty_terminal::event::{Event as TerminalEvent, EventListener, Notify};
use alacritty_terminal::event_loop::Notifier;
use alacritty_terminal::grid::{BidirectionalIterator, Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{self, ClipboardType, Term, TermMode};
use alacritty_terminal::vte::ansi::NamedColor;

#[cfg(unix)]
use crate::cli::{IpcConfig, ParsedOptions};
use crate::cli::{Options as CliOptions, WindowOptions};
use crate::clipboard::Clipboard;
use crate::config::ui_config::{HintAction, HintInternalAction};
use crate::config::{self, UiConfig};
#[cfg(not(windows))]
use crate::daemon::foreground_process_path;
use crate::daemon::spawn_daemon;
use crate::display::color::Rgb;
use crate::display::hint::HintMatch;
use crate::display::window::Window;
use crate::display::{Display, Preedit, SizeInfo};
use crate::input::{self, ActionContext as _, FONT_SIZE_STEP};
#[cfg(unix)]
use crate::ipc::{self, SocketReply};
use crate::logging::{LOG_TARGET_CONFIG, LOG_TARGET_WINIT};
use crate::message_bar::{Message, MessageBuffer};
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::window_context::WindowContext;

/// Duration after the last user input until an unlimited search is performed.
pub const TYPING_SEARCH_DELAY: Duration = Duration::from_millis(500);

/// Maximum number of lines for the blocking search while still typing the search regex.
const MAX_SEARCH_WHILE_TYPING: Option<usize> = Some(1000);

/// Maximum number of search terms stored in the history.
const MAX_SEARCH_HISTORY_SIZE: usize = 255;

/// Touch zoom speed.
const TOUCH_ZOOM_FACTOR: f32 = 0.01;

/// Cooldown between invocations of the bell command.
const BELL_CMD_COOLDOWN: Duration = Duration::from_millis(100);

/// The event processor.
///
/// Stores some state from received events and dispatches actions when they are
/// triggered.
pub struct Processor {
    pub config_monitor: Option<ConfigMonitor>,

    clipboard: Clipboard,
    scheduler: Scheduler,
    initial_window_options: Option<WindowOptions>,
    initial_window_error: Option<Box<dyn Error>>,
    windows: HashMap<WindowId, WindowContext, RandomState>,
    proxy: EventLoopProxy<Event>,
    gl_config: Option<GlutinConfig>,
    #[cfg(unix)]
    global_ipc_options: ParsedOptions,
    cli_options: CliOptions,
    config: Rc<UiConfig>,
}

impl Processor {
    /// Create a new event processor.
    pub fn new(
        config: UiConfig,
        cli_options: CliOptions,
        event_loop: &EventLoop<Event>,
    ) -> Processor {
        let proxy = event_loop.create_proxy();
        let scheduler = Scheduler::new(proxy.clone());
        let initial_window_options = Some(cli_options.window_options.clone());

        // Disable all device events, since we don't care about them.
        event_loop.listen_device_events(DeviceEvents::Never);

        // SAFETY: Since this takes a pointer to the winit event loop, it MUST be dropped first,
        // which is done in `loop_exiting`.
        let clipboard = unsafe { Clipboard::new(event_loop.display_handle().unwrap().as_raw()) };

        // Create a config monitor.
        //
        // The monitor watches the config file for changes and reloads it. Pending
        // config changes are processed in the main loop.
        let mut config_monitor = None;
        if config.live_config_reload() {
            config_monitor =
                ConfigMonitor::new(config.config_paths.clone(), event_loop.create_proxy());
        }

        Processor {
            initial_window_options,
            initial_window_error: None,
            cli_options,
            proxy,
            scheduler,
            gl_config: None,
            config: Rc::new(config),
            clipboard,
            windows: Default::default(),
            #[cfg(unix)]
            global_ipc_options: Default::default(),
            config_monitor,
        }
    }

    /// Create initial window and load GL platform.
    ///
    /// This will initialize the OpenGL Api and pick a config that
    /// will be used for the rest of the windows.
    pub fn create_initial_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_options: WindowOptions,
    ) -> Result<(), Box<dyn Error>> {
        let window_context = WindowContext::initial(
            event_loop,
            self.proxy.clone(),
            self.config.clone(),
            window_options,
        )?;

        self.gl_config = Some(window_context.display.gl_context().config());
        self.windows.insert(window_context.id(), window_context);

        Ok(())
    }

    /// Create a new terminal window.
    pub fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        options: WindowOptions,
    ) -> Result<(), Box<dyn Error>> {
        let gl_config = self.gl_config.as_ref().unwrap();

        // Override config with CLI/IPC options.
        let mut config_overrides = options.config_overrides();
        #[cfg(unix)]
        config_overrides.extend_from_slice(&self.global_ipc_options);
        let mut config = self.config.clone();
        config = config_overrides.override_config_rc(config);

        let window_context = WindowContext::additional(
            gl_config,
            event_loop,
            self.proxy.clone(),
            config,
            options,
            config_overrides,
        )?;

        self.windows.insert(window_context.id(), window_context);
        Ok(())
    }

    /// Run the event loop.
    ///
    /// The result is exit code generate from the loop.
    pub fn run(&mut self, event_loop: EventLoop<Event>) -> Result<(), Box<dyn Error>> {
        let result = event_loop.run_app(self);
        match self.initial_window_error.take() {
            Some(initial_window_error) => Err(initial_window_error),
            _ => result.map_err(Into::into),
        }
    }

    /// Check if an event is irrelevant and can be skipped.
    fn skip_window_event(event: &WindowEvent) -> bool {
        matches!(
            event,
            WindowEvent::KeyboardInput { is_synthetic: true, .. }
                | WindowEvent::ActivationTokenDone { .. }
                | WindowEvent::DoubleTapGesture { .. }
                | WindowEvent::TouchpadPressure { .. }
                | WindowEvent::RotationGesture { .. }
                | WindowEvent::PinchGesture { .. }
                | WindowEvent::AxisMotion { .. }
                | WindowEvent::PanGesture { .. }
                | WindowEvent::HoveredFileCancelled
                | WindowEvent::Destroyed
                | WindowEvent::ThemeChanged(_)
                | WindowEvent::HoveredFile(_)
                | WindowEvent::Moved(_)
        )
    }
}

impl ApplicationHandler<Event> for Processor {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if cause != StartCause::Init || self.cli_options.daemon {
            return;
        }

        if let Some(window_options) = self.initial_window_options.take() {
            if let Err(err) = self.create_initial_window(event_loop, window_options) {
                self.initial_window_error = Some(err);
                event_loop.exit();
                return;
            }
        }

        info!("Initialisation complete");
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.config.debug.print_events {
            info!(target: LOG_TARGET_WINIT, "{event:?}");
        }

        // Ignore all events we do not care about.
        if Self::skip_window_event(&event) {
            return;
        }

        let window_context = match self.windows.get_mut(&window_id) {
            Some(window_context) => window_context,
            None => return,
        };

        let is_redraw = matches!(event, WindowEvent::RedrawRequested);

        window_context.handle_event(
            #[cfg(target_os = "macos")]
            _event_loop,
            &self.proxy,
            &mut self.clipboard,
            &mut self.scheduler,
            WinitEvent::WindowEvent { window_id, event },
        );

        if is_redraw {
            window_context.draw(&mut self.scheduler);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        if self.config.debug.print_events {
            info!(target: LOG_TARGET_WINIT, "{event:?}");
        }

        // Handle events which don't mandate the WindowId.
        match (event.payload, event.window_id.as_ref()) {
            // Process IPC config update.
            #[cfg(unix)]
            (EventType::IpcConfig(ipc_config), window_id) => {
                // Try and parse options as toml.
                let mut options = ParsedOptions::from_options(&ipc_config.options);

                // Override IPC config for each window with matching ID.
                for (_, window_context) in self
                    .windows
                    .iter_mut()
                    .filter(|(id, _)| window_id.is_none() || window_id == Some(*id))
                {
                    if ipc_config.reset {
                        window_context.reset_window_config(self.config.clone());
                    } else {
                        window_context.add_window_config(self.config.clone(), &options);
                    }
                }

                // Persist global options for future windows.
                if window_id.is_none() {
                    if ipc_config.reset {
                        self.global_ipc_options.clear();
                    } else {
                        self.global_ipc_options.append(&mut options);
                    }
                }
            },
            // Process IPC config requests.
            #[cfg(unix)]
            (EventType::IpcGetConfig(stream), window_id) => {
                // Get the config for the requested window ID.
                let config = match self.windows.iter().find(|(id, _)| window_id == Some(*id)) {
                    Some((_, window_context)) => window_context.config(),
                    None => &self.global_ipc_options.override_config_rc(self.config.clone()),
                };

                // Convert config to JSON format.
                let config_json = match serde_json::to_string(&config) {
                    Ok(config_json) => config_json,
                    Err(err) => {
                        error!("Failed config serialization: {err}");
                        return;
                    },
                };

                // Send JSON config to the socket.
                if let Ok(mut stream) = stream.try_clone() {
                    ipc::send_reply(&mut stream, SocketReply::GetConfig(config_json));
                }
            },
            (EventType::ConfigReload(path), _) => {
                // Clear config logs from message bar for all terminals.
                for window_context in self.windows.values_mut() {
                    if !window_context.message_buffer.is_empty() {
                        window_context.message_buffer.remove_target(LOG_TARGET_CONFIG);
                        window_context.display.pending_update.dirty = true;
                    }
                }

                // Load config and update each terminal.
                if let Ok(config) = config::reload(&path, &mut self.cli_options) {
                    self.config = Rc::new(config);

                    // Restart config monitor if imports changed.
                    if let Some(monitor) = self.config_monitor.take() {
                        let paths = &self.config.config_paths;
                        self.config_monitor = if monitor.needs_restart(paths) {
                            monitor.shutdown();
                            ConfigMonitor::new(paths.clone(), self.proxy.clone())
                        } else {
                            Some(monitor)
                        };
                    }

                    for window_context in self.windows.values_mut() {
                        window_context.update_config(self.config.clone());
                    }
                }
            },
            // Create a new terminal window.
            (EventType::CreateWindow(options), _) => {
                // XXX Ensure that no context is current when creating a new window,
                // otherwise it may lock the backing buffer of the
                // surface of current context when asking
                // e.g. EGL on Wayland to create a new context.
                for window_context in self.windows.values_mut() {
                    window_context.display.make_not_current();
                }

                if self.gl_config.is_none() {
                    // Handle initial window creation in daemon mode.
                    if let Err(err) = self.create_initial_window(event_loop, options) {
                        self.initial_window_error = Some(err);
                        event_loop.exit();
                    }
                } else if let Err(err) = self.create_window(event_loop, options) {
                    error!("Could not open window: {err:?}");
                }
            },
            // Process events affecting all windows.
            (payload, None) => {
                let event = WinitEvent::UserEvent(Event::new(payload, None));
                for window_context in self.windows.values_mut() {
                    window_context.handle_event(
                        #[cfg(target_os = "macos")]
                        event_loop,
                        &self.proxy,
                        &mut self.clipboard,
                        &mut self.scheduler,
                        event.clone(),
                    );
                }
            },
            (EventType::Terminal(TerminalEvent::Wakeup), Some(window_id)) => {
                if let Some(window_context) = self.windows.get_mut(window_id) {
                    window_context.dirty = true;
                    if window_context.display.window.has_frame {
                        window_context.display.window.request_redraw();
                    }
                }
            },
            (EventType::Terminal(TerminalEvent::Exit), Some(window_id)) => {
                // Remove the closed terminal.
                let window_context = match self.windows.entry(*window_id) {
                    // Don't exit when terminal exits if user asked to hold the window.
                    Entry::Occupied(window_context)
                        if !window_context.get().display.window.hold =>
                    {
                        window_context.remove()
                    },
                    _ => return,
                };

                // Unschedule pending events.
                self.scheduler.unschedule_window(window_context.id());

                // Shutdown if no more terminals are open.
                if self.windows.is_empty() && !self.cli_options.daemon {
                    // Write ref tests of last window to disk.
                    if self.config.debug.ref_test {
                        window_context.write_ref_test_results();
                    }

                    event_loop.exit();
                }
            },
            // NOTE: This event bypasses batching to minimize input latency.
            (EventType::Frame, Some(window_id)) => {
                if let Some(window_context) = self.windows.get_mut(window_id) {
                    window_context.display.window.has_frame = true;
                    if window_context.dirty {
                        window_context.display.window.request_redraw();
                    }
                }
            },
            (payload, Some(window_id)) => {
                if let Some(window_context) = self.windows.get_mut(window_id) {
                    window_context.handle_event(
                        #[cfg(target_os = "macos")]
                        event_loop,
                        &self.proxy,
                        &mut self.clipboard,
                        &mut self.scheduler,
                        WinitEvent::UserEvent(Event::new(payload, *window_id)),
                    );
                }
            },
        };
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.config.debug.print_events {
            info!(target: LOG_TARGET_WINIT, "About to wait");
        }

        // Dispatch event to all windows.
        for window_context in self.windows.values_mut() {
            window_context.handle_event(
                #[cfg(target_os = "macos")]
                event_loop,
                &self.proxy,
                &mut self.clipboard,
                &mut self.scheduler,
                WinitEvent::AboutToWait,
            );
        }

        // Update the scheduler after event processing to ensure
        // the event loop deadline is as accurate as possible.
        let control_flow = match self.scheduler.update() {
            Some(instant) => ControlFlow::WaitUntil(instant),
            None => ControlFlow::Wait,
        };
        event_loop.set_control_flow(control_flow);
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if self.config.debug.print_events {
            info!("Exiting the event loop");
        }

        match self.gl_config.take().map(|config| config.display()) {
            #[cfg(not(target_os = "macos"))]
            Some(glutin::display::Display::Egl(display)) => {
                // Ensure that all the windows are dropped, so the destructors for
                // Renderer and contexts ran.
                self.windows.clear();

                // SAFETY: the display is being destroyed after destroying all the
                // windows, thus no attempt to access the EGL state will be made.
                unsafe {
                    display.terminate();
                }
            },
            _ => (),
        }

        // SAFETY: The clipboard must be dropped before the event loop, so use the nop clipboard
        // as a safe placeholder.
        self.clipboard = Clipboard::new_nop();
    }
}

/// Alacritty events.
#[derive(Debug, Clone)]
pub struct Event {
    /// Limit event to a specific window.
    window_id: Option<WindowId>,

    /// Event payload.
    payload: EventType,
}

impl Event {
    pub fn new<I: Into<Option<WindowId>>>(payload: EventType, window_id: I) -> Self {
        Self { window_id: window_id.into(), payload }
    }
}

impl From<Event> for WinitEvent<Event> {
    fn from(event: Event) -> Self {
        WinitEvent::UserEvent(event)
    }
}

/// Alacritty events.
#[derive(Debug, Clone)]
pub enum EventType {
    Terminal(TerminalEvent),
    ConfigReload(PathBuf),
    Message(Message),
    Scroll(Scroll),
    CreateWindow(WindowOptions),
    #[cfg(unix)]
    IpcConfig(IpcConfig),
    #[cfg(unix)]
    IpcGetConfig(Arc<UnixStream>),
    BlinkCursor,
    BlinkCursorTimeout,
    SearchNext,
    Frame,
}

impl From<TerminalEvent> for EventType {
    fn from(event: TerminalEvent) -> Self {
        Self::Terminal(event)
    }
}

/// Regex search state.
pub struct SearchState {
    /// Search direction.
    pub direction: Direction,

    /// Current position in the search history.
    pub history_index: Option<usize>,

    /// Change in display offset since the beginning of the search.
    display_offset_delta: i32,

    /// Search origin in viewport coordinates relative to original display offset.
    origin: Point,

    /// Focused match during active search.
    focused_match: Option<Match>,

    /// Search regex and history.
    ///
    /// During an active search, the first element is the user's current input.
    ///
    /// While going through history, the [`SearchState::history_index`] will point to the element
    /// in history which is currently being previewed.
    history: VecDeque<String>,

    /// Compiled search automatons.
    dfas: Option<RegexSearch>,
}

impl SearchState {
    /// Search regex text if a search is active.
    pub fn regex(&self) -> Option<&String> {
        self.history_index.and_then(|index| self.history.get(index))
    }

    /// Direction of the search from the search origin.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Focused match during vi-less search.
    pub fn focused_match(&self) -> Option<&Match> {
        self.focused_match.as_ref()
    }

    /// Clear the focused match.
    pub fn clear_focused_match(&mut self) {
        self.focused_match = None;
    }

    /// Active search dfas.
    pub fn dfas(&mut self) -> Option<&mut RegexSearch> {
        self.dfas.as_mut()
    }

    /// Search regex text if a search is active.
    fn regex_mut(&mut self) -> Option<&mut String> {
        self.history_index.and_then(move |index| self.history.get_mut(index))
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            direction: Direction::Right,
            display_offset_delta: Default::default(),
            focused_match: Default::default(),
            history_index: Default::default(),
            history: Default::default(),
            origin: Default::default(),
            dfas: Default::default(),
        }
    }
}

/// Vi inline search state.
pub struct InlineSearchState {
    /// Whether inline search is currently waiting for search character input.
    pub char_pending: bool,
    pub character: Option<char>,

    direction: Direction,
    stop_short: bool,
}

impl Default for InlineSearchState {
    fn default() -> Self {
        Self {
            direction: Direction::Right,
            char_pending: Default::default(),
            stop_short: Default::default(),
            character: Default::default(),
        }
    }
}

pub struct ActionContext<'a, N, T> {
    pub notifier: &'a mut N,
    pub terminal: &'a mut Term<T>,
    pub clipboard: &'a mut Clipboard,
    pub mouse: &'a mut Mouse,
    pub touch: &'a mut TouchPurpose,
    pub modifiers: &'a mut Modifiers,
    pub display: &'a mut Display,
    pub message_buffer: &'a mut MessageBuffer,
    pub config: &'a UiConfig,
    pub cursor_blink_timed_out: &'a mut bool,
    pub prev_bell_cmd: &'a mut Option<Instant>,
    #[cfg(target_os = "macos")]
    pub event_loop: &'a ActiveEventLoop,
    pub event_proxy: &'a EventLoopProxy<Event>,
    pub scheduler: &'a mut Scheduler,
    pub search_state: &'a mut SearchState,
    pub inline_search_state: &'a mut InlineSearchState,
    pub dirty: &'a mut bool,
    pub occluded: &'a mut bool,
    pub preserve_title: bool,
    #[cfg(not(windows))]
    pub master_fd: RawFd,
    #[cfg(not(windows))]
    pub shell_pid: u32,
}

impl<'a, N: Notify + 'a, T: EventListener> input::ActionContext<T> for ActionContext<'a, N, T> {
    #[inline]
    fn write_to_pty<B: Into<Cow<'static, [u8]>>>(&self, val: B) {
        self.notifier.notify(val);
    }

    /// Request a redraw.
    #[inline]
    fn mark_dirty(&mut self) {
        *self.dirty = true;
    }

    #[inline]
    fn size_info(&self) -> SizeInfo {
        self.display.size_info
    }

    fn scroll(&mut self, scroll: Scroll) {
        let old_offset = self.terminal.grid().display_offset() as i32;

        let old_vi_cursor = self.terminal.vi_mode_cursor;
        self.terminal.scroll_display(scroll);

        let lines_changed = old_offset - self.terminal.grid().display_offset() as i32;

        // Keep track of manual display offset changes during search.
        if self.search_active() {
            self.search_state.display_offset_delta += lines_changed;
        }

        let vi_mode = self.terminal.mode().contains(TermMode::VI);

        // Update selection.
        if vi_mode && self.terminal.selection.as_ref().is_some_and(|s| !s.is_empty()) {
            self.update_selection(self.terminal.vi_mode_cursor.point, Side::Right);
        } else if self.mouse.left_button_state == ElementState::Pressed
            || self.mouse.right_button_state == ElementState::Pressed
        {
            let display_offset = self.terminal.grid().display_offset();
            let point = self.mouse.point(&self.size_info(), display_offset);
            self.update_selection(point, self.mouse.cell_side);
        }

        // Scrolling inside Vi mode moves the cursor, so start typing.
        if vi_mode {
            self.on_typing_start();
        }

        // Update dirty if actually scrolled or moved Vi cursor in Vi mode.
        *self.dirty |=
            lines_changed != 0 || (vi_mode && old_vi_cursor != self.terminal.vi_mode_cursor);
    }

    // Copy text selection.
    fn copy_selection(&mut self, ty: ClipboardType) {
        let text = match self.terminal.selection_to_string().filter(|s| !s.is_empty()) {
            Some(text) => text,
            None => return,
        };

        if ty == ClipboardType::Selection && self.config.selection.save_to_clipboard {
            self.clipboard.store(ClipboardType::Clipboard, text.clone());
        }
        self.clipboard.store(ty, text);
    }

    fn selection_is_empty(&self) -> bool {
        self.terminal.selection.as_ref().is_none_or(Selection::is_empty)
    }

    fn clear_selection(&mut self) {
        // Clear the selection on the terminal.
        let selection = self.terminal.selection.take();
        // Mark the terminal as dirty when selection wasn't empty.
        *self.dirty |= selection.is_some_and(|s| !s.is_empty());
    }

    fn update_selection(&mut self, mut point: Point, side: Side) {
        let mut selection = match self.terminal.selection.take() {
            Some(selection) => selection,
            None => return,
        };

        // Treat motion over message bar like motion over the last line.
        point.line = min(point.line, self.terminal.bottommost_line());

        // Update selection.
        selection.update(point, side);

        // Move vi cursor and expand selection.
        if self.terminal.mode().contains(TermMode::VI) && !self.search_active() {
            self.terminal.vi_mode_cursor.point = point;
            selection.include_all();
        }

        self.terminal.selection = Some(selection);
        *self.dirty = true;
    }

    fn start_selection(&mut self, ty: SelectionType, point: Point, side: Side) {
        self.terminal.selection = Some(Selection::new(ty, point, side));
        *self.dirty = true;

        self.copy_selection(ClipboardType::Selection);
    }

    fn toggle_selection(&mut self, ty: SelectionType, point: Point, side: Side) {
        match &mut self.terminal.selection {
            Some(selection) if selection.ty == ty && !selection.is_empty() => {
                self.clear_selection();
            },
            Some(selection) if !selection.is_empty() => {
                selection.ty = ty;
                *self.dirty = true;

                self.copy_selection(ClipboardType::Selection);
            },
            _ => self.start_selection(ty, point, side),
        }
    }

    #[inline]
    fn mouse_mode(&self) -> bool {
        self.terminal.mode().intersects(TermMode::MOUSE_MODE)
            && !self.terminal.mode().contains(TermMode::VI)
    }

    #[inline]
    fn mouse_mut(&mut self) -> &mut Mouse {
        self.mouse
    }

    #[inline]
    fn mouse(&self) -> &Mouse {
        self.mouse
    }

    #[inline]
    fn touch_purpose(&mut self) -> &mut TouchPurpose {
        self.touch
    }

    #[inline]
    fn modifiers(&mut self) -> &mut Modifiers {
        self.modifiers
    }

    #[inline]
    fn window(&mut self) -> &mut Window {
        &mut self.display.window
    }

    #[inline]
    fn display(&mut self) -> &mut Display {
        self.display
    }

    #[inline]
    fn terminal(&self) -> &Term<T> {
        self.terminal
    }

    #[inline]
    fn terminal_mut(&mut self) -> &mut Term<T> {
        self.terminal
    }

    fn spawn_new_instance(&mut self) {
        let mut env_args = env::args();
        let alacritty = env_args.next().unwrap();

        let mut args: Vec<String> = Vec::new();

        // Reuse the arguments passed to Alacritty for the new instance.
        #[allow(clippy::while_let_on_iterator)]
        while let Some(arg) = env_args.next() {
            // New instances shouldn't inherit command.
            if arg == "-e" || arg == "--command" {
                break;
            }

            // On unix, the working directory of the foreground shell is used by `start_daemon`.
            #[cfg(not(windows))]
            if arg == "--working-directory" {
                let _ = env_args.next();
                continue;
            }

            args.push(arg);
        }

        self.spawn_daemon(&alacritty, &args);
    }

    #[cfg(not(windows))]
    fn create_new_window(&mut self, #[cfg(target_os = "macos")] tabbing_id: Option<String>) {
        let mut options = WindowOptions::default();
        options.terminal_options.working_directory =
            foreground_process_path(self.master_fd, self.shell_pid).ok();

        #[cfg(target_os = "macos")]
        {
            options.window_tabbing_id = tabbing_id;
        }

        let _ = self.event_proxy.send_event(Event::new(EventType::CreateWindow(options), None));
    }

    #[cfg(windows)]
    fn create_new_window(&mut self) {
        let _ = self
            .event_proxy
            .send_event(Event::new(EventType::CreateWindow(WindowOptions::default()), None));
    }

    fn spawn_daemon<I, S>(&self, program: &str, args: I)
    where
        I: IntoIterator<Item = S> + Debug + Copy,
        S: AsRef<OsStr>,
    {
        #[cfg(not(windows))]
        let result = spawn_daemon(program, args, self.master_fd, self.shell_pid);
        #[cfg(windows)]
        let result = spawn_daemon(program, args);

        match result {
            Ok(_) => debug!("Launched {program} with args {args:?}"),
            Err(err) => warn!("Unable to launch {program} with args {args:?}: {err}"),
        }
    }

    fn change_font_size(&mut self, delta: f32) {
        // Round to pick integral px steps, since fonts look better on them.
        let new_size = self.display.font_size.as_px().round() + delta;
        self.display.font_size = FontSize::from_px(new_size);
        let font = self.config.font.clone().with_size(self.display.font_size);
        self.display.pending_update.set_font(font);
    }

    fn reset_font_size(&mut self) {
        let scale_factor = self.display.window.scale_factor as f32;
        self.display.font_size = self.config.font.size().scale(scale_factor);
        self.display
            .pending_update
            .set_font(self.config.font.clone().with_size(self.display.font_size));
    }

    #[inline]
    fn pop_message(&mut self) {
        if !self.message_buffer.is_empty() {
            self.display.pending_update.dirty = true;
            self.message_buffer.pop();
        }
    }

    #[inline]
    fn start_search(&mut self, direction: Direction) {
        // Only create new history entry if the previous regex wasn't empty.
        if self.search_state.history.front().is_none_or(|regex| !regex.is_empty()) {
            self.search_state.history.push_front(String::new());
            self.search_state.history.truncate(MAX_SEARCH_HISTORY_SIZE);
        }

        self.search_state.history_index = Some(0);
        self.search_state.direction = direction;
        self.search_state.focused_match = None;

        // Store original search position as origin and reset location.
        if self.terminal.mode().contains(TermMode::VI) {
            self.search_state.origin = self.terminal.vi_mode_cursor.point;
            self.search_state.display_offset_delta = 0;

            // Adjust origin for content moving upward on search start.
            if self.terminal.grid().cursor.point.line + 1 == self.terminal.screen_lines() {
                self.search_state.origin.line -= 1;
            }
        } else {
            let viewport_top = Line(-(self.terminal.grid().display_offset() as i32)) - 1;
            let viewport_bottom = viewport_top + self.terminal.bottommost_line();
            let last_column = self.terminal.last_column();
            self.search_state.origin = match direction {
                Direction::Right => Point::new(viewport_top, Column(0)),
                Direction::Left => Point::new(viewport_bottom, last_column),
            };
        }

        // Enable IME so we can input into the search bar with it if we were in Vi mode.
        self.window().set_text_input_active(true);

        self.display.damage_tracker.frame().mark_fully_damaged();
        self.display.pending_update.dirty = true;
    }

    #[inline]
    fn start_seeded_search(&mut self, direction: Direction, text: String) {
        let origin = self.terminal.vi_mode_cursor.point;

        // Start new search.
        self.clear_selection();
        self.start_search(direction);

        // Enter initial selection text.
        for c in text.chars() {
            if let '$' | '('..='+' | '?' | '['..='^' | '{'..='}' = c {
                self.search_input('\\');
            }
            self.search_input(c);
        }

        // Leave search mode.
        self.confirm_search();

        if !self.terminal.mode().contains(TermMode::VI) {
            return;
        }

        // Find the target vi cursor point by going to the next match to the right of the origin,
        // then jump to the next search match in the target direction.
        let target = self.search_next(origin, Direction::Right, Side::Right).and_then(|rm| {
            let regex_match = match direction {
                Direction::Right => {
                    let origin = rm.end().add(self.terminal, Boundary::None, 1);
                    self.search_next(origin, Direction::Right, Side::Left)?
                },
                Direction::Left => {
                    let origin = rm.start().sub(self.terminal, Boundary::None, 1);
                    self.search_next(origin, Direction::Left, Side::Left)?
                },
            };
            Some(*regex_match.start())
        });

        // Move the vi cursor to the target position.
        if let Some(target) = target {
            self.terminal_mut().vi_goto_point(target);
            self.mark_dirty();
        }
    }

    #[inline]
    fn confirm_search(&mut self) {
        // Just cancel search when not in vi mode.
        if !self.terminal.mode().contains(TermMode::VI) {
            self.cancel_search();
            return;
        }

        // Force unlimited search if the previous one was interrupted.
        let timer_id = TimerId::new(Topic::DelayedSearch, self.display.window.id());
        if self.scheduler.scheduled(timer_id) {
            self.goto_match(None);
        }

        self.exit_search();
    }

    #[inline]
    fn cancel_search(&mut self) {
        if self.terminal.mode().contains(TermMode::VI) {
            // Recover pre-search state in vi mode.
            self.search_reset_state();
        } else if let Some(focused_match) = &self.search_state.focused_match {
            // Create a selection for the focused match.
            let start = *focused_match.start();
            let end = *focused_match.end();
            self.start_selection(SelectionType::Simple, start, Side::Left);
            self.update_selection(end, Side::Right);
            self.copy_selection(ClipboardType::Selection);
        }

        self.search_state.dfas = None;

        self.exit_search();
    }

    #[inline]
    fn search_input(&mut self, c: char) {
        match self.search_state.history_index {
            Some(0) => (),
            // When currently in history, replace active regex with history on change.
            Some(index) => {
                self.search_state.history[0] = self.search_state.history[index].clone();
                self.search_state.history_index = Some(0);
            },
            None => return,
        }
        let regex = &mut self.search_state.history[0];

        match c {
            // Handle backspace/ctrl+h.
            '\x08' | '\x7f' => {
                let _ = regex.pop();
            },
            // Add ascii and unicode text.
            ' '..='~' | '\u{a0}'..='\u{10ffff}' => regex.push(c),
            // Ignore non-printable characters.
            _ => return,
        }

        if !self.terminal.mode().contains(TermMode::VI) {
            // Clear selection so we do not obstruct any matches.
            self.terminal.selection = None;
        }

        self.update_search();
    }

    #[inline]
    fn search_pop_word(&mut self) {
        if let Some(regex) = self.search_state.regex_mut() {
            *regex = regex.trim_end().to_owned();
            regex.truncate(regex.rfind(' ').map_or(0, |i| i + 1));
            self.update_search();
        }
    }

    /// Go to the previous regex in the search history.
    #[inline]
    fn search_history_previous(&mut self) {
        let index = match &mut self.search_state.history_index {
            None => return,
            Some(index) if *index + 1 >= self.search_state.history.len() => return,
            Some(index) => index,
        };

        *index += 1;
        self.update_search();
    }

    /// Go to the previous regex in the search history.
    #[inline]
    fn search_history_next(&mut self) {
        let index = match &mut self.search_state.history_index {
            Some(0) | None => return,
            Some(index) => index,
        };

        *index -= 1;
        self.update_search();
    }

    #[inline]
    fn advance_search_origin(&mut self, direction: Direction) {
        // Use focused match as new search origin if available.
        if let Some(focused_match) = &self.search_state.focused_match {
            let new_origin = match direction {
                Direction::Right => focused_match.end().add(self.terminal, Boundary::None, 1),
                Direction::Left => focused_match.start().sub(self.terminal, Boundary::None, 1),
            };

            self.terminal.scroll_to_point(new_origin);

            self.search_state.display_offset_delta = 0;
            self.search_state.origin = new_origin;
        }

        // Search for the next match using the supplied direction.
        let search_direction = mem::replace(&mut self.search_state.direction, direction);
        self.goto_match(None);
        self.search_state.direction = search_direction;

        // If we found a match, we set the search origin right in front of it to make sure that
        // after modifications to the regex the search is started without moving the focused match
        // around.
        let focused_match = match &self.search_state.focused_match {
            Some(focused_match) => focused_match,
            None => return,
        };

        // Set new origin to the left/right of the match, depending on search direction.
        let new_origin = match self.search_state.direction {
            Direction::Right => *focused_match.start(),
            Direction::Left => *focused_match.end(),
        };

        // Store the search origin with display offset by checking how far we need to scroll to it.
        let old_display_offset = self.terminal.grid().display_offset() as i32;
        self.terminal.scroll_to_point(new_origin);
        let new_display_offset = self.terminal.grid().display_offset() as i32;
        self.search_state.display_offset_delta = new_display_offset - old_display_offset;

        // Store origin and scroll back to the match.
        self.terminal.scroll_display(Scroll::Delta(-self.search_state.display_offset_delta));
        self.search_state.origin = new_origin;
    }

    /// Find the next search match.
    fn search_next(&mut self, origin: Point, direction: Direction, side: Side) -> Option<Match> {
        self.search_state
            .dfas
            .as_mut()
            .and_then(|dfas| self.terminal.search_next(dfas, origin, direction, side, None))
    }

    #[inline]
    fn search_direction(&self) -> Direction {
        self.search_state.direction
    }

    #[inline]
    fn search_active(&self) -> bool {
        self.search_state.history_index.is_some()
    }

    /// Handle keyboard typing start.
    ///
    /// This will temporarily disable some features like terminal cursor blinking or the mouse
    /// cursor.
    ///
    /// All features are re-enabled again automatically.
    #[inline]
    fn on_typing_start(&mut self) {
        // Disable cursor blinking.
        let timer_id = TimerId::new(Topic::BlinkCursor, self.display.window.id());
        if self.scheduler.unschedule(timer_id).is_some() {
            self.schedule_blinking();

            // Mark the cursor as visible and queue redraw if the cursor was hidden.
            if mem::take(&mut self.display.cursor_hidden) {
                *self.dirty = true;
            }
        } else if *self.cursor_blink_timed_out {
            self.update_cursor_blinking();
        }

        // Hide mouse cursor.
        if self.config.mouse.hide_when_typing && self.display.window.mouse_visible() {
            self.display.window.set_mouse_visible(false);

            // Request hint highlights update, since the mouse may have been hovering a hint.
            self.mouse.hint_highlight_dirty = true
        }
    }

    /// Process a new character for keyboard hints.
    fn hint_input(&mut self, c: char) {
        if let Some(hint) = self.display.hint_state.keyboard_input(self.terminal, c) {
            self.mouse.block_hint_launcher = false;
            self.trigger_hint(&hint);
        }
        *self.dirty = true;
    }

    /// Trigger a hint action.
    fn trigger_hint(&mut self, hint: &HintMatch) {
        if self.mouse.block_hint_launcher {
            return;
        }

        let hint_bounds = hint.bounds();
        let text = match hint.text(self.terminal) {
            Some(text) => text,
            None => return,
        };

        match &hint.action() {
            // Launch an external program.
            HintAction::Command(command) => {
                let mut args = command.args().to_vec();
                args.push(text.into());
                self.spawn_daemon(command.program(), &args);
            },
            // Copy the text to the clipboard.
            HintAction::Action(HintInternalAction::Copy) => {
                self.clipboard.store(ClipboardType::Clipboard, text);
            },
            // Write the text to the PTY/search.
            HintAction::Action(HintInternalAction::Paste) => self.paste(&text, true),
            // Select the text.
            HintAction::Action(HintInternalAction::Select) => {
                self.start_selection(SelectionType::Simple, *hint_bounds.start(), Side::Left);
                self.update_selection(*hint_bounds.end(), Side::Right);
                self.copy_selection(ClipboardType::Selection);
            },
            // Move the vi mode cursor.
            HintAction::Action(HintInternalAction::MoveViModeCursor) => {
                // Enter vi mode if we're not in it already.
                if !self.terminal.mode().contains(TermMode::VI) {
                    self.terminal.toggle_vi_mode();
                }

                self.terminal.vi_goto_point(*hint_bounds.start());
                self.mark_dirty();
            },
        }
    }

    /// Expand the selection to the current mouse cursor position.
    #[inline]
    fn expand_selection(&mut self) {
        let control = self.modifiers().state().control_key();
        let selection_type = match self.mouse().click_state {
            ClickState::None => return,
            _ if control => SelectionType::Block,
            ClickState::Click => SelectionType::Simple,
            ClickState::DoubleClick => SelectionType::Semantic,
            ClickState::TripleClick => SelectionType::Lines,
        };

        // Load mouse point, treating message bar and padding as the closest cell.
        let display_offset = self.terminal().grid().display_offset();
        let point = self.mouse().point(&self.size_info(), display_offset);

        let cell_side = self.mouse().cell_side;

        let selection = match &mut self.terminal_mut().selection {
            Some(selection) => selection,
            None => return,
        };

        selection.ty = selection_type;
        self.update_selection(point, cell_side);

        // Move vi mode cursor to mouse click position.
        if self.terminal().mode().contains(TermMode::VI) && !self.search_active() {
            self.terminal_mut().vi_mode_cursor.point = point;
        }
    }

    /// Get the semantic word at the specified point.
    fn semantic_word(&self, point: Point) -> String {
        let terminal = self.terminal();
        let grid = terminal.grid();

        // Find the next semantic word boundary to the right.
        let mut end = terminal.semantic_search_right(point);

        // Get point at which skipping over semantic characters has led us back to the
        // original character.
        let start_cell = &grid[point];
        let search_end = if start_cell.flags.intersects(Flags::LEADING_WIDE_CHAR_SPACER) {
            point.add(terminal, Boundary::None, 2)
        } else if start_cell.flags.intersects(Flags::WIDE_CHAR) {
            point.add(terminal, Boundary::None, 1)
        } else {
            point
        };

        // Keep moving until we're not on top of a semantic escape character.
        let semantic_chars = terminal.semantic_escape_chars();
        loop {
            let cell = &grid[end];

            // Get cell's character, taking wide characters into account.
            let c = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                grid[end.sub(terminal, Boundary::None, 1)].c
            } else {
                cell.c
            };

            if !semantic_chars.contains(c) {
                break;
            }

            end = terminal.semantic_search_right(end.add(terminal, Boundary::None, 1));

            // Stop if the entire grid is only semantic escape characters.
            if end == search_end {
                return String::new();
            }
        }

        // Find the beginning of the semantic word.
        let start = terminal.semantic_search_left(end);

        terminal.bounds_to_string(start, end)
    }

    /// Handle beginning of terminal text input.
    fn on_terminal_input_start(&mut self) {
        self.on_typing_start();
        self.clear_selection();

        if self.terminal().grid().display_offset() != 0 {
            self.scroll(Scroll::Bottom);
        }
    }

    /// Paste a text into the terminal.
    fn paste(&mut self, text: &str, bracketed: bool) {
        if self.search_active() {
            for c in text.chars() {
                self.search_input(c);
            }
        } else if self.inline_search_state.char_pending {
            self.inline_search_input(text);
        } else if bracketed && self.terminal().mode().contains(TermMode::BRACKETED_PASTE) {
            self.on_terminal_input_start();

            self.write_to_pty(&b"\x1b[200~"[..]);

            // Write filtered escape sequences.
            //
            // We remove `\x1b` to ensure it's impossible for the pasted text to write the bracketed
            // paste end escape `\x1b[201~` and `\x03` since some shells incorrectly terminate
            // bracketed paste when they receive it.
            let filtered = text.replace(['\x1b', '\x03'], "");
            self.write_to_pty(filtered.into_bytes());

            self.write_to_pty(&b"\x1b[201~"[..]);
        } else {
            self.on_terminal_input_start();

            let payload = if bracketed {
                // In non-bracketed (ie: normal) mode, terminal applications cannot distinguish
                // pasted data from keystrokes.
                //
                // In theory, we should construct the keystrokes needed to produce the data we are
                // pasting... since that's neither practical nor sensible (and probably an
                // impossible task to solve in a general way), we'll just replace line breaks
                // (windows and unix style) with a single carriage return (\r, which is what the
                // Enter key produces).
                text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
            } else {
                // When we explicitly disable bracketed paste don't manipulate with the input,
                // so we pass user input as is.
                text.to_owned().into_bytes()
            };

            self.write_to_pty(payload);
        }
    }

    /// Toggle the vi mode status.
    #[inline]
    fn toggle_vi_mode(&mut self) {
        let was_in_vi_mode = self.terminal.mode().contains(TermMode::VI);
        if was_in_vi_mode {
            // If we had search running when leaving Vi mode we should mark terminal fully damaged
            // to cleanup highlighted results.
            if self.search_state.dfas.take().is_some() {
                self.display.damage_tracker.frame().mark_fully_damaged();
            }
        } else {
            self.clear_selection();
        }

        if self.search_active() {
            self.cancel_search();
        }

        // We don't want IME in Vi mode.
        self.window().set_text_input_active(was_in_vi_mode);

        self.terminal.toggle_vi_mode();

        *self.dirty = true;
    }

    /// Get vi inline search state.
    fn inline_search_state(&mut self) -> &mut InlineSearchState {
        self.inline_search_state
    }

    /// Start vi mode inline search.
    fn start_inline_search(&mut self, direction: Direction, stop_short: bool) {
        self.inline_search_state.stop_short = stop_short;
        self.inline_search_state.direction = direction;
        self.inline_search_state.char_pending = true;
        self.inline_search_state.character = None;
    }

    /// Jump to the next matching character in the line.
    fn inline_search_next(&mut self) {
        let direction = self.inline_search_state.direction;
        self.inline_search(direction);
    }

    /// Jump to the next matching character in the line.
    fn inline_search_previous(&mut self) {
        let direction = self.inline_search_state.direction.opposite();
        self.inline_search(direction);
    }

    /// Process input during inline search.
    fn inline_search_input(&mut self, text: &str) {
        // Ignore input with empty text, like modifier keys.
        let c = match text.chars().next() {
            Some(c) => c,
            None => return,
        };

        self.inline_search_state.char_pending = false;
        self.inline_search_state.character = Some(c);
        self.window().set_text_input_active(false);

        // Immediately move to the captured character.
        self.inline_search_next();
    }

    fn message(&self) -> Option<&Message> {
        self.message_buffer.message()
    }

    fn config(&self) -> &UiConfig {
        self.config
    }

    #[cfg(target_os = "macos")]
    fn event_loop(&self) -> &ActiveEventLoop {
        self.event_loop
    }

    fn clipboard_mut(&mut self) -> &mut Clipboard {
        self.clipboard
    }

    fn scheduler_mut(&mut self) -> &mut Scheduler {
        self.scheduler
    }
}

impl<'a, N: Notify + 'a, T: EventListener> ActionContext<'a, N, T> {
    fn update_search(&mut self) {
        let regex = match self.search_state.regex() {
            Some(regex) => regex,
            None => return,
        };

        // Hide cursor while typing into the search bar.
        if self.config.mouse.hide_when_typing {
            self.display.window.set_mouse_visible(false);
        }

        if regex.is_empty() {
            // Stop search if there's nothing to search for.
            self.search_reset_state();
            self.search_state.dfas = None;
        } else {
            // Create search dfas for the new regex string.
            self.search_state.dfas = RegexSearch::new(regex).ok();

            // Update search highlighting.
            self.goto_match(MAX_SEARCH_WHILE_TYPING);
        }

        *self.dirty = true;
    }

    /// Reset terminal to the state before search was started.
    fn search_reset_state(&mut self) {
        // Unschedule pending timers.
        let timer_id = TimerId::new(Topic::DelayedSearch, self.display.window.id());
        self.scheduler.unschedule(timer_id);

        // Clear focused match.
        self.search_state.focused_match = None;

        // The viewport reset logic is only needed for vi mode, since without it our origin is
        // always at the current display offset instead of at the vi cursor position which we need
        // to recover to.
        if !self.terminal.mode().contains(TermMode::VI) {
            return;
        }

        // Reset display offset and cursor position.
        self.terminal.vi_mode_cursor.point = self.search_state.origin;
        self.terminal.scroll_display(Scroll::Delta(self.search_state.display_offset_delta));
        self.search_state.display_offset_delta = 0;

        *self.dirty = true;
    }

    /// Jump to the first regex match from the search origin.
    fn goto_match(&mut self, mut limit: Option<usize>) {
        let dfas = match &mut self.search_state.dfas {
            Some(dfas) => dfas,
            None => return,
        };

        // Limit search only when enough lines are available to run into the limit.
        limit = limit.filter(|&limit| limit <= self.terminal.total_lines());

        // Jump to the next match.
        let direction = self.search_state.direction;
        let clamped_origin = self.search_state.origin.grid_clamp(self.terminal, Boundary::Grid);
        match self.terminal.search_next(dfas, clamped_origin, direction, Side::Left, limit) {
            Some(regex_match) => {
                let old_offset = self.terminal.grid().display_offset() as i32;

                if self.terminal.mode().contains(TermMode::VI) {
                    // Move vi cursor to the start of the match.
                    self.terminal.vi_goto_point(*regex_match.start());
                } else {
                    // Select the match when vi mode is not active.
                    self.terminal.scroll_to_point(*regex_match.start());
                }

                // Update the focused match.
                self.search_state.focused_match = Some(regex_match);

                // Store number of lines the viewport had to be moved.
                let display_offset = self.terminal.grid().display_offset();
                self.search_state.display_offset_delta += old_offset - display_offset as i32;

                // Since we found a result, we require no delayed re-search.
                let timer_id = TimerId::new(Topic::DelayedSearch, self.display.window.id());
                self.scheduler.unschedule(timer_id);
            },
            // Reset viewport only when we know there is no match, to prevent unnecessary jumping.
            None if limit.is_none() => self.search_reset_state(),
            None => {
                // Schedule delayed search if we ran into our search limit.
                let timer_id = TimerId::new(Topic::DelayedSearch, self.display.window.id());
                if !self.scheduler.scheduled(timer_id) {
                    let event = Event::new(EventType::SearchNext, self.display.window.id());
                    self.scheduler.schedule(event, TYPING_SEARCH_DELAY, false, timer_id);
                }

                // Clear focused match.
                self.search_state.focused_match = None;
            },
        }

        *self.dirty = true;
    }

    /// Cleanup the search state.
    fn exit_search(&mut self) {
        let vi_mode = self.terminal.mode().contains(TermMode::VI);
        self.window().set_text_input_active(!vi_mode);

        self.display.damage_tracker.frame().mark_fully_damaged();
        self.display.pending_update.dirty = true;
        self.search_state.history_index = None;

        // Clear focused match.
        self.search_state.focused_match = None;
    }

    /// Update the cursor blinking state.
    fn update_cursor_blinking(&mut self) {
        // Get config cursor style.
        let mut cursor_style = self.config.cursor.style;
        let vi_mode = self.terminal.mode().contains(TermMode::VI);
        if vi_mode {
            cursor_style = self.config.cursor.vi_mode_style.unwrap_or(cursor_style);
        }

        // Check terminal cursor style.
        let terminal_blinking = self.terminal.cursor_style().blinking;
        let mut blinking = cursor_style.blinking_override().unwrap_or(terminal_blinking);
        blinking &= (vi_mode || self.terminal().mode().contains(TermMode::SHOW_CURSOR))
            && self.display().ime.preedit().is_none();

        // Update cursor blinking state.
        let window_id = self.display.window.id();
        self.scheduler.unschedule(TimerId::new(Topic::BlinkCursor, window_id));
        self.scheduler.unschedule(TimerId::new(Topic::BlinkTimeout, window_id));

        // Reset blinking timeout.
        *self.cursor_blink_timed_out = false;

        if blinking && self.terminal.is_focused {
            self.schedule_blinking();
            self.schedule_blinking_timeout();
        } else {
            self.display.cursor_hidden = false;
            *self.dirty = true;
        }
    }

    fn schedule_blinking(&mut self) {
        let window_id = self.display.window.id();
        let timer_id = TimerId::new(Topic::BlinkCursor, window_id);
        let event = Event::new(EventType::BlinkCursor, window_id);
        let blinking_interval = Duration::from_millis(self.config.cursor.blink_interval());
        self.scheduler.schedule(event, blinking_interval, true, timer_id);
    }

    fn schedule_blinking_timeout(&mut self) {
        let blinking_timeout = self.config.cursor.blink_timeout();
        if blinking_timeout == Duration::ZERO {
            return;
        }

        let window_id = self.display.window.id();
        let event = Event::new(EventType::BlinkCursorTimeout, window_id);
        let timer_id = TimerId::new(Topic::BlinkTimeout, window_id);

        self.scheduler.schedule(event, blinking_timeout, false, timer_id);
    }

    /// Perform vi mode inline search in the specified direction.
    fn inline_search(&mut self, direction: Direction) {
        let c = match self.inline_search_state.character {
            Some(c) => c,
            None => return,
        };
        let mut buf = [0; 4];
        let search_character = c.encode_utf8(&mut buf);

        // Find next match in this line.
        let vi_point = self.terminal.vi_mode_cursor.point;
        let point = match direction {
            Direction::Right => self.terminal.inline_search_right(vi_point, search_character),
            Direction::Left => self.terminal.inline_search_left(vi_point, search_character),
        };

        // Jump to point if there's a match.
        if let Ok(mut point) = point {
            if self.inline_search_state.stop_short {
                let grid = self.terminal.grid();
                point = match direction {
                    Direction::Right => {
                        grid.iter_from(point).prev().map_or(point, |cell| cell.point)
                    },
                    Direction::Left => {
                        grid.iter_from(point).next().map_or(point, |cell| cell.point)
                    },
                };
            }

            self.terminal.vi_goto_point(point);
            self.mark_dirty();
        }
    }
}

/// Identified purpose of the touch input.
#[derive(Default, Debug)]
pub enum TouchPurpose {
    #[default]
    None,
    Select(TouchEvent),
    Scroll(TouchEvent),
    Zoom(TouchZoom),
    ZoomPendingSlot(TouchEvent),
    Tap(TouchEvent),
    Invalid(HashSet<u64, RandomState>),
}

/// Touch zooming state.
#[derive(Debug)]
pub struct TouchZoom {
    slots: (TouchEvent, TouchEvent),
    fractions: f32,
}

impl TouchZoom {
    pub fn new(slots: (TouchEvent, TouchEvent)) -> Self {
        Self { slots, fractions: Default::default() }
    }

    /// Get slot distance change since last update.
    pub fn font_delta(&mut self, slot: TouchEvent) -> f32 {
        let old_distance = self.distance();

        // Update touch slots.
        if slot.id == self.slots.0.id {
            self.slots.0 = slot;
        } else {
            self.slots.1 = slot;
        }

        // Calculate font change in `FONT_SIZE_STEP` increments.
        let delta = (self.distance() - old_distance) * TOUCH_ZOOM_FACTOR + self.fractions;
        let font_delta = (delta.abs() / FONT_SIZE_STEP).floor() * FONT_SIZE_STEP * delta.signum();
        self.fractions = delta - font_delta;

        font_delta
    }

    /// Get active touch slots.
    pub fn slots(&self) -> (TouchEvent, TouchEvent) {
        self.slots
    }

    /// Calculate distance between slots.
    fn distance(&self) -> f32 {
        let delta_x = self.slots.0.location.x - self.slots.1.location.x;
        let delta_y = self.slots.0.location.y - self.slots.1.location.y;
        delta_x.hypot(delta_y) as f32
    }
}

/// State of the mouse.
#[derive(Debug)]
pub struct Mouse {
    pub left_button_state: ElementState,
    pub middle_button_state: ElementState,
    pub right_button_state: ElementState,
    pub last_click_timestamp: Instant,
    pub last_click_button: MouseButton,
    pub click_state: ClickState,
    pub accumulated_scroll: AccumulatedScroll,
    pub cell_side: Side,
    pub block_hint_launcher: bool,
    pub hint_highlight_dirty: bool,
    pub inside_text_area: bool,
    pub x: usize,
    pub y: usize,
}

impl Default for Mouse {
    fn default() -> Mouse {
        Mouse {
            last_click_timestamp: Instant::now(),
            last_click_button: MouseButton::Left,
            left_button_state: ElementState::Released,
            middle_button_state: ElementState::Released,
            right_button_state: ElementState::Released,
            click_state: ClickState::None,
            cell_side: Side::Left,
            hint_highlight_dirty: Default::default(),
            block_hint_launcher: Default::default(),
            inside_text_area: Default::default(),
            accumulated_scroll: Default::default(),
            x: Default::default(),
            y: Default::default(),
        }
    }
}

impl Mouse {
    /// Convert mouse pixel coordinates to viewport point.
    ///
    /// If the coordinates are outside of the terminal grid, like positions inside the padding, the
    /// coordinates will be clamped to the closest grid coordinates.
    #[inline]
    pub fn point(&self, size: &SizeInfo, display_offset: usize) -> Point {
        let col = self.x.saturating_sub(size.padding_x() as usize) / (size.cell_width() as usize);
        let col = min(Column(col), size.last_column());

        let line = self.y.saturating_sub(size.padding_y() as usize) / (size.cell_height() as usize);
        let line = min(line, size.bottommost_line().0 as usize);

        term::viewport_to_point(display_offset, Point::new(line, col))
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ClickState {
    None,
    Click,
    DoubleClick,
    TripleClick,
}

/// The amount of scroll accumulated from the pointer events.
#[derive(Default, Debug)]
pub struct AccumulatedScroll {
    /// Scroll we should perform along `x` axis.
    pub x: f64,

    /// Scroll we should perform along `y` axis.
    pub y: f64,
}

impl input::Processor<EventProxy, ActionContext<'_, Notifier, EventProxy>> {
    /// Handle events from winit.
    pub fn handle_event(&mut self, event: WinitEvent<Event>) {
        match event {
            WinitEvent::UserEvent(Event { payload, .. }) => match payload {
                EventType::SearchNext => self.ctx.goto_match(None),
                EventType::Scroll(scroll) => self.ctx.scroll(scroll),
                EventType::BlinkCursor => {
                    // Only change state when timeout isn't reached, since we could get
                    // BlinkCursor and BlinkCursorTimeout events at the same time.
                    if !*self.ctx.cursor_blink_timed_out {
                        self.ctx.display.cursor_hidden ^= true;
                        *self.ctx.dirty = true;
                    }
                },
                EventType::BlinkCursorTimeout => {
                    // Disable blinking after timeout reached.
                    let timer_id = TimerId::new(Topic::BlinkCursor, self.ctx.display.window.id());
                    self.ctx.scheduler.unschedule(timer_id);
                    *self.ctx.cursor_blink_timed_out = true;
                    self.ctx.display.cursor_hidden = false;
                    *self.ctx.dirty = true;
                },
                // Add message only if it's not already queued.
                EventType::Message(message) if !self.ctx.message_buffer.is_queued(&message) => {
                    self.ctx.message_buffer.push(message);
                    self.ctx.display.pending_update.dirty = true;
                },
                EventType::Terminal(event) => match event {
                    TerminalEvent::Title(title) => {
                        if !self.ctx.preserve_title && self.ctx.config.window.dynamic_title {
                            self.ctx.window().set_title(title);
                        }
                    },
                    TerminalEvent::ResetTitle => {
                        let window_config = &self.ctx.config.window;
                        if !self.ctx.preserve_title && window_config.dynamic_title {
                            self.ctx.display.window.set_title(window_config.identity.title.clone());
                        }
                    },
                    TerminalEvent::Bell => {
                        // Set window urgency hint when window is not focused.
                        let focused = self.ctx.terminal.is_focused;
                        if !focused && self.ctx.terminal.mode().contains(TermMode::URGENCY_HINTS) {
                            self.ctx.window().set_urgent(true);
                        }

                        // Ring visual bell.
                        self.ctx.display.visual_bell.ring();

                        // Execute bell command.
                        if let Some(bell_command) = &self.ctx.config.bell.command {
                            if self
                                .ctx
                                .prev_bell_cmd
                                .is_none_or(|i| i.elapsed() >= BELL_CMD_COOLDOWN)
                            {
                                self.ctx.spawn_daemon(bell_command.program(), bell_command.args());

                                *self.ctx.prev_bell_cmd = Some(Instant::now());
                            }
                        }
                    },
                    TerminalEvent::ClipboardStore(clipboard_type, content) => {
                        if self.ctx.terminal.is_focused {
                            self.ctx.clipboard.store(clipboard_type, content);
                        }
                    },
                    TerminalEvent::ClipboardLoad(clipboard_type, format) => {
                        if self.ctx.terminal.is_focused {
                            let text = format(self.ctx.clipboard.load(clipboard_type).as_str());
                            self.ctx.write_to_pty(text.into_bytes());
                        }
                    },
                    TerminalEvent::ColorRequest(index, format) => {
                        let color = match self.ctx.terminal().colors()[index] {
                            Some(color) => Rgb(color),
                            // Ignore cursor color requests unless it was changed.
                            None if index == NamedColor::Cursor as usize => return,
                            None => self.ctx.display.colors[index],
                        };
                        self.ctx.write_to_pty(format(color.0).into_bytes());
                    },
                    TerminalEvent::TextAreaSizeRequest(format) => {
                        let text = format(self.ctx.size_info().into());
                        self.ctx.write_to_pty(text.into_bytes());
                    },
                    TerminalEvent::PtyWrite(text) => self.ctx.write_to_pty(text.into_bytes()),
                    TerminalEvent::MouseCursorDirty => self.reset_mouse_cursor(),
                    TerminalEvent::CursorBlinkingChange => self.ctx.update_cursor_blinking(),
                    TerminalEvent::Exit | TerminalEvent::ChildExit(_) | TerminalEvent::Wakeup => (),
                },
                #[cfg(unix)]
                EventType::IpcConfig(_) | EventType::IpcGetConfig(..) => (),
                EventType::Message(_)
                | EventType::ConfigReload(_)
                | EventType::CreateWindow(_)
                | EventType::Frame => (),
            },
            WinitEvent::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::CloseRequested => {
                        // User asked to close the window, so no need to hold it.
                        self.ctx.window().hold = false;
                        self.ctx.terminal.exit();
                    },
                    WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                        let old_scale_factor =
                            mem::replace(&mut self.ctx.window().scale_factor, scale_factor);

                        let display_update_pending = &mut self.ctx.display.pending_update;

                        // Rescale font size for the new factor.
                        let font_scale = scale_factor as f32 / old_scale_factor as f32;
                        self.ctx.display.font_size = self.ctx.display.font_size.scale(font_scale);

                        let font = self.ctx.config.font.clone();
                        display_update_pending.set_font(font.with_size(self.ctx.display.font_size));
                    },
                    WindowEvent::Resized(size) => {
                        // Ignore resize events to zero in any dimension, to avoid issues with Winit
                        // and the ConPTY. A 0x0 resize will also occur when the window is minimized
                        // on Windows.
                        if size.width == 0 || size.height == 0 {
                            return;
                        }

                        self.ctx.display.pending_update.set_dimensions(size);
                    },
                    WindowEvent::KeyboardInput { event, is_synthetic: false, .. } => {
                        self.key_input(event);
                    },
                    WindowEvent::ModifiersChanged(modifiers) => self.modifiers_input(modifiers),
                    WindowEvent::MouseInput { state, button, .. } => {
                        self.ctx.window().set_mouse_visible(true);
                        self.mouse_input(state, button);
                    },
                    WindowEvent::CursorMoved { position, .. } => {
                        self.ctx.window().set_mouse_visible(true);
                        self.mouse_moved(position);
                    },
                    WindowEvent::MouseWheel { delta, phase, .. } => {
                        self.ctx.window().set_mouse_visible(true);
                        self.mouse_wheel_input(delta, phase);
                    },
                    WindowEvent::Touch(touch) => self.touch(touch),
                    WindowEvent::Focused(is_focused) => {
                        self.ctx.terminal.is_focused = is_focused;

                        // When the unfocused hollow is used we must redraw on focus change.
                        if self.ctx.config.cursor.unfocused_hollow {
                            *self.ctx.dirty = true;
                        }

                        if is_focused {
                            // Reset the urgency hint when gaining focus.
                            self.ctx.window().set_urgent(false);
                        } else {
                            // Clear touch focus when window loses focus.
                            self.ctx.window().set_touch_focus(false);
                        }

                        self.ctx.update_cursor_blinking();
                        self.on_focus_change(is_focused);
                    },
                    WindowEvent::Occluded(occluded) => {
                        *self.ctx.occluded = occluded;
                    },
                    WindowEvent::DroppedFile(path) => {
                        let path: String = path.to_string_lossy().into();
                        self.ctx.paste(&(path + " "), true);
                    },
                    WindowEvent::CursorEntered { .. } => self.ctx.window().set_pointer_focus(true),
                    WindowEvent::CursorLeft { .. } => {
                        self.ctx.window().set_pointer_focus(false);
                        self.ctx.mouse.inside_text_area = false;

                        if self.ctx.display().highlighted_hint.is_some() {
                            *self.ctx.dirty = true;
                        }
                    },
                    WindowEvent::Ime(ime) => match ime {
                        Ime::Commit(text) => {
                            *self.ctx.dirty = true;
                            // Don't use bracketed paste for single char input.
                            self.ctx.paste(&text, text.chars().count() > 1);
                            self.ctx.update_cursor_blinking();
                        },
                        Ime::Preedit(text, cursor_offset) => {
                            let preedit =
                                (!text.is_empty()).then(|| Preedit::new(text, cursor_offset));

                            if self.ctx.display.ime.preedit() != preedit.as_ref() {
                                self.ctx.display.ime.set_preedit(preedit);
                                self.ctx.update_cursor_blinking();
                                *self.ctx.dirty = true;
                            }
                        },
                        Ime::Enabled => {
                            self.ctx.display.ime.set_enabled(true);
                            *self.ctx.dirty = true;
                        },
                        Ime::Disabled => {
                            self.ctx.display.ime.set_enabled(false);
                            *self.ctx.dirty = true;
                        },
                    },
                    WindowEvent::KeyboardInput { is_synthetic: true, .. }
                    | WindowEvent::ActivationTokenDone { .. }
                    | WindowEvent::DoubleTapGesture { .. }
                    | WindowEvent::TouchpadPressure { .. }
                    | WindowEvent::RotationGesture { .. }
                    | WindowEvent::PinchGesture { .. }
                    | WindowEvent::AxisMotion { .. }
                    | WindowEvent::PanGesture { .. }
                    | WindowEvent::HoveredFileCancelled
                    | WindowEvent::Destroyed
                    | WindowEvent::ThemeChanged(_)
                    | WindowEvent::HoveredFile(_)
                    | WindowEvent::RedrawRequested
                    | WindowEvent::Moved(_) => (),
                }
            },
            WinitEvent::Suspended
            | WinitEvent::NewEvents { .. }
            | WinitEvent::DeviceEvent { .. }
            | WinitEvent::LoopExiting
            | WinitEvent::Resumed
            | WinitEvent::MemoryWarning
            | WinitEvent::AboutToWait => (),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventProxy {
    proxy: EventLoopProxy<Event>,
    window_id: WindowId,
}

impl EventProxy {
    pub fn new(proxy: EventLoopProxy<Event>, window_id: WindowId) -> Self {
        Self { proxy, window_id }
    }

    /// Send an event to the event loop.
    pub fn send_event(&self, event: EventType) {
        let _ = self.proxy.send_event(Event::new(event, self.window_id));
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: TerminalEvent) {
        let _ = self.proxy.send_event(Event::new(event.into(), self.window_id));
    }
}
