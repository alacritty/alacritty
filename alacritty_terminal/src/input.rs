// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//! Handle input from glutin
//!
//! Certain key combinations should send some escape sequence back to the pty.
//! In order to figure that out, state about which modifier keys are pressed
//! needs to be tracked. Additionally, we need a bit of a state machine to
//! determine what to do when a non-modifier key is pressed.
use crate::url::Url;
use std::borrow::Cow;
use std::mem;
use std::time::Instant;

use glutin::{
    ElementState, KeyboardInput, ModifiersState, MouseButton, MouseCursor, MouseScrollDelta,
    TouchPhase, VirtualKeyCode,
};

use crate::ansi::{ClearMode, Handler};
use crate::clipboard::ClipboardType;
use crate::config::{self, Key};
use crate::event::{ClickState, Mouse};
use crate::grid::Scroll;
use crate::index::{Column, Line, Point, Side};
use crate::message_bar::{self, Message};
use crate::term::mode::TermMode;
use crate::term::{SizeInfo, Term};
use crate::util::start_daemon;

pub const FONT_SIZE_STEP: f32 = 0.5;

/// Processes input from glutin.
///
/// An escape sequence may be emitted in case specific keys or key combinations
/// are activated.
pub struct Processor<'a, A: 'a> {
    pub key_bindings: &'a [KeyBinding],
    pub mouse_bindings: &'a [MouseBinding],
    pub mouse_config: &'a config::Mouse,
    pub scrolling_config: &'a config::Scrolling,
    pub ctx: A,
    pub save_to_clipboard: bool,
    pub alt_send_esc: bool,
}

pub trait ActionContext {
    fn write_to_pty<B: Into<Cow<'static, [u8]>>>(&mut self, _: B);
    fn size_info(&self) -> SizeInfo;
    fn copy_selection(&mut self, _: ClipboardType);
    fn clear_selection(&mut self);
    fn update_selection(&mut self, point: Point, side: Side);
    fn simple_selection(&mut self, point: Point, side: Side);
    fn block_selection(&mut self, point: Point, side: Side);
    fn semantic_selection(&mut self, point: Point);
    fn line_selection(&mut self, point: Point);
    fn selection_is_empty(&self) -> bool;
    fn mouse_mut(&mut self) -> &mut Mouse;
    fn mouse(&self) -> &Mouse;
    fn mouse_coords(&self) -> Option<Point>;
    fn received_count(&mut self) -> &mut usize;
    fn suppress_chars(&mut self) -> &mut bool;
    fn modifiers(&mut self) -> &mut Modifiers;
    fn scroll(&mut self, scroll: Scroll);
    fn hide_window(&mut self);
    fn terminal(&self) -> &Term;
    fn terminal_mut(&mut self) -> &mut Term;
    fn spawn_new_instance(&mut self);
    fn toggle_fullscreen(&mut self);
    #[cfg(target_os = "macos")]
    fn toggle_simple_fullscreen(&mut self);
}

#[derive(Debug, Default, Copy, Clone)]
pub struct Modifiers {
    mods: ModifiersState,
    lshift: bool,
    rshift: bool,
}

impl Modifiers {
    pub fn update(&mut self, input: KeyboardInput) {
        match input.virtual_keycode {
            Some(VirtualKeyCode::LShift) => self.lshift = input.state == ElementState::Pressed,
            Some(VirtualKeyCode::RShift) => self.rshift = input.state == ElementState::Pressed,
            _ => (),
        }

        self.mods = input.modifiers;
    }

    pub fn shift(self) -> bool {
        self.lshift || self.rshift
    }

    pub fn ctrl(self) -> bool {
        self.mods.ctrl
    }

    pub fn logo(self) -> bool {
        self.mods.logo
    }

    pub fn alt(self) -> bool {
        self.mods.alt
    }
}

impl From<&mut Modifiers> for ModifiersState {
    fn from(mods: &mut Modifiers) -> ModifiersState {
        ModifiersState { shift: mods.shift(), ..mods.mods }
    }
}

/// Describes a state and action to take in that state
///
/// This is the shared component of `MouseBinding` and `KeyBinding`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding<T> {
    /// Modifier keys required to activate binding
    pub mods: ModifiersState,

    /// String to send to pty if mods and mode match
    pub action: Action,

    /// Terminal mode required to activate binding
    pub mode: TermMode,

    /// excluded terminal modes where the binding won't be activated
    pub notmode: TermMode,

    /// This property is used as part of the trigger detection code.
    ///
    /// For example, this might be a key like "G", or a mouse button.
    pub trigger: T,
}

/// Bindings that are triggered by a keyboard key
pub type KeyBinding = Binding<Key>;

/// Bindings that are triggered by a mouse button
pub type MouseBinding = Binding<MouseButton>;

impl Default for KeyBinding {
    fn default() -> KeyBinding {
        KeyBinding {
            mods: Default::default(),
            action: Action::Esc(String::new()),
            mode: TermMode::NONE,
            notmode: TermMode::NONE,
            trigger: Key::A,
        }
    }
}

impl Default for MouseBinding {
    fn default() -> MouseBinding {
        MouseBinding {
            mods: Default::default(),
            action: Action::Esc(String::new()),
            mode: TermMode::NONE,
            notmode: TermMode::NONE,
            trigger: MouseButton::Left,
        }
    }
}

impl<T: Eq> Binding<T> {
    #[inline]
    fn is_triggered_by(
        &self,
        mode: TermMode,
        mods: ModifiersState,
        input: &T,
        relaxed: bool,
    ) -> bool {
        // Check input first since bindings are stored in one big list. This is
        // the most likely item to fail so prioritizing it here allows more
        // checks to be short circuited.
        self.trigger == *input
            && mode.contains(self.mode)
            && !mode.intersects(self.notmode)
            && self.mods_match(mods, relaxed)
    }

    #[inline]
    pub fn triggers_match(&self, binding: &Binding<T>) -> bool {
        // Check the binding's key and modifiers
        if self.trigger != binding.trigger || self.mods != binding.mods {
            return false;
        }

        // Completely empty modes match all modes
        if (self.mode.is_empty() && self.notmode.is_empty())
            || (binding.mode.is_empty() && binding.notmode.is_empty())
        {
            return true;
        }

        // Check for intersection (equality is required since empty does not intersect itself)
        (self.mode == binding.mode || self.mode.intersects(binding.mode))
            && (self.notmode == binding.notmode || self.notmode.intersects(binding.notmode))
    }
}

impl<T> Binding<T> {
    /// Execute the action associate with this binding
    #[inline]
    fn execute<A: ActionContext>(&self, ctx: &mut A, mouse_mode: bool) {
        self.action.execute(ctx, mouse_mode)
    }

    /// Check that two mods descriptions for equivalence
    #[inline]
    fn mods_match(&self, mods: ModifiersState, relaxed: bool) -> bool {
        if relaxed {
            self.mods.relaxed_eq(mods)
        } else {
            self.mods == mods
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum Action {
    /// Write an escape sequence.
    #[serde(skip)]
    Esc(String),

    /// Paste contents of system clipboard.
    Paste,

    /// Store current selection into clipboard.
    Copy,

    /// Paste contents of selection buffer.
    PasteSelection,

    /// Increase font size.
    IncreaseFontSize,

    /// Decrease font size.
    DecreaseFontSize,

    /// Reset font size to the config value.
    ResetFontSize,

    /// Scroll exactly one page up.
    ScrollPageUp,

    /// Scroll exactly one page down.
    ScrollPageDown,

    /// Scroll one line up.
    ScrollLineUp,

    /// Scroll one line down.
    ScrollLineDown,

    /// Scroll all the way to the top.
    ScrollToTop,

    /// Scroll all the way to the bottom.
    ScrollToBottom,

    /// Clear the display buffer(s) to remove history.
    ClearHistory,

    /// Run given command.
    #[serde(skip)]
    Command(String, Vec<String>),

    /// Hide the Alacritty window.
    Hide,

    /// Quit Alacritty.
    Quit,

    /// Clear warning and error notices.
    ClearLogNotice,

    /// Spawn a new instance of Alacritty.
    SpawnNewInstance,

    /// Toggle fullscreen.
    ToggleFullscreen,

    /// Toggle simple fullscreen on macos.
    #[cfg(target_os = "macos")]
    ToggleSimpleFullscreen,

    /// Allow receiving char input.
    ReceiveChar,

    /// No action.
    None,
}

impl Default for Action {
    fn default() -> Action {
        Action::None
    }
}

impl Action {
    #[inline]
    fn execute<A: ActionContext>(&self, ctx: &mut A, mouse_mode: bool) {
        match *self {
            Action::Esc(ref s) => {
                ctx.scroll(Scroll::Bottom);
                ctx.write_to_pty(s.clone().into_bytes())
            },
            Action::Copy => {
                ctx.copy_selection(ClipboardType::Clipboard);
            },
            Action::Paste => {
                let text = ctx.terminal_mut().clipboard().load(ClipboardType::Clipboard);
                self.paste(ctx, &text);
            },
            Action::PasteSelection => {
                // Only paste if mouse events are not captured by an application
                if !mouse_mode {
                    let text = ctx.terminal_mut().clipboard().load(ClipboardType::Selection);
                    self.paste(ctx, &text);
                }
            },
            Action::Command(ref program, ref args) => {
                trace!("Running command {} with args {:?}", program, args);

                match start_daemon(program, args) {
                    Ok(_) => debug!("Spawned new proc"),
                    Err(err) => warn!("Couldn't run command {}", err),
                }
            },
            Action::ToggleFullscreen => {
                ctx.toggle_fullscreen();
            },
            #[cfg(target_os = "macos")]
            Action::ToggleSimpleFullscreen => {
                ctx.toggle_simple_fullscreen();
            },
            Action::Hide => {
                ctx.hide_window();
            },
            Action::Quit => {
                ctx.terminal_mut().exit();
            },
            Action::IncreaseFontSize => {
                ctx.terminal_mut().change_font_size(FONT_SIZE_STEP);
            },
            Action::DecreaseFontSize => {
                ctx.terminal_mut().change_font_size(-FONT_SIZE_STEP);
            },
            Action::ResetFontSize => {
                ctx.terminal_mut().reset_font_size();
            },
            Action::ScrollPageUp => {
                ctx.scroll(Scroll::PageUp);
            },
            Action::ScrollPageDown => {
                ctx.scroll(Scroll::PageDown);
            },
            Action::ScrollLineUp => {
                ctx.scroll(Scroll::Lines(1));
            },
            Action::ScrollLineDown => {
                ctx.scroll(Scroll::Lines(-1));
            },
            Action::ScrollToTop => {
                ctx.scroll(Scroll::Top);
            },
            Action::ScrollToBottom => {
                ctx.scroll(Scroll::Bottom);
            },
            Action::ClearHistory => {
                ctx.terminal_mut().clear_screen(ClearMode::Saved);
            },
            Action::ClearLogNotice => {
                ctx.terminal_mut().message_buffer_mut().pop();
            },
            Action::SpawnNewInstance => {
                ctx.spawn_new_instance();
            },
            Action::ReceiveChar | Action::None => (),
        }
    }

    fn paste<A: ActionContext>(&self, ctx: &mut A, contents: &str) {
        if ctx.terminal().mode().contains(TermMode::BRACKETED_PASTE) {
            ctx.write_to_pty(&b"\x1b[200~"[..]);
            ctx.write_to_pty(contents.replace("\x1b", "").into_bytes());
            ctx.write_to_pty(&b"\x1b[201~"[..]);
        } else {
            // In non-bracketed (ie: normal) mode, terminal applications cannot distinguish
            // pasted data from keystrokes.
            // In theory, we should construct the keystrokes needed to produce the data we are
            // pasting... since that's neither practical nor sensible (and probably an impossible
            // task to solve in a general way), we'll just replace line breaks (windows and unix
            // style) with a single carriage return (\r, which is what the Enter key produces).
            ctx.write_to_pty(contents.replace("\r\n", "\r").replace("\n", "\r").into_bytes());
        }
    }
}

trait RelaxedEq<T: ?Sized = Self> {
    fn relaxed_eq(&self, other: T) -> bool;
}

impl RelaxedEq for ModifiersState {
    // Make sure that modifiers in the config are always present,
    // but ignore surplus modifiers.
    fn relaxed_eq(&self, other: Self) -> bool {
        (!self.logo || other.logo)
            && (!self.alt || other.alt)
            && (!self.ctrl || other.ctrl)
            && (!self.shift || other.shift)
    }
}

impl From<&'static str> for Action {
    fn from(s: &'static str) -> Action {
        Action::Esc(s.into())
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MouseState {
    Url(Url),
    MessageBar,
    MessageBarButton,
    Mouse,
    Text,
}

impl<'a, A: ActionContext + 'a> Processor<'a, A> {
    fn mouse_state(&mut self, point: Point, mods: ModifiersState) -> MouseState {
        let mouse_mode =
            TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG | TermMode::MOUSE_REPORT_CLICK;

        // Check message bar before URL to ignore URLs in the message bar
        if let Some(message) = self.message_at_point(Some(point)) {
            if self.message_close_at_point(point, message) {
                return MouseState::MessageBarButton;
            } else {
                return MouseState::MessageBar;
            }
        }

        // Check for URL at point with required modifiers held
        if self.mouse_config.url.mods().relaxed_eq(mods)
            && (!self.ctx.terminal().mode().intersects(mouse_mode) || mods.shift)
            && self.mouse_config.url.launcher.is_some()
            && self.ctx.selection_is_empty()
            && self.ctx.mouse().left_button_state != ElementState::Pressed
        {
            let buffer_point = self.ctx.terminal().visible_to_buffer(point);
            if let Some(url) =
                self.ctx.terminal().urls().drain(..).find(|url| url.contains(buffer_point))
            {
                return MouseState::Url(url);
            }
        }

        if self.ctx.terminal().mode().intersects(mouse_mode) && !self.ctx.modifiers().shift() {
            MouseState::Mouse
        } else {
            MouseState::Text
        }
    }

    #[inline]
    pub fn mouse_moved(&mut self, x: usize, y: usize, modifiers: ModifiersState) {
        self.ctx.mouse_mut().x = x;
        self.ctx.mouse_mut().y = y;

        let size_info = self.ctx.size_info();
        let point = size_info.pixels_to_coords(x, y);

        let cell_side = self.get_mouse_side();
        let prev_side = mem::replace(&mut self.ctx.mouse_mut().cell_side, cell_side);
        let prev_line = mem::replace(&mut self.ctx.mouse_mut().line, point.line);
        let prev_col = mem::replace(&mut self.ctx.mouse_mut().column, point.col);

        let motion_mode = TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG;
        let report_mode = TermMode::MOUSE_REPORT_CLICK | motion_mode;

        let cell_changed =
            prev_line != self.ctx.mouse().line || prev_col != self.ctx.mouse().column;

        // If the mouse hasn't changed cells, do nothing
        if !cell_changed && prev_side == cell_side {
            return;
        }

        // Don't launch URLs if mouse has moved
        self.ctx.mouse_mut().block_url_launcher = true;

        let mouse_state = self.mouse_state(point, modifiers);
        self.update_mouse_cursor(mouse_state);
        match mouse_state {
            MouseState::Url(url) => {
                let url_bounds = url.linear_bounds(self.ctx.terminal());
                self.ctx.terminal_mut().set_url_highlight(url_bounds);
            },
            MouseState::MessageBar | MouseState::MessageBarButton => {
                self.ctx.terminal_mut().reset_url_highlight();
                return;
            },
            _ => self.ctx.terminal_mut().reset_url_highlight(),
        }

        if self.ctx.mouse().left_button_state == ElementState::Pressed
            && (modifiers.shift || !self.ctx.terminal().mode().intersects(report_mode))
        {
            self.ctx.update_selection(Point { line: point.line, col: point.col }, cell_side);
        } else if self.ctx.terminal().mode().intersects(motion_mode)
            && size_info.contains_point(x, y, false)
            && cell_changed
        {
            if self.ctx.mouse().left_button_state == ElementState::Pressed {
                self.mouse_report(32, ElementState::Pressed, modifiers);
            } else if self.ctx.mouse().middle_button_state == ElementState::Pressed {
                self.mouse_report(33, ElementState::Pressed, modifiers);
            } else if self.ctx.mouse().right_button_state == ElementState::Pressed {
                self.mouse_report(34, ElementState::Pressed, modifiers);
            } else if self.ctx.terminal().mode().contains(TermMode::MOUSE_MOTION) {
                self.mouse_report(35, ElementState::Pressed, modifiers);
            }
        }
    }

    fn get_mouse_side(&self) -> Side {
        let size_info = self.ctx.size_info();
        let x = self.ctx.mouse().x;

        let cell_x = x.saturating_sub(size_info.padding_x as usize) % size_info.cell_width as usize;
        let half_cell_width = (size_info.cell_width / 2.0) as usize;

        let additional_padding =
            (size_info.width - size_info.padding_x * 2.) % size_info.cell_width;
        let end_of_grid = size_info.width - size_info.padding_x - additional_padding;

        if cell_x > half_cell_width
            // Edge case when mouse leaves the window
            || x as f32 >= end_of_grid
        {
            Side::Right
        } else {
            Side::Left
        }
    }

    pub fn normal_mouse_report(&mut self, button: u8) {
        let (line, column) = (self.ctx.mouse().line, self.ctx.mouse().column);

        if line < Line(223) && column < Column(223) {
            let msg = vec![
                b'\x1b',
                b'[',
                b'M',
                32 + button,
                32 + 1 + column.0 as u8,
                32 + 1 + line.0 as u8,
            ];

            self.ctx.write_to_pty(msg);
        }
    }

    pub fn sgr_mouse_report(&mut self, button: u8, state: ElementState) {
        let (line, column) = (self.ctx.mouse().line, self.ctx.mouse().column);
        let c = match state {
            ElementState::Pressed => 'M',
            ElementState::Released => 'm',
        };

        let msg = format!("\x1b[<{};{};{}{}", button, column + 1, line + 1, c);
        self.ctx.write_to_pty(msg.into_bytes());
    }

    pub fn mouse_report(&mut self, button: u8, state: ElementState, modifiers: ModifiersState) {
        // Calculate modifiers value
        let mut mods = 0;
        if modifiers.shift {
            mods += 4;
        }
        if modifiers.alt {
            mods += 8;
        }
        if modifiers.ctrl {
            mods += 16;
        }

        // Report mouse events
        if self.ctx.terminal().mode().contains(TermMode::SGR_MOUSE) {
            self.sgr_mouse_report(button + mods, state);
        } else if let ElementState::Released = state {
            self.normal_mouse_report(3 + mods);
        } else {
            self.normal_mouse_report(button + mods);
        }
    }

    pub fn on_mouse_double_click(&mut self, button: MouseButton, point: Option<Point>) {
        if let (Some(point), true) = (point, button == MouseButton::Left) {
            self.ctx.semantic_selection(point);
        }
    }

    pub fn on_mouse_triple_click(&mut self, button: MouseButton, point: Option<Point>) {
        if let (Some(point), true) = (point, button == MouseButton::Left) {
            self.ctx.line_selection(point);
        }
    }

    pub fn on_mouse_press(
        &mut self,
        button: MouseButton,
        modifiers: ModifiersState,
        point: Option<Point>,
    ) {
        let now = Instant::now();
        let elapsed = self.ctx.mouse().last_click_timestamp.elapsed();
        self.ctx.mouse_mut().last_click_timestamp = now;

        let button_changed = self.ctx.mouse().last_button != button;

        self.ctx.mouse_mut().click_state = match self.ctx.mouse().click_state {
            ClickState::Click
                if !button_changed && elapsed < self.mouse_config.double_click.threshold =>
            {
                self.ctx.mouse_mut().block_url_launcher = true;
                self.on_mouse_double_click(button, point);
                ClickState::DoubleClick
            }
            ClickState::DoubleClick
                if !button_changed && elapsed < self.mouse_config.triple_click.threshold =>
            {
                self.ctx.mouse_mut().block_url_launcher = true;
                self.on_mouse_triple_click(button, point);
                ClickState::TripleClick
            }
            _ => {
                // Don't launch URLs if this click cleared the selection
                self.ctx.mouse_mut().block_url_launcher = !self.ctx.selection_is_empty();

                self.ctx.clear_selection();

                // Start new empty selection
                let side = self.ctx.mouse().cell_side;
                if let Some(point) = point {
                    if modifiers.ctrl {
                        self.ctx.block_selection(point, side);
                    } else {
                        self.ctx.simple_selection(point, side);
                    }
                }

                let report_modes =
                    TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION;
                if !modifiers.shift && self.ctx.terminal().mode().intersects(report_modes) {
                    let code = match button {
                        MouseButton::Left => 0,
                        MouseButton::Middle => 1,
                        MouseButton::Right => 2,
                        // Can't properly report more than three buttons.
                        MouseButton::Other(_) => return,
                    };
                    self.mouse_report(code, ElementState::Pressed, modifiers);
                    return;
                }

                ClickState::Click
            },
        };
    }

    pub fn on_mouse_release(
        &mut self,
        button: MouseButton,
        modifiers: ModifiersState,
        point: Option<Point>,
    ) {
        let report_modes =
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION;
        if !modifiers.shift && self.ctx.terminal().mode().intersects(report_modes) {
            let code = match button {
                MouseButton::Left => 0,
                MouseButton::Middle => 1,
                MouseButton::Right => 2,
                // Can't properly report more than three buttons.
                MouseButton::Other(_) => return,
            };
            self.mouse_report(code, ElementState::Released, modifiers);
            return;
        } else if let (Some(point), true) = (point, button == MouseButton::Left) {
            let mouse_state = self.mouse_state(point, modifiers);
            self.update_mouse_cursor(mouse_state);
            if let MouseState::Url(url) = mouse_state {
                let url_bounds = url.linear_bounds(self.ctx.terminal());
                self.ctx.terminal_mut().set_url_highlight(url_bounds);
                self.launch_url(url);
            }
        }

        self.copy_selection();
    }

    /// Spawn URL launcher when clicking on URLs.
    fn launch_url(&self, url: Url) {
        if self.ctx.mouse().block_url_launcher {
            return;
        }

        if let Some(ref launcher) = self.mouse_config.url.launcher {
            let mut args = launcher.args().to_vec();
            args.push(self.ctx.terminal().url_to_string(url));

            match start_daemon(launcher.program(), &args) {
                Ok(_) => debug!("Launched {} with args {:?}", launcher.program(), args),
                Err(_) => warn!("Unable to launch {} with args {:?}", launcher.program(), args),
            }
        }
    }

    pub fn on_mouse_wheel(
        &mut self,
        delta: MouseScrollDelta,
        phase: TouchPhase,
        modifiers: ModifiersState,
    ) {
        match delta {
            MouseScrollDelta::LineDelta(_columns, lines) => {
                let new_scroll_px = lines * self.ctx.size_info().cell_height;
                self.scroll_terminal(modifiers, new_scroll_px as i32);
            },
            MouseScrollDelta::PixelDelta(lpos) => {
                match phase {
                    TouchPhase::Started => {
                        // Reset offset to zero
                        self.ctx.mouse_mut().scroll_px = 0;
                    },
                    TouchPhase::Moved => {
                        self.scroll_terminal(modifiers, lpos.y as i32);
                    },
                    _ => (),
                }
            },
        }
    }

    fn scroll_terminal(&mut self, modifiers: ModifiersState, new_scroll_px: i32) {
        let mouse_modes =
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION;
        let height = self.ctx.size_info().cell_height as i32;

        // Make sure the new and deprecated setting are both allowed
        let faux_multiplier = self.scrolling_config.faux_multiplier() as usize;

        if self.ctx.terminal().mode().intersects(mouse_modes) {
            self.ctx.mouse_mut().scroll_px += new_scroll_px;

            let code = if new_scroll_px > 0 { 64 } else { 65 };
            let lines = (self.ctx.mouse().scroll_px / height).abs();

            for _ in 0..lines {
                self.mouse_report(code, ElementState::Pressed, modifiers);
            }
        } else if self.ctx.terminal().mode().contains(TermMode::ALT_SCREEN)
            && faux_multiplier > 0
            && !modifiers.shift
        {
            self.ctx.mouse_mut().scroll_px += new_scroll_px * faux_multiplier as i32;

            let cmd = if new_scroll_px > 0 { b'A' } else { b'B' };
            let lines = (self.ctx.mouse().scroll_px / height).abs();

            let mut content = Vec::with_capacity(lines as usize * 3);
            for _ in 0..lines {
                content.push(0x1b);
                content.push(b'O');
                content.push(cmd);
            }
            self.ctx.write_to_pty(content);
        } else {
            let multiplier = i32::from(self.scrolling_config.multiplier());
            self.ctx.mouse_mut().scroll_px += new_scroll_px * multiplier;

            let lines = self.ctx.mouse().scroll_px / height;

            self.ctx.scroll(Scroll::Lines(lines as isize));
        }

        self.ctx.mouse_mut().scroll_px %= height;
    }

    pub fn on_focus_change(&mut self, is_focused: bool) {
        if self.ctx.terminal().mode().contains(TermMode::FOCUS_IN_OUT) {
            let chr = if is_focused { "I" } else { "O" };

            let msg = format!("\x1b[{}", chr);
            self.ctx.write_to_pty(msg.into_bytes());
        }
    }

    pub fn mouse_input(
        &mut self,
        state: ElementState,
        button: MouseButton,
        modifiers: ModifiersState,
    ) {
        match button {
            MouseButton::Left => self.ctx.mouse_mut().left_button_state = state,
            MouseButton::Middle => self.ctx.mouse_mut().middle_button_state = state,
            MouseButton::Right => self.ctx.mouse_mut().right_button_state = state,
            _ => (),
        }

        let point = self.ctx.mouse_coords();

        // Skip normal mouse events if the message bar has been clicked
        if let Some(message) = self.message_at_point(point) {
            // Message should never be `Some` if point is `None`
            debug_assert!(point.is_some());
            self.on_message_bar_click(state, point.unwrap(), message, modifiers);
        } else {
            match state {
                ElementState::Pressed => {
                    self.process_mouse_bindings(modifiers, button);
                    self.on_mouse_press(button, modifiers, point);
                },
                ElementState::Released => self.on_mouse_release(button, modifiers, point),
            }
        }

        self.ctx.mouse_mut().last_button = button;
    }

    /// Process key input.
    pub fn process_key(&mut self, input: KeyboardInput) {
        self.ctx.modifiers().update(input);

        // Update mouse cursor for temporarily disabling mouse mode
        if input.virtual_keycode == Some(VirtualKeyCode::LShift)
            || input.virtual_keycode == Some(VirtualKeyCode::RShift)
        {
            if let Some(point) = self.ctx.mouse_coords() {
                let mods = self.ctx.modifiers().into();
                let mouse_state = self.mouse_state(point, mods);
                self.update_mouse_cursor(mouse_state);
            }
        }

        match input.state {
            ElementState::Pressed => {
                *self.ctx.received_count() = 0;
                self.process_key_bindings(input);
            },
            ElementState::Released => *self.ctx.suppress_chars() = false,
        }
    }

    /// Process a received character.
    pub fn received_char(&mut self, c: char) {
        if *self.ctx.suppress_chars() {
            return;
        }

        self.ctx.scroll(Scroll::Bottom);
        self.ctx.clear_selection();

        let utf8_len = c.len_utf8();
        let mut bytes = Vec::with_capacity(utf8_len);
        unsafe {
            bytes.set_len(utf8_len);
            c.encode_utf8(&mut bytes[..]);
        }

        if self.alt_send_esc
            && *self.ctx.received_count() == 0
            && self.ctx.modifiers().alt()
            && utf8_len == 1
        {
            bytes.insert(0, b'\x1b');
        }

        self.ctx.write_to_pty(bytes);

        *self.ctx.received_count() += 1;
    }

    /// Attempt to find a binding and execute its action.
    ///
    /// The provided mode, mods, and key must match what is allowed by a binding
    /// for its action to be executed.
    fn process_key_bindings(&mut self, input: KeyboardInput) {
        let mode = *self.ctx.terminal().mode();

        *self.ctx.suppress_chars() = self
            .key_bindings
            .iter()
            .filter(|binding| {
                let key = match (binding.trigger, input.virtual_keycode) {
                    (Key::Scancode(_), _) => Key::Scancode(input.scancode),
                    (_, Some(key)) => Key::from_glutin_input(key),
                    _ => return false,
                };

                binding.is_triggered_by(mode, input.modifiers, &key, false)
            })
            .fold(None, |suppress_chars, binding| {
                // Binding was triggered; run the action
                binding.execute(&mut self.ctx, false);

                // Don't suppress when there has been a `ReceiveChar` action
                Some(suppress_chars.unwrap_or(true) && binding.action != Action::ReceiveChar)
            })
            // Don't suppress char if no bindings were triggered
            .unwrap_or(false);
    }

    /// Attempt to find a binding and execute its action.
    ///
    /// The provided mode, mods, and key must match what is allowed by a binding
    /// for its action to be executed.
    fn process_mouse_bindings(&mut self, mods: ModifiersState, button: MouseButton) {
        for binding in self.mouse_bindings {
            if binding.is_triggered_by(*self.ctx.terminal().mode(), mods, &button, true) {
                // binding was triggered; run the action
                let mouse_mode = !mods.shift
                    && self.ctx.terminal().mode().intersects(
                        TermMode::MOUSE_REPORT_CLICK
                            | TermMode::MOUSE_DRAG
                            | TermMode::MOUSE_MOTION,
                    );
                binding.execute(&mut self.ctx, mouse_mode);
            }
        }
    }

    /// Return the message bar's message if there is some at the specified point
    fn message_at_point(&mut self, point: Option<Point>) -> Option<Message> {
        if let (Some(point), Some(message)) =
            (point, self.ctx.terminal_mut().message_buffer_mut().message())
        {
            let size = self.ctx.size_info();
            if point.line.0 >= size.lines().saturating_sub(message.text(&size).len()) {
                return Some(message);
            }
        }

        None
    }

    /// Whether the point is over the message bar's close button
    fn message_close_at_point(&self, point: Point, message: Message) -> bool {
        let size = self.ctx.size_info();
        point.col + message_bar::CLOSE_BUTTON_TEXT.len() >= size.cols()
            && point.line == size.lines() - message.text(&size).len()
    }

    /// Handle clicks on the message bar.
    fn on_message_bar_click(
        &mut self,
        button_state: ElementState,
        point: Point,
        message: Message,
        mods: ModifiersState,
    ) {
        match button_state {
            ElementState::Released => self.copy_selection(),
            ElementState::Pressed => {
                if self.message_close_at_point(point, message) {
                    let mouse_state = self.mouse_state(point, mods);
                    self.update_mouse_cursor(mouse_state);
                    self.ctx.terminal_mut().message_buffer_mut().pop();
                }

                self.ctx.clear_selection();
            },
        }
    }

    /// Copy text selection.
    fn copy_selection(&mut self) {
        if self.save_to_clipboard {
            self.ctx.copy_selection(ClipboardType::Clipboard);
        }
        self.ctx.copy_selection(ClipboardType::Selection);
    }

    #[inline]
    fn update_mouse_cursor(&mut self, mouse_state: MouseState) {
        let mouse_cursor = match mouse_state {
            MouseState::Url(_) | MouseState::MessageBarButton => MouseCursor::Hand,
            MouseState::Text => MouseCursor::Text,
            _ => MouseCursor::Default,
        };

        self.ctx.terminal_mut().set_mouse_cursor(mouse_cursor);
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::time::Duration;

    use glutin::{ElementState, Event, ModifiersState, MouseButton, VirtualKeyCode, WindowEvent};

    use crate::clipboard::{Clipboard, ClipboardType};
    use crate::config::{self, ClickHandler, Config};
    use crate::event::{ClickState, Mouse, WindowChanges};
    use crate::grid::Scroll;
    use crate::index::{Point, Side};
    use crate::message_bar::MessageBuffer;
    use crate::selection::Selection;
    use crate::term::{SizeInfo, Term, TermMode};

    use super::{Action, Binding, Modifiers, Processor};

    const KEY: VirtualKeyCode = VirtualKeyCode::Key0;

    type MockBinding = Binding<usize>;

    impl Default for MockBinding {
        fn default() -> Self {
            Self {
                mods: Default::default(),
                action: Default::default(),
                mode: TermMode::empty(),
                notmode: TermMode::empty(),
                trigger: Default::default(),
            }
        }
    }

    #[test]
    fn binding_matches_itself() {
        let binding = MockBinding::default();
        let identical_binding = MockBinding::default();

        assert!(binding.triggers_match(&identical_binding));
        assert!(identical_binding.triggers_match(&binding));
    }

    #[test]
    fn binding_matches_different_action() {
        let binding = MockBinding::default();
        let mut different_action = MockBinding::default();
        different_action.action = Action::ClearHistory;

        assert!(binding.triggers_match(&different_action));
        assert!(different_action.triggers_match(&binding));
    }

    #[test]
    fn mods_binding_requires_strict_match() {
        let mut superset_mods = MockBinding::default();
        superset_mods.mods = ModifiersState { alt: true, logo: true, ctrl: true, shift: true };
        let mut subset_mods = MockBinding::default();
        subset_mods.mods = ModifiersState { alt: true, logo: false, ctrl: false, shift: false };

        assert!(!superset_mods.triggers_match(&subset_mods));
        assert!(!subset_mods.triggers_match(&superset_mods));
    }

    #[test]
    fn binding_matches_identical_mode() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::ALT_SCREEN;
        let mut b2 = MockBinding::default();
        b2.mode = TermMode::ALT_SCREEN;

        assert!(b1.triggers_match(&b2));
    }

    #[test]
    fn binding_without_mode_matches_any_mode() {
        let b1 = MockBinding::default();
        let mut b2 = MockBinding::default();
        b2.mode = TermMode::APP_KEYPAD;
        b2.notmode = TermMode::ALT_SCREEN;

        assert!(b1.triggers_match(&b2));
    }

    #[test]
    fn binding_with_mode_matches_empty_mode() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::APP_KEYPAD;
        b1.notmode = TermMode::ALT_SCREEN;
        let b2 = MockBinding::default();

        assert!(b1.triggers_match(&b2));
    }

    #[test]
    fn binding_matches_superset_mode() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::APP_KEYPAD;
        let mut b2 = MockBinding::default();
        b2.mode = TermMode::ALT_SCREEN | TermMode::APP_KEYPAD;

        assert!(b1.triggers_match(&b2));
    }

    #[test]
    fn binding_matches_subset_mode() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::ALT_SCREEN | TermMode::APP_KEYPAD;
        let mut b2 = MockBinding::default();
        b2.mode = TermMode::APP_KEYPAD;

        assert!(b1.triggers_match(&b2));
    }

    #[test]
    fn binding_matches_partial_intersection() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::ALT_SCREEN | TermMode::APP_KEYPAD;
        let mut b2 = MockBinding::default();
        b2.mode = TermMode::APP_KEYPAD | TermMode::APP_CURSOR;

        assert!(b1.triggers_match(&b2));
    }

    #[test]
    fn binding_mismatches_notmode() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::ALT_SCREEN;
        let mut b2 = MockBinding::default();
        b2.notmode = TermMode::ALT_SCREEN;

        assert!(!b1.triggers_match(&b2));
    }

    #[test]
    fn binding_mismatches_unrelated() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::ALT_SCREEN;
        let mut b2 = MockBinding::default();
        b2.mode = TermMode::APP_KEYPAD;

        assert!(!b1.triggers_match(&b2));
    }

    #[test]
    fn binding_trigger_input() {
        let mut binding = MockBinding::default();
        binding.trigger = 13;

        let mods = binding.mods;
        let mode = binding.mode;

        assert!(binding.is_triggered_by(mode, mods, &13, true));
        assert!(!binding.is_triggered_by(mode, mods, &32, true));
    }

    #[test]
    fn binding_trigger_mods() {
        let mut binding = MockBinding::default();
        binding.mods = ModifiersState { alt: true, logo: true, ctrl: false, shift: false };

        let superset_mods = ModifiersState { alt: true, logo: true, ctrl: true, shift: true };
        let subset_mods = ModifiersState { alt: false, logo: false, ctrl: false, shift: false };

        let t = binding.trigger;
        let mode = binding.mode;

        assert!(binding.is_triggered_by(mode, binding.mods, &t, true));
        assert!(binding.is_triggered_by(mode, binding.mods, &t, false));

        assert!(binding.is_triggered_by(mode, superset_mods, &t, true));
        assert!(!binding.is_triggered_by(mode, superset_mods, &t, false));

        assert!(!binding.is_triggered_by(mode, subset_mods, &t, true));
        assert!(!binding.is_triggered_by(mode, subset_mods, &t, false));
    }

    #[test]
    fn binding_trigger_modes() {
        let mut binding = MockBinding::default();
        binding.mode = TermMode::ALT_SCREEN;

        let t = binding.trigger;
        let mods = binding.mods;

        assert!(!binding.is_triggered_by(TermMode::INSERT, mods, &t, true));
        assert!(binding.is_triggered_by(TermMode::ALT_SCREEN, mods, &t, true));
        assert!(binding.is_triggered_by(TermMode::ALT_SCREEN | TermMode::INSERT, mods, &t, true));
    }

    #[test]
    fn binding_trigger_notmodes() {
        let mut binding = MockBinding::default();
        binding.notmode = TermMode::ALT_SCREEN;

        let t = binding.trigger;
        let mods = binding.mods;

        assert!(binding.is_triggered_by(TermMode::INSERT, mods, &t, true));
        assert!(!binding.is_triggered_by(TermMode::ALT_SCREEN, mods, &t, true));
        assert!(!binding.is_triggered_by(TermMode::ALT_SCREEN | TermMode::INSERT, mods, &t, true));
    }

    #[derive(PartialEq)]
    enum MultiClick {
        DoubleClick,
        TripleClick,
        None,
    }

    struct ActionContext<'a> {
        pub terminal: &'a mut Term,
        pub selection: &'a mut Option<Selection>,
        pub size_info: &'a SizeInfo,
        pub mouse: &'a mut Mouse,
        pub last_action: MultiClick,
        pub received_count: usize,
        pub suppress_chars: bool,
        pub modifiers: Modifiers,
        pub window_changes: &'a mut WindowChanges,
    }

    impl<'a> super::ActionContext for ActionContext<'a> {
        fn write_to_pty<B: Into<Cow<'static, [u8]>>>(&mut self, _val: B) {}

        fn update_selection(&mut self, _point: Point, _side: Side) {}

        fn simple_selection(&mut self, _point: Point, _side: Side) {}

        fn block_selection(&mut self, _point: Point, _side: Side) {}

        fn copy_selection(&mut self, _: ClipboardType) {}

        fn clear_selection(&mut self) {}

        fn hide_window(&mut self) {}

        fn spawn_new_instance(&mut self) {}

        fn toggle_fullscreen(&mut self) {}

        #[cfg(target_os = "macos")]
        fn toggle_simple_fullscreen(&mut self) {}

        fn terminal(&self) -> &Term {
            &self.terminal
        }

        fn terminal_mut(&mut self) -> &mut Term {
            &mut self.terminal
        }

        fn size_info(&self) -> SizeInfo {
            *self.size_info
        }

        fn semantic_selection(&mut self, _point: Point) {
            // set something that we can check for here
            self.last_action = MultiClick::DoubleClick;
        }

        fn line_selection(&mut self, _point: Point) {
            self.last_action = MultiClick::TripleClick;
        }

        fn selection_is_empty(&self) -> bool {
            true
        }

        fn scroll(&mut self, scroll: Scroll) {
            self.terminal.scroll_display(scroll);
        }

        fn mouse_coords(&self) -> Option<Point> {
            self.terminal.pixels_to_coords(self.mouse.x as usize, self.mouse.y as usize)
        }

        #[inline]
        fn mouse_mut(&mut self) -> &mut Mouse {
            self.mouse
        }

        #[inline]
        fn mouse(&self) -> &Mouse {
            self.mouse
        }

        fn received_count(&mut self) -> &mut usize {
            &mut self.received_count
        }

        fn suppress_chars(&mut self) -> &mut bool {
            &mut self.suppress_chars
        }

        fn modifiers(&mut self) -> &mut Modifiers {
            &mut self.modifiers
        }
    }

    macro_rules! test_clickstate {
        {
            name: $name:ident,
            initial_state: $initial_state:expr,
            initial_button: $initial_button:expr,
            input: $input:expr,
            end_state: $end_state:pat,
            last_action: $last_action:expr
        } => {
            #[test]
            fn $name() {
                let config = Config::default();
                let size = SizeInfo {
                    width: 21.0,
                    height: 51.0,
                    cell_width: 3.0,
                    cell_height: 3.0,
                    padding_x: 0.0,
                    padding_y: 0.0,
                    dpr: 1.0,
                };

                let mut terminal = Term::new(&config, size, MessageBuffer::new(), Clipboard::new_nop());

                let mut mouse = Mouse::default();
                mouse.click_state = $initial_state;
                mouse.last_button = $initial_button;

                let mut selection = None;

                let context = ActionContext {
                    terminal: &mut terminal,
                    selection: &mut selection,
                    mouse: &mut mouse,
                    size_info: &size,
                    last_action: MultiClick::None,
                    received_count: 0,
                    suppress_chars: false,
                    modifiers: Default::default(),
                    window_changes: &mut WindowChanges::default(),
                };

                let mut processor = Processor {
                    ctx: context,
                    mouse_config: &config::Mouse {
                        double_click: ClickHandler {
                            threshold: Duration::from_millis(1000),
                        },
                        triple_click: ClickHandler {
                            threshold: Duration::from_millis(1000),
                        },
                        hide_when_typing: false,
                        url: Default::default(),
                    },
                    scrolling_config: &config::Scrolling::default(),
                    key_bindings: &config.key_bindings[..],
                    mouse_bindings: &config.mouse_bindings[..],
                    save_to_clipboard: config.selection.save_to_clipboard,
                    alt_send_esc: config.alt_send_esc(),
                };

                if let Event::WindowEvent { event: WindowEvent::MouseInput { state, button, modifiers, .. }, .. } = $input {
                    processor.mouse_input(state, button, modifiers);
                };

                assert!(match processor.ctx.mouse.click_state {
                    $end_state => processor.ctx.last_action == $last_action,
                    _ => false
                });
            }
        }
    }

    macro_rules! test_process_binding {
        {
            name: $name:ident,
            binding: $binding:expr,
            triggers: $triggers:expr,
            mode: $mode:expr,
            mods: $mods:expr
        } => {
            #[test]
            fn $name() {
                if $triggers {
                    assert!($binding.is_triggered_by($mode, $mods, &KEY, false));
                } else {
                    assert!(!$binding.is_triggered_by($mode, $mods, &KEY, false));
                }
            }
        }
    }

    test_clickstate! {
        name: single_click,
        initial_state: ClickState::None,
        initial_button: MouseButton::Other(0),
        input: Event::WindowEvent {
            event: WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                device_id: unsafe { ::std::mem::transmute_copy(&0) },
                modifiers: ModifiersState::default(),
            },
            window_id: unsafe { ::std::mem::transmute_copy(&0) },
        },
        end_state: ClickState::Click,
        last_action: MultiClick::None
    }

    test_clickstate! {
        name: double_click,
        initial_state: ClickState::Click,
        initial_button: MouseButton::Left,
        input: Event::WindowEvent {
            event: WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                device_id: unsafe { ::std::mem::transmute_copy(&0) },
                modifiers: ModifiersState::default(),
            },
            window_id: unsafe { ::std::mem::transmute_copy(&0) },
        },
        end_state: ClickState::DoubleClick,
        last_action: MultiClick::DoubleClick
    }

    test_clickstate! {
        name: triple_click,
        initial_state: ClickState::DoubleClick,
        initial_button: MouseButton::Left,
        input: Event::WindowEvent {
            event: WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                device_id: unsafe { ::std::mem::transmute_copy(&0) },
                modifiers: ModifiersState::default(),
            },
            window_id: unsafe { ::std::mem::transmute_copy(&0) },
        },
        end_state: ClickState::TripleClick,
        last_action: MultiClick::TripleClick
    }

    test_clickstate! {
        name: multi_click_separate_buttons,
        initial_state: ClickState::DoubleClick,
        initial_button: MouseButton::Left,
        input: Event::WindowEvent {
            event: WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                device_id: unsafe { ::std::mem::transmute_copy(&0) },
                modifiers: ModifiersState::default(),
            },
            window_id: unsafe { ::std::mem::transmute_copy(&0) },
        },
        end_state: ClickState::Click,
        last_action: MultiClick::None
    }

    test_process_binding! {
        name: process_binding_nomode_shiftmod_require_shift,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: true, ctrl: false, alt: false, logo: false }, action: Action::from("\x1b[1;2D"), mode: TermMode::NONE, notmode: TermMode::NONE },
        triggers: true,
        mode: TermMode::NONE,
        mods: ModifiersState { shift: true, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_shift,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: true, ctrl: false, alt: false, logo: false }, action: Action::from("\x1b[1;2D"), mode: TermMode::NONE, notmode: TermMode::NONE },
        triggers: false,
        mode: TermMode::NONE,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_nomode_controlmod,
        binding: Binding { trigger: KEY, mods: ModifiersState { ctrl: true, shift: false, alt: false, logo: false }, action: Action::from("\x1b[1;5D"), mode: TermMode::NONE, notmode: TermMode::NONE },
        triggers: true,
        mode: TermMode::NONE,
        mods: ModifiersState { ctrl: true, shift: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_not_appcursor,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }, action: Action::from("\x1b[D"), mode: TermMode::NONE, notmode: TermMode::APP_CURSOR },
        triggers: true,
        mode: TermMode::NONE,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_appcursormode_nomod_require_appcursor,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }, action: Action::from("\x1bOD"), mode: TermMode::APP_CURSOR, notmode: TermMode::NONE },
        triggers: true,
        mode: TermMode::APP_CURSOR,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_appcursor,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }, action: Action::from("\x1bOD"), mode: TermMode::APP_CURSOR, notmode: TermMode::NONE },
        triggers: false,
        mode: TermMode::NONE,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_appcursormode_appkeypadmode_nomod_require_appcursor,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }, action: Action::from("\x1bOD"), mode: TermMode::APP_CURSOR, notmode: TermMode::NONE },
        triggers: true,
        mode: TermMode::APP_CURSOR | TermMode::APP_KEYPAD,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_fail_with_extra_mods,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: true }, action: Action::from("arst"), mode: TermMode::NONE, notmode: TermMode::NONE },
        triggers: false,
        mode: TermMode::NONE,
        mods: ModifiersState { shift: false, ctrl: false, alt: true, logo: true }
    }
}
