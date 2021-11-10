//! Process window events.

use std::borrow::Cow;
use std::cmp::{max, min};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::Debug;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{env, f32, mem};

use glutin::dpi::PhysicalSize;
use glutin::event::{ElementState, Event as GlutinEvent, ModifiersState, MouseButton, WindowEvent};
use glutin::event_loop::{ControlFlow, EventLoop, EventLoopProxy, EventLoopWindowTarget};
use glutin::platform::run_return::EventLoopExtRunReturn;
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use glutin::platform::unix::EventLoopWindowTargetExtUnix;
use glutin::window::WindowId;
use log::{error, info};
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use wayland_client::{Display as WaylandDisplay, EventQueue};

use crossfont::{self, Size};

use alacritty_terminal::config::LOG_TARGET_CONFIG;
use alacritty_terminal::event::{Event as TerminalEvent, EventListener, Notify};
use alacritty_terminal::event_loop::Notifier;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{ClipboardType, SizeInfo, Term, TermMode};
#[cfg(not(windows))]
use alacritty_terminal::tty;

use crate::cli::{Options as CliOptions, TerminalOptions as TerminalCliOptions};
use crate::clipboard::Clipboard;
use crate::config::ui_config::{HintAction, HintInternalAction};
use crate::config::{self, UiConfig};
use crate::daemon::start_daemon;
use crate::display::hint::HintMatch;
use crate::display::window::Window;
use crate::display::{self, Display};
use crate::input::{self, ActionContext as _, FONT_SIZE_STEP};
#[cfg(target_os = "macos")]
use crate::macos;
use crate::message_bar::{Message, MessageBuffer};
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::window_context::WindowContext;

/// Duration after the last user input until an unlimited search is performed.
pub const TYPING_SEARCH_DELAY: Duration = Duration::from_millis(500);

/// Maximum number of lines for the blocking search while still typing the search regex.
const MAX_SEARCH_WHILE_TYPING: Option<usize> = Some(1000);

/// Maximum number of search terms stored in the history.
const MAX_SEARCH_HISTORY_SIZE: usize = 255;

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

impl From<Event> for GlutinEvent<'_, Event> {
    fn from(event: Event) -> Self {
        GlutinEvent::UserEvent(event)
    }
}

/// Alacritty events.
#[derive(Debug, Clone)]
pub enum EventType {
    DprChanged(f64, (u32, u32)),
    Terminal(TerminalEvent),
    ConfigReload(PathBuf),
    Message(Message),
    Scroll(Scroll),
    CreateWindow(Option<TerminalCliOptions>),
    BlinkCursor,
    SearchNext,
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

    /// Active search dfas.
    pub fn dfas(&self) -> Option<&RegexSearch> {
        self.dfas.as_ref()
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

pub struct ActionContext<'a, N, T> {
    pub notifier: &'a mut N,
    pub terminal: &'a mut Term<T>,
    pub clipboard: &'a mut Clipboard,
    pub mouse: &'a mut Mouse,
    pub received_count: &'a mut usize,
    pub suppress_chars: &'a mut bool,
    pub modifiers: &'a mut ModifiersState,
    pub display: &'a mut Display,
    pub message_buffer: &'a mut MessageBuffer,
    pub config: &'a mut UiConfig,
    pub event_loop: &'a EventLoopWindowTarget<Event>,
    pub event_proxy: &'a EventLoopProxy<Event>,
    pub scheduler: &'a mut Scheduler,
    pub search_state: &'a mut SearchState,
    pub font_size: &'a mut Size,
    pub dirty: &'a mut bool,
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

        self.terminal.scroll_display(scroll);

        // Keep track of manual display offset changes during search.
        if self.search_active() {
            let display_offset = self.terminal.grid().display_offset();
            self.search_state.display_offset_delta += old_offset - display_offset as i32;
        }

        // Update selection.
        if self.terminal.mode().contains(TermMode::VI)
            && self.terminal.selection.as_ref().map_or(true, |s| !s.is_empty())
        {
            self.update_selection(self.terminal.vi_mode_cursor.point, Side::Right);
        } else if self.mouse.left_button_state == ElementState::Pressed
            || self.mouse.right_button_state == ElementState::Pressed
        {
            let display_offset = self.terminal.grid().display_offset();
            let point = self.mouse.point(&self.size_info(), display_offset);
            self.update_selection(point, self.mouse.cell_side);
        }
        self.copy_selection(ClipboardType::Selection);

        *self.dirty = true;
    }

    // Copy text selection.
    fn copy_selection(&mut self, ty: ClipboardType) {
        let text = match self.terminal.selection_to_string().filter(|s| !s.is_empty()) {
            Some(text) => text,
            None => return,
        };

        if ty == ClipboardType::Selection && self.config.terminal_config.selection.save_to_clipboard
        {
            self.clipboard.store(ClipboardType::Clipboard, text.clone());
        }
        self.clipboard.store(ty, text);
    }

    fn selection_is_empty(&self) -> bool {
        self.terminal.selection.as_ref().map(Selection::is_empty).unwrap_or(true)
    }

    fn clear_selection(&mut self) {
        self.terminal.selection = None;
        *self.dirty = true;
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
    fn received_count(&mut self) -> &mut usize {
        self.received_count
    }

    #[inline]
    fn suppress_chars(&mut self) -> &mut bool {
        self.suppress_chars
    }

    #[inline]
    fn modifiers(&mut self) -> &mut ModifiersState {
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

        // Use working directory of controlling process, or fallback to initial shell, then add it
        // as working-directory parameter.
        #[cfg(unix)]
        let mut args = foreground_process_path()
            .map(|path| vec!["--working-directory".into(), path])
            .unwrap_or_default();

        #[cfg(not(unix))]
        let mut args: Vec<PathBuf> = Vec::new();

        let working_directory_set = !args.is_empty();

        // Reuse the arguments passed to Alacritty for the new instance.
        while let Some(arg) = env_args.next() {
            // Drop working directory from existing parameters.
            if working_directory_set && arg == "--working-directory" {
                let _ = env_args.next();
                continue;
            }

            args.push(arg.into());
        }

        start_daemon(&alacritty, &args);
    }

    #[cfg(not(windows))]
    fn create_new_window(&mut self) {
        let options = if let Ok(working_directory) = foreground_process_path() {
            let mut options = TerminalCliOptions::new();
            options.working_directory = Some(working_directory);
            Some(options)
        } else {
            None
        };

        let _ = self.event_proxy.send_event(Event::new(EventType::CreateWindow(options), None));
    }

    #[cfg(windows)]
    fn create_new_window(&mut self) {
        let _ = self.event_proxy.send_event(Event::new(EventType::CreateWindow(None), None));
    }

    fn change_font_size(&mut self, delta: f32) {
        *self.font_size = max(*self.font_size + delta, Size::new(FONT_SIZE_STEP));
        let font = self.config.font.clone().with_size(*self.font_size);
        self.display.pending_update.set_font(font);
        *self.dirty = true;
    }

    fn reset_font_size(&mut self) {
        *self.font_size = self.config.font.size();
        self.display.pending_update.set_font(self.config.font.clone());
        *self.dirty = true;
    }

    #[inline]
    fn pop_message(&mut self) {
        if !self.message_buffer.is_empty() {
            self.display.pending_update.dirty = true;
            self.message_buffer.pop();
            *self.dirty = true;
        }
    }

    #[inline]
    fn start_search(&mut self, direction: Direction) {
        // Only create new history entry if the previous regex wasn't empty.
        if self.search_state.history.get(0).map_or(true, |regex| !regex.is_empty()) {
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

        self.display.pending_update.dirty = true;
        *self.dirty = true;
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
            regex.truncate(regex.rfind(' ').map(|i| i + 1).unwrap_or(0));
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
            .as_ref()
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
        if let Some(timer) = self.scheduler.unschedule(timer_id) {
            let interval =
                Duration::from_millis(self.config.terminal_config.cursor.blink_interval());
            self.scheduler.schedule(timer.event, interval, true, timer.id);
            self.display.cursor_hidden = false;
            *self.dirty = true;
        }

        // Hide mouse cursor.
        if self.config.mouse.hide_when_typing {
            self.display.window.set_mouse_visible(false);
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

        match &hint.action {
            // Launch an external program.
            HintAction::Command(command) => {
                let text = self.terminal.bounds_to_string(*hint.bounds.start(), *hint.bounds.end());
                let mut args = command.args().to_vec();
                args.push(text);
                start_daemon(command.program(), &args);
            },
            // Copy the text to the clipboard.
            HintAction::Action(HintInternalAction::Copy) => {
                let text = self.terminal.bounds_to_string(*hint.bounds.start(), *hint.bounds.end());
                self.clipboard.store(ClipboardType::Clipboard, text);
            },
            // Write the text to the PTY/search.
            HintAction::Action(HintInternalAction::Paste) => {
                let text = self.terminal.bounds_to_string(*hint.bounds.start(), *hint.bounds.end());
                self.paste(&text);
            },
            // Select the text.
            HintAction::Action(HintInternalAction::Select) => {
                self.start_selection(SelectionType::Simple, *hint.bounds.start(), Side::Left);
                self.update_selection(*hint.bounds.end(), Side::Right);
                self.copy_selection(ClipboardType::Selection);
            },
            // Move the vi mode cursor.
            HintAction::Action(HintInternalAction::MoveViModeCursor) => {
                // Enter vi mode if we're not in it already.
                if !self.terminal.mode().contains(TermMode::VI) {
                    self.terminal.toggle_vi_mode();
                }

                self.terminal.vi_goto_point(*hint.bounds.start());
            },
        }
    }

    /// Expand the selection to the current mouse cursor position.
    #[inline]
    fn expand_selection(&mut self) {
        let selection_type = match self.mouse().click_state {
            ClickState::Click => {
                if self.modifiers().ctrl() {
                    SelectionType::Block
                } else {
                    SelectionType::Simple
                }
            },
            ClickState::DoubleClick => SelectionType::Semantic,
            ClickState::TripleClick => SelectionType::Lines,
            ClickState::None => return,
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

    /// Paste a text into the terminal.
    fn paste(&mut self, text: &str) {
        if self.search_active() {
            for c in text.chars() {
                self.search_input(c);
            }
        } else if self.terminal().mode().contains(TermMode::BRACKETED_PASTE) {
            self.write_to_pty(&b"\x1b[200~"[..]);
            self.write_to_pty(text.replace("\x1b", "").into_bytes());
            self.write_to_pty(&b"\x1b[201~"[..]);
        } else {
            // In non-bracketed (ie: normal) mode, terminal applications cannot distinguish
            // pasted data from keystrokes.
            // In theory, we should construct the keystrokes needed to produce the data we are
            // pasting... since that's neither practical nor sensible (and probably an impossible
            // task to solve in a general way), we'll just replace line breaks (windows and unix
            // style) with a single carriage return (\r, which is what the Enter key produces).
            self.write_to_pty(text.replace("\r\n", "\r").replace("\n", "\r").into_bytes());
        }
    }

    /// Toggle the vi mode status.
    #[inline]
    fn toggle_vi_mode(&mut self) {
        if !self.terminal.mode().contains(TermMode::VI) {
            self.clear_selection();
        }

        self.cancel_search();
        self.terminal.toggle_vi_mode();

        *self.dirty = true;
    }

    fn message(&self) -> Option<&Message> {
        self.message_buffer.message()
    }

    fn config(&self) -> &UiConfig {
        self.config
    }

    fn event_loop(&self) -> &EventLoopWindowTarget<Event> {
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
        self.terminal.scroll_display(Scroll::Delta(self.search_state.display_offset_delta));
        self.search_state.display_offset_delta = 0;
        self.terminal.vi_mode_cursor.point =
            self.search_state.origin.grid_clamp(self.terminal, Boundary::Grid);

        *self.dirty = true;
    }

    /// Jump to the first regex match from the search origin.
    fn goto_match(&mut self, mut limit: Option<usize>) {
        let dfas = match &self.search_state.dfas {
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
        self.display.pending_update.dirty = true;
        self.search_state.history_index = None;
        *self.dirty = true;

        // Clear focused match.
        self.search_state.focused_match = None;
    }

    /// Update the cursor blinking state.
    fn update_cursor_blinking(&mut self) {
        // Get config cursor style.
        let mut cursor_style = self.config.terminal_config.cursor.style;
        if self.terminal.mode().contains(TermMode::VI) {
            cursor_style = self.config.terminal_config.cursor.vi_mode_style.unwrap_or(cursor_style);
        };

        // Check terminal cursor style.
        let terminal_blinking = self.terminal.cursor_style().blinking;
        let blinking = cursor_style.blinking_override().unwrap_or(terminal_blinking);

        // Update cursor blinking state.
        let timer_id = TimerId::new(Topic::BlinkCursor, self.display.window.id());
        self.scheduler.unschedule(timer_id);
        if blinking && self.terminal.is_focused {
            let event = Event::new(EventType::BlinkCursor, self.display.window.id());
            let interval =
                Duration::from_millis(self.config.terminal_config.cursor.blink_interval());
            self.scheduler.schedule(event, interval, true, timer_id);
        } else {
            self.display.cursor_hidden = false;
            *self.dirty = true;
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ClickState {
    None,
    Click,
    DoubleClick,
    TripleClick,
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
    pub scroll_px: f64,
    pub cell_side: Side,
    pub lines_scrolled: f32,
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
            lines_scrolled: Default::default(),
            scroll_px: Default::default(),
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

        display::viewport_to_point(display_offset, Point::new(line, col))
    }
}

impl input::Processor<EventProxy, ActionContext<'_, Notifier, EventProxy>> {
    /// Handle events from glutin.
    ///
    /// Doesn't take self mutably due to borrow checking.
    pub fn handle_event(&mut self, event: GlutinEvent<'_, Event>) {
        match event {
            GlutinEvent::UserEvent(Event { payload, .. }) => match payload {
                EventType::DprChanged(scale_factor, (width, height)) => {
                    let display_update_pending = &mut self.ctx.display.pending_update;

                    // Push current font to update its DPR.
                    let font = self.ctx.config.font.clone();
                    display_update_pending.set_font(font.with_size(*self.ctx.font_size));

                    // Resize to event's dimensions, since no resize event is emitted on Wayland.
                    display_update_pending.set_dimensions(PhysicalSize::new(width, height));

                    self.ctx.window().dpr = scale_factor;
                    *self.ctx.dirty = true;
                },
                EventType::SearchNext => self.ctx.goto_match(None),
                EventType::Scroll(scroll) => self.ctx.scroll(scroll),
                EventType::BlinkCursor => {
                    self.ctx.display.cursor_hidden ^= true;
                    *self.ctx.dirty = true;
                },
                EventType::Message(message) => {
                    self.ctx.message_buffer.push(message);
                    self.ctx.display.pending_update.dirty = true;
                    *self.ctx.dirty = true;
                },
                EventType::Terminal(event) => match event {
                    TerminalEvent::Title(title) => {
                        if self.ctx.config.window.dynamic_title {
                            self.ctx.window().set_title(&title);
                        }
                    },
                    TerminalEvent::ResetTitle => {
                        if self.ctx.config.window.dynamic_title {
                            self.ctx.display.window.set_title(&self.ctx.config.window.title);
                        }
                    },
                    TerminalEvent::Wakeup => *self.ctx.dirty = true,
                    TerminalEvent::Bell => {
                        // Set window urgency.
                        if self.ctx.terminal.mode().contains(TermMode::URGENCY_HINTS) {
                            let focused = self.ctx.terminal.is_focused;
                            self.ctx.window().set_urgent(!focused);
                        }

                        // Ring visual bell.
                        self.ctx.display.visual_bell.ring();

                        // Execute bell command.
                        if let Some(bell_command) = &self.ctx.config.bell.command {
                            start_daemon(bell_command.program(), bell_command.args());
                        }
                    },
                    TerminalEvent::ClipboardStore(clipboard_type, content) => {
                        self.ctx.clipboard.store(clipboard_type, content);
                    },
                    TerminalEvent::ClipboardLoad(clipboard_type, format) => {
                        let text = format(self.ctx.clipboard.load(clipboard_type).as_str());
                        self.ctx.write_to_pty(text.into_bytes());
                    },
                    TerminalEvent::ColorRequest(index, format) => {
                        let text = format(self.ctx.display.colors[index]);
                        self.ctx.write_to_pty(text.into_bytes());
                    },
                    TerminalEvent::PtyWrite(text) => self.ctx.write_to_pty(text.into_bytes()),
                    TerminalEvent::MouseCursorDirty => self.reset_mouse_cursor(),
                    TerminalEvent::Exit => (),
                    TerminalEvent::CursorBlinkingChange => self.ctx.update_cursor_blinking(),
                },
                EventType::ConfigReload(_) | EventType::CreateWindow(_) => (),
            },
            GlutinEvent::RedrawRequested(_) => *self.ctx.dirty = true,
            GlutinEvent::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::CloseRequested => self.ctx.terminal.exit(),
                    WindowEvent::Resized(size) => {
                        // Minimizing the window sends a Resize event with zero width and
                        // height. But there's no need to ever actually resize to this.
                        // ConPTY has issues when resizing down to zero size and back.
                        #[cfg(windows)]
                        if size.width == 0 && size.height == 0 {
                            return;
                        }

                        self.ctx.display.pending_update.set_dimensions(size);
                        *self.ctx.dirty = true;
                    },
                    WindowEvent::KeyboardInput { input, is_synthetic: false, .. } => {
                        self.key_input(input);
                    },
                    WindowEvent::ModifiersChanged(modifiers) => self.modifiers_input(modifiers),
                    WindowEvent::ReceivedCharacter(c) => self.received_char(c),
                    WindowEvent::MouseInput { state, button, .. } => {
                        self.ctx.window().set_mouse_visible(true);
                        self.mouse_input(state, button);
                        *self.ctx.dirty = true;
                    },
                    WindowEvent::CursorMoved { position, .. } => {
                        self.ctx.window().set_mouse_visible(true);
                        self.mouse_moved(position);
                    },
                    WindowEvent::MouseWheel { delta, phase, .. } => {
                        self.ctx.window().set_mouse_visible(true);
                        self.mouse_wheel_input(delta, phase);
                    },
                    WindowEvent::Focused(is_focused) => {
                        self.ctx.terminal.is_focused = is_focused;
                        *self.ctx.dirty = true;

                        if is_focused {
                            self.ctx.window().set_urgent(false);
                        } else {
                            self.ctx.window().set_mouse_visible(true);
                        }

                        self.ctx.update_cursor_blinking();
                        self.on_focus_change(is_focused);
                    },
                    WindowEvent::DroppedFile(path) => {
                        let path: String = path.to_string_lossy().into();
                        self.ctx.write_to_pty((path + " ").into_bytes());
                    },
                    WindowEvent::CursorLeft { .. } => {
                        self.ctx.mouse.inside_text_area = false;

                        if self.ctx.display().highlighted_hint.is_some() {
                            *self.ctx.dirty = true;
                        }
                    },
                    WindowEvent::KeyboardInput { is_synthetic: true, .. }
                    | WindowEvent::TouchpadPressure { .. }
                    | WindowEvent::ScaleFactorChanged { .. }
                    | WindowEvent::CursorEntered { .. }
                    | WindowEvent::AxisMotion { .. }
                    | WindowEvent::HoveredFileCancelled
                    | WindowEvent::Destroyed
                    | WindowEvent::ThemeChanged(_)
                    | WindowEvent::HoveredFile(_)
                    | WindowEvent::Touch(_)
                    | WindowEvent::Moved(_) => (),
                }
            },
            GlutinEvent::Suspended { .. }
            | GlutinEvent::NewEvents { .. }
            | GlutinEvent::DeviceEvent { .. }
            | GlutinEvent::MainEventsCleared
            | GlutinEvent::RedrawEventsCleared
            | GlutinEvent::Resumed
            | GlutinEvent::LoopDestroyed => (),
        }
    }
}

/// The event processor.
///
/// Stores some state from received events and dispatches actions when they are
/// triggered.
pub struct Processor {
    #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
    wayland_event_queue: Option<EventQueue>,
    windows: HashMap<WindowId, WindowContext>,
    cli_options: CliOptions,
    config: UiConfig,
}

impl Processor {
    /// Create a new event processor.
    ///
    /// Takes a writer which is expected to be hooked up to the write end of a PTY.
    pub fn new(
        config: UiConfig,
        cli_options: CliOptions,
        _event_loop: &EventLoop<Event>,
    ) -> Processor {
        // Initialize Wayland event queue, to handle Wayland callbacks.
        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        let wayland_event_queue = _event_loop.wayland_display().map(|display| {
            let display = unsafe { WaylandDisplay::from_external_display(display as _) };
            display.create_event_queue()
        });

        Processor {
            windows: HashMap::new(),
            cli_options,
            config,
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            wayland_event_queue,
        }
    }

    /// Create a new terminal window.
    pub fn create_window(
        &mut self,
        event_loop: &EventLoopWindowTarget<Event>,
        options: Option<TerminalCliOptions>,
        proxy: EventLoopProxy<Event>,
    ) -> Result<(), Box<dyn Error>> {
        let window_context = WindowContext::new(
            &self.config,
            options,
            event_loop,
            proxy,
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            self.wayland_event_queue.as_ref(),
        )?;
        self.windows.insert(window_context.id(), window_context);
        Ok(())
    }

    /// Run the event loop.
    pub fn run(&mut self, mut event_loop: EventLoop<Event>) {
        let proxy = event_loop.create_proxy();
        let mut scheduler = Scheduler::new(proxy.clone());

        // NOTE: Since this takes a pointer to the winit event loop, it MUST be dropped first.
        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        let mut clipboard = unsafe { Clipboard::new(event_loop.wayland_display()) };
        #[cfg(any(not(feature = "wayland"), target_os = "macos", windows))]
        let mut clipboard = Clipboard::new();

        event_loop.run_return(|event, event_loop, control_flow| {
            if self.config.debug.print_events {
                info!("glutin event: {:?}", event);
            }

            // Ignore all events we do not care about.
            if Self::skip_event(&event) {
                return;
            }

            match event {
                // Check for shutdown.
                GlutinEvent::UserEvent(Event {
                    window_id: Some(window_id),
                    payload: EventType::Terminal(TerminalEvent::Exit),
                }) => {
                    // Remove the closed terminal.
                    let window_context = match self.windows.remove(&window_id) {
                        Some(window_context) => window_context,
                        None => return,
                    };

                    // Unschedule pending events.
                    scheduler.unschedule_window(window_context.id());

                    // Shutdown if no more terminals are open.
                    if self.windows.is_empty() {
                        // Write ref tests of last window to disk.
                        if self.config.debug.ref_test {
                            window_context.write_ref_test_results();
                        }

                        *control_flow = ControlFlow::Exit;
                    }
                },
                // Process all pending events.
                GlutinEvent::RedrawEventsCleared => {
                    *control_flow = match scheduler.update() {
                        Some(instant) => ControlFlow::WaitUntil(instant),
                        None => ControlFlow::Wait,
                    };

                    // Check for pending frame callbacks on Wayland.
                    #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
                    if let Some(wayland_event_queue) = self.wayland_event_queue.as_mut() {
                        wayland_event_queue
                            .dispatch_pending(&mut (), |_, _, _| {})
                            .expect("failed to dispatch wayland event queue");
                    }

                    // Dispatch event to all windows.
                    for window_context in self.windows.values_mut() {
                        window_context.handle_event(
                            event_loop,
                            &proxy,
                            &mut self.config,
                            &mut clipboard,
                            &mut scheduler,
                            GlutinEvent::RedrawEventsCleared,
                        );
                    }
                },
                // Process config update.
                GlutinEvent::UserEvent(Event {
                    payload: EventType::ConfigReload(path), ..
                }) => {
                    // Clear config logs from message bar for all terminals.
                    for window_context in self.windows.values_mut() {
                        if !window_context.message_buffer.is_empty() {
                            window_context.message_buffer.remove_target(LOG_TARGET_CONFIG);
                            window_context.display.pending_update.dirty = true;
                        }
                    }

                    // Load config and update each terminal.
                    if let Ok(config) = config::reload(&path, &self.cli_options) {
                        let old_config = mem::replace(&mut self.config, config);

                        for window_context in self.windows.values_mut() {
                            window_context.update_config(&old_config, &self.config);
                        }
                    }
                },
                // Create a new terminal window.
                GlutinEvent::UserEvent(Event {
                    payload: EventType::CreateWindow(options), ..
                }) => {
                    if let Err(err) = self.create_window(event_loop, options, proxy.clone()) {
                        error!("Could not open window: {:?}", err);
                    }
                },
                // Process events affecting all windows.
                GlutinEvent::UserEvent(event @ Event { window_id: None, .. }) => {
                    for window_context in self.windows.values_mut() {
                        window_context.handle_event(
                            event_loop,
                            &proxy,
                            &mut self.config,
                            &mut clipboard,
                            &mut scheduler,
                            event.clone().into(),
                        );
                    }
                },
                // Process window-specific events.
                GlutinEvent::WindowEvent { window_id, .. }
                | GlutinEvent::UserEvent(Event { window_id: Some(window_id), .. })
                | GlutinEvent::RedrawRequested(window_id) => {
                    if let Some(window_context) = self.windows.get_mut(&window_id) {
                        window_context.handle_event(
                            event_loop,
                            &proxy,
                            &mut self.config,
                            &mut clipboard,
                            &mut scheduler,
                            event,
                        );
                    }
                },
                _ => (),
            }
        });
    }

    /// Check if an event is irrelevant and can be skipped.
    fn skip_event(event: &GlutinEvent<'_, Event>) -> bool {
        match event {
            GlutinEvent::WindowEvent { event, .. } => matches!(
                event,
                WindowEvent::KeyboardInput { is_synthetic: true, .. }
                    | WindowEvent::TouchpadPressure { .. }
                    | WindowEvent::CursorEntered { .. }
                    | WindowEvent::AxisMotion { .. }
                    | WindowEvent::HoveredFileCancelled
                    | WindowEvent::Destroyed
                    | WindowEvent::HoveredFile(_)
                    | WindowEvent::Touch(_)
                    | WindowEvent::Moved(_)
            ),
            GlutinEvent::Suspended { .. }
            | GlutinEvent::NewEvents { .. }
            | GlutinEvent::MainEventsCleared
            | GlutinEvent::LoopDestroyed => true,
            _ => false,
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

#[cfg(not(windows))]
pub fn foreground_process_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut pid = unsafe { libc::tcgetpgrp(tty::master_fd()) };
    if pid < 0 {
        pid = tty::child_pid();
    }

    #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
    let link_path = format!("/proc/{}/cwd", pid);
    #[cfg(target_os = "freebsd")]
    let link_path = format!("/compat/linux/proc/{}/cwd", pid);

    #[cfg(not(target_os = "macos"))]
    let cwd = std::fs::read_link(link_path)?;

    #[cfg(target_os = "macos")]
    let cwd = macos::proc::cwd(pid)?;

    Ok(cwd)
}
