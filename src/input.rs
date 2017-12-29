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
use std::borrow::Cow;
use std::mem;
use std::process::Command;
use std::time::Instant;
use std::os::unix::process::CommandExt;

use copypasta::{Clipboard, Load, Buffer};
use glutin::{ElementState, VirtualKeyCode, MouseButton, TouchPhase, MouseScrollDelta};
use glutin::ModifiersState;

use config;
use event::{ClickState, Mouse};
use index::{Line, Column, Side, Point};
use term::SizeInfo;
use term::mode::{self, TermMode};
use util::fmt::Red;

/// Processes input from glutin.
///
/// An escape sequence may be emitted in case specific keys or key combinations
/// are activated.
///
/// TODO also need terminal state when processing input
pub struct Processor<'a, A: 'a> {
    pub key_bindings: &'a [KeyBinding],
    pub mouse_bindings: &'a [MouseBinding],
    pub mouse_config: &'a config::Mouse,
    pub ctx: A,
}

pub trait ActionContext {
    fn write_to_pty<B: Into<Cow<'static, [u8]>>>(&mut self, B);
    fn terminal_mode(&self) -> TermMode;
    fn size_info(&self) -> SizeInfo;
    fn copy_selection(&self, Buffer);
    fn clear_selection(&mut self);
    fn update_selection(&mut self, point: Point, side: Side);
    fn simple_selection(&mut self, point: Point, side: Side);
    fn semantic_selection(&mut self, point: Point);
    fn line_selection(&mut self, point: Point);
    fn mouse_mut(&mut self) -> &mut Mouse;
    fn mouse_coords(&self) -> Option<Point>;
    fn received_count(&mut self) -> &mut usize;
    fn suppress_chars(&mut self) -> &mut bool;
    fn last_modifiers(&mut self) -> &mut ModifiersState;
    fn change_font_size(&mut self, delta: i8);
    fn reset_font_size(&mut self);
}

/// Describes a state and action to take in that state
///
/// This is the shared component of `MouseBinding` and `KeyBinding`
#[derive(Debug, Clone)]
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
pub type KeyBinding = Binding<VirtualKeyCode>;

/// Bindings that are triggered by a mouse button
pub type MouseBinding = Binding<MouseButton>;

impl<T: Eq> Binding<T> {
    #[inline]
    fn is_triggered_by(
        &self,
        mode: TermMode,
        mods: &ModifiersState,
        input: &T
    ) -> bool {
        // Check input first since bindings are stored in one big list. This is
        // the most likely item to fail so prioritizing it here allows more
        // checks to be short circuited.
        self.trigger == *input &&
            self.mode_matches(&mode) &&
            self.not_mode_matches(&mode) &&
            self.mods_match(mods)
    }
}

impl<T> Binding<T> {
    /// Execute the action associate with this binding
    #[inline]
    fn execute<A: ActionContext>(&self, ctx: &mut A) {
        self.action.execute(ctx)
    }

    #[inline]
    fn mode_matches(&self, mode: &TermMode) -> bool {
        self.mode.is_empty() || mode.intersects(self.mode)
    }

    #[inline]
    fn not_mode_matches(&self, mode: &TermMode) -> bool {
        self.notmode.is_empty() || !mode.intersects(self.notmode)
    }

    /// Check that two mods descriptions for equivalence
    ///
    /// Optimized to use single check instead of four (one per modifier)
    #[inline]
    fn mods_match(&self, mods: &ModifiersState) -> bool {
        debug_assert!(4 == mem::size_of::<ModifiersState>());
        unsafe {
            mem::transmute_copy::<_, u32>(&self.mods) == mem::transmute_copy::<_, u32>(mods)
        }
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Write an escape sequence
    Esc(String),

    /// Paste contents of system clipboard
    Paste,

    // Store current selection into clipboard
    Copy,

    /// Paste contents of selection buffer
    PasteSelection,

    /// Increase font size
    IncreaseFontSize,

    /// Decrease font size
    DecreaseFontSize,

    /// Reset font size to the config value
    ResetFontSize,

    /// Run given command
    Command(String, Vec<String>),

    /// Quits Alacritty.
    Quit,
}

impl Action {
    #[inline]
    fn execute<A: ActionContext>(&self, ctx: &mut A) {
        match *self {
            Action::Esc(ref s) => {
                ctx.write_to_pty(s.clone().into_bytes())
            },
            Action::Copy => {
                ctx.copy_selection(Buffer::Primary);
            },
            Action::Paste => {
                Clipboard::new()
                    .and_then(|clipboard| clipboard.load_primary() )
                    .map(|contents| { self.paste(ctx, contents) })
                    .unwrap_or_else(|err| {
                        eprintln!("Error loading data from clipboard. {}", Red(err));
                    });
            },
            Action::PasteSelection => {
                Clipboard::new()
                    .and_then(|clipboard| clipboard.load_selection() )
                    .map(|contents| { self.paste(ctx, contents) })
                    .unwrap_or_else(|err| {
                        warn!("Error loading data from clipboard. {}", Red(err));
                    });
            },
            Action::Command(ref program, ref args) => {
                trace!("running command: {} {:?}", program, args);
                match Command::new(program)
                    .args(args)
                    .before_exec(|| {
                        // Detach forked process from Alacritty. This will cause
                        // init or whatever to clean up child processes for us.
                        unsafe { ::libc::daemon(1, 0); }
                        Ok(())
                    })
                    .spawn()
                {
                    Ok(child) => {
                        debug!("spawned new proc with pid: {}", child.id());
                    },
                    Err(err) => {
                        warn!("couldn't run command: {}", err);
                    },
                }
            },
            Action::Quit => {
                // FIXME should do a more graceful shutdown
                ::std::process::exit(0);
            },
            Action::IncreaseFontSize => {
               ctx.change_font_size(1);
            },
            Action::DecreaseFontSize => {
               ctx.change_font_size(-1);
            }
            Action::ResetFontSize => {
               ctx.reset_font_size();
            }
        }
    }

    fn paste<A: ActionContext>(&self, ctx: &mut A, contents: String) {
        if ctx.terminal_mode().contains(mode::TermMode::BRACKETED_PASTE) {
            ctx.write_to_pty(&b"\x1b[200~"[..]);
            ctx.write_to_pty(contents.into_bytes());
            ctx.write_to_pty(&b"\x1b[201~"[..]);
        } else {
            ctx.write_to_pty(contents.into_bytes());
        }
    }
}

impl From<&'static str> for Action {
    fn from(s: &'static str) -> Action {
        Action::Esc(s.into())
    }
}

impl<'a, A: ActionContext + 'a> Processor<'a, A> {
    #[inline]
    pub fn mouse_moved(&mut self, x: u32, y: u32) {
        self.ctx.mouse_mut().x = x;
        self.ctx.mouse_mut().y = y;

        let size_info = self.ctx.size_info();
        if let Some(point) = size_info.pixels_to_coords(x as usize, y as usize) {
            let prev_line = mem::replace(&mut self.ctx.mouse_mut().line, point.line);
            let prev_col = mem::replace(&mut self.ctx.mouse_mut().column, point.col);

            let cell_x = (x as usize - size_info.padding_x as usize) % size_info.cell_width as usize;
            let half_cell_width = (size_info.cell_width / 2.0) as usize;

            let cell_side = if cell_x > half_cell_width {
                Side::Right
            } else {
                Side::Left
            };
            self.ctx.mouse_mut().cell_side = cell_side;

            if self.ctx.mouse_mut().left_button_state == ElementState::Pressed {
                let report_mode = mode::TermMode::MOUSE_REPORT_CLICK | mode::TermMode::MOUSE_MOTION;
                if !self.ctx.terminal_mode().intersects(report_mode) {
                    self.ctx.update_selection(Point {
                        line: point.line,
                        col: point.col
                    }, cell_side);
                } else if self.ctx.terminal_mode().contains(mode::TermMode::MOUSE_MOTION)
                        // Only report motion when changing cells
                        && (
                            prev_line != self.ctx.mouse_mut().line
                            || prev_col != self.ctx.mouse_mut().column
                        ) {
                        self.mouse_report(32);
                }
            }
        }
    }

    pub fn normal_mouse_report(&mut self, button: u8) {
        let (line, column) = (self.ctx.mouse_mut().line, self.ctx.mouse_mut().column);

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

    pub fn sgr_mouse_report(&mut self, button: u8, release: bool) {
        let (line, column) = (self.ctx.mouse_mut().line, self.ctx.mouse_mut().column);
        let c = if release { 'm' } else { 'M' };

        let msg = format!("\x1b[<{};{};{}{}", button, column + 1, line + 1, c);
        self.ctx.write_to_pty(msg.into_bytes());
    }

    pub fn mouse_report(&mut self, button: u8) {
        if self.ctx.terminal_mode().contains(mode::TermMode::SGR_MOUSE) {
            let release = self.ctx.mouse_mut().left_button_state != ElementState::Pressed;
            self.sgr_mouse_report(button, release);
        } else {
            self.normal_mouse_report(button);
        }
    }

    pub fn on_mouse_double_click(&mut self) {
        if let Some(point) = self.ctx.mouse_coords() {
            self.ctx.semantic_selection(point);
        }
    }

    pub fn on_mouse_triple_click(&mut self) {
        if let Some(point) = self.ctx.mouse_coords() {
            self.ctx.line_selection(point);
        }
    }

    pub fn on_mouse_press(&mut self) {
        let now = Instant::now();
        let elapsed = self.ctx.mouse_mut().last_click_timestamp.elapsed();
        self.ctx.mouse_mut().last_click_timestamp = now;

        self.ctx.mouse_mut().click_state = match self.ctx.mouse_mut().click_state {
            ClickState::Click if elapsed < self.mouse_config.double_click.threshold => {
                self.on_mouse_double_click();
                ClickState::DoubleClick
            },
            ClickState::DoubleClick if elapsed < self.mouse_config.triple_click.threshold => {
                self.on_mouse_triple_click();
                ClickState::TripleClick
            },
            _ => {
                let report_modes = mode::TermMode::MOUSE_REPORT_CLICK | mode::TermMode::MOUSE_MOTION;
                if self.ctx.terminal_mode().intersects(report_modes) {
                    self.mouse_report(0);
                    return;
                }

                self.ctx.clear_selection();
                ClickState::Click
            }
        };
    }

    pub fn on_mouse_release(&mut self) {
        if self.ctx.terminal_mode().intersects(mode::TermMode::MOUSE_REPORT_CLICK | mode::TermMode::MOUSE_MOTION) {
            self.mouse_report(3);
            return;
        }

        self.ctx.copy_selection(Buffer::Selection);
    }

    pub fn on_mouse_wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase) {
        let modes = mode::TermMode::MOUSE_REPORT_CLICK | mode::TermMode::MOUSE_MOTION | mode::TermMode::SGR_MOUSE |
            mode::TermMode::ALT_SCREEN;
        if !self.ctx.terminal_mode().intersects(modes) {
            return;
        }

        match delta {
            MouseScrollDelta::LineDelta(_columns, lines) => {
                let to_scroll = self.ctx.mouse_mut().lines_scrolled + lines;
                let code = if to_scroll > 0.0 {
                    64
                } else {
                    65
                };

                for _ in 0..(to_scroll.abs() as usize) {
                    if self.ctx.terminal_mode().intersects(mode::TermMode::ALT_SCREEN) {
                        // Faux scrolling
                        if code == 64 {
                            // Scroll up one line
                            self.ctx.write_to_pty("\x1bOA".as_bytes());
                        } else {
                            // Scroll down one line
                            self.ctx.write_to_pty("\x1bOB".as_bytes());
                        }
                    } else {
                        self.normal_mouse_report(code);
                    }
                }

                self.ctx.mouse_mut().lines_scrolled = to_scroll % 1.0;
            },
            MouseScrollDelta::PixelDelta(_x, y) => {
                match phase {
                    TouchPhase::Started => {
                        // Reset offset to zero
                        self.ctx.mouse_mut().scroll_px = 0;
                    },
                    TouchPhase::Moved => {
                        self.ctx.mouse_mut().scroll_px += y as i32;
                        let height = self.ctx.size_info().cell_height as i32;

                        while self.ctx.mouse_mut().scroll_px.abs() >= height {
                            let button = if self.ctx.mouse_mut().scroll_px > 0 {
                                self.ctx.mouse_mut().scroll_px -= height;
                                64
                            } else {
                                self.ctx.mouse_mut().scroll_px += height;
                                65
                            };

                            if self.ctx.terminal_mode().intersects(mode::TermMode::ALT_SCREEN) {
                                // Faux scrolling
                                if button == 64 {
                                    // Scroll up one line
                                    self.ctx.write_to_pty("\x1bOA".as_bytes());
                                } else {
                                    // Scroll down one line
                                    self.ctx.write_to_pty("\x1bOB".as_bytes());
                                }
                            } else {
                                self.normal_mouse_report(button);
                            }
                        }
                    },
                    _ => (),
                }
            }
        }
    }

    pub fn on_focus_change(&mut self, is_focused: bool) {
        if self.ctx.terminal_mode().contains(mode::TermMode::FOCUS_IN_OUT) {
            let chr = if is_focused {
                "I"
            } else {
                "O"
            };

            let msg = format!("\x1b[{}", chr);
            self.ctx.write_to_pty(msg.into_bytes());
        }
    }

    pub fn mouse_input(&mut self, state: ElementState, button: MouseButton) {
        if let MouseButton::Left = button {
            let state = mem::replace(&mut self.ctx.mouse_mut().left_button_state, state);
            if self.ctx.mouse_mut().left_button_state != state {
                match self.ctx.mouse_mut().left_button_state {
                    ElementState::Pressed => {
                        self.on_mouse_press();
                    },
                    ElementState::Released => {
                        self.on_mouse_release();
                    }
                }
            }
        }

        if let ElementState::Released = state {
            return;
        }

        self.process_mouse_bindings(&ModifiersState::default(), button);
    }

    /// Process key input
    ///
    /// If a keybinding was run, returns true. Otherwise returns false.
    pub fn process_key(
        &mut self,
        state: ElementState,
        key: Option<VirtualKeyCode>,
        mods: &ModifiersState,
    ) {
        match (key, state) {
            (Some(key), ElementState::Pressed) => {
                *self.ctx.last_modifiers() = *mods;
                *self.ctx.received_count() = 0;
                *self.ctx.suppress_chars() = false;

                if self.process_key_bindings(mods, key) {
                    *self.ctx.suppress_chars() = true;
                }
            },
            (_, ElementState::Released) => *self.ctx.suppress_chars() = false,
            _ => ()
        }
    }

    /// Process a received character
    pub fn received_char(&mut self, c: char) {
        if !*self.ctx.suppress_chars() {
            self.ctx.clear_selection();

            let utf8_len = c.len_utf8();
            if *self.ctx.received_count() == 0 && self.ctx.last_modifiers().alt && utf8_len == 1 {
                self.ctx.write_to_pty(b"\x1b".to_vec());
            }

            let mut bytes = Vec::with_capacity(utf8_len);
            unsafe {
                bytes.set_len(utf8_len);
                c.encode_utf8(&mut bytes[..]);
            }

            self.ctx.write_to_pty(bytes);

            *self.ctx.received_count() += 1;
        }
    }

    /// Attempts to find a binding and execute its action
    ///
    /// The provided mode, mods, and key must match what is allowed by a binding
    /// for its action to be executed.
    ///
    /// Returns true if an action is executed.
    fn process_key_bindings(&mut self, mods: &ModifiersState, key: VirtualKeyCode) -> bool {
        for binding in self.key_bindings {
            if binding.is_triggered_by(self.ctx.terminal_mode(), mods, &key) {
                // binding was triggered; run the action
                binding.execute(&mut self.ctx);
                return true;
            }
        }

        false
    }

    /// Attempts to find a binding and execute its action
    ///
    /// The provided mode, mods, and key must match what is allowed by a binding
    /// for its action to be executed.
    ///
    /// Returns true if an action is executed.
    fn process_mouse_bindings(&mut self, mods: &ModifiersState, button: MouseButton) -> bool {
        for binding in self.mouse_bindings {
            if binding.is_triggered_by(self.ctx.terminal_mode(), mods, &button) {
                // binding was triggered; run the action
                binding.execute(&mut self.ctx);
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::time::Duration;

    use glutin::{VirtualKeyCode, Event, WindowEvent, ElementState, MouseButton, ModifiersState};

    use term::{SizeInfo, Term, TermMode, mode};
    use event::{Mouse, ClickState};
    use config::{self, Config, ClickHandler};
    use index::{Point, Side};
    use selection::Selection;

    use super::{Action, Binding, Processor};

    const KEY: VirtualKeyCode = VirtualKeyCode::Key0;

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
        pub last_modifiers: ModifiersState,
    }

    impl <'a>super::ActionContext for ActionContext<'a> {
        fn write_to_pty<B: Into<Cow<'static, [u8]>>>(&mut self, _val: B) {
            // STUBBED
        }

        fn terminal_mode(&self) -> TermMode {
            *self.terminal.mode()
        }

        fn size_info(&self) -> SizeInfo {
            *self.size_info
        }

        fn copy_selection(&self, _buffer: ::copypasta::Buffer) {
            // STUBBED
        }

        fn clear_selection(&mut self) {}
        fn update_selection(&mut self, _point: Point, _side: Side) {}
        fn simple_selection(&mut self, _point: Point, _side: Side) {}

        fn semantic_selection(&mut self, _point: Point) {
            // set something that we can check for here
            self.last_action = MultiClick::DoubleClick;
        }

        fn line_selection(&mut self, _point: Point) {
            self.last_action = MultiClick::TripleClick;
        }

        fn mouse_coords(&self) -> Option<Point> {
            self.terminal.pixels_to_coords(self.mouse.x as usize, self.mouse.y as usize)
        }

        #[inline]
        fn mouse_mut(&mut self) -> &mut Mouse {
            self.mouse
        }
        fn received_count(&mut self) -> &mut usize {
            &mut self.received_count
        }
        fn suppress_chars(&mut self) -> &mut bool {
            &mut self.suppress_chars
        }
        fn last_modifiers(&mut self) -> &mut ModifiersState {
            &mut self.last_modifiers
        }
        fn change_font_size(&mut self, _delta: i8) {
        }
        fn reset_font_size(&mut self) {
        }
    }

    macro_rules! test_clickstate {
        {
            name: $name:ident,
            initial_state: $initial_state:expr,
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
                };

                let mut terminal = Term::new(&config, size);

                let mut mouse = Mouse::default();
                mouse.click_state = $initial_state;

                let mut selection = None;

                let context = ActionContext {
                    terminal: &mut terminal,
                    selection: &mut selection,
                    mouse: &mut mouse,
                    size_info: &size,
                    last_action: MultiClick::None,
                    received_count: 0,
                    suppress_chars: false,
                    last_modifiers: ModifiersState::default(),
                };

                let mut processor = Processor {
                    ctx: context,
                    mouse_config: &config::Mouse {
                        double_click: ClickHandler {
                            threshold: Duration::from_millis(1000),
                        },
                        triple_click: ClickHandler {
                            threshold: Duration::from_millis(1000),
                        }
                    },
                    key_bindings: &config.key_bindings()[..],
                    mouse_bindings: &config.mouse_bindings()[..],
                };

                if let Event::WindowEvent { event: WindowEvent::MouseInput { state, button, .. }, .. } = $input {
                    processor.mouse_input(state, button);
                };

                assert!(match mouse.click_state {
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
                    assert!($binding.is_triggered_by($mode, &$mods, &KEY));
                } else {
                    assert!(!$binding.is_triggered_by($mode, &$mods, &KEY));
                }
            }
        }
    }

    test_clickstate! {
        name: single_click,
        initial_state: ClickState::None,
        input: Event::WindowEvent {
            event: WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                device_id: unsafe { ::std::mem::transmute_copy(&0) },
            },
            window_id: unsafe { ::std::mem::transmute_copy(&0) },
        },
        end_state: ClickState::Click,
        last_action: MultiClick::None
    }

    test_clickstate! {
        name: double_click,
        initial_state: ClickState::Click,
        input: Event::WindowEvent {
            event: WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                device_id: unsafe { ::std::mem::transmute_copy(&0) },
            },
            window_id: unsafe { ::std::mem::transmute_copy(&0) },
        },
        end_state: ClickState::DoubleClick,
        last_action: MultiClick::DoubleClick
    }

    test_clickstate! {
        name: triple_click,
        initial_state: ClickState::DoubleClick,
        input: Event::WindowEvent {
            event: WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                device_id: unsafe { ::std::mem::transmute_copy(&0) },
            },
            window_id: unsafe { ::std::mem::transmute_copy(&0) },
        },
        end_state: ClickState::TripleClick,
        last_action: MultiClick::TripleClick
    }

    test_process_binding! {
        name: process_binding_nomode_shiftmod_require_shift,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: true, ctrl: false, alt: false, logo: false }, action: Action::from("\x1b[1;2D"), mode: mode::TermMode::NONE, notmode: mode::TermMode::NONE },
        triggers: true,
        mode: mode::TermMode::NONE,
        mods: ModifiersState { shift: true, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_shift,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: true, ctrl: false, alt: false, logo: false }, action: Action::from("\x1b[1;2D"), mode: mode::TermMode::NONE, notmode: mode::TermMode::NONE },
        triggers: false,
        mode: mode::TermMode::NONE,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_nomode_controlmod,
        binding: Binding { trigger: KEY, mods: ModifiersState { ctrl: true, shift: false, alt: false, logo: false }, action: Action::from("\x1b[1;5D"), mode: mode::TermMode::NONE, notmode: mode::TermMode::NONE },
        triggers: true,
        mode: mode::TermMode::NONE,
        mods: ModifiersState { ctrl: true, shift: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_not_appcursor,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }, action: Action::from("\x1b[D"), mode: mode::TermMode::NONE, notmode: mode::TermMode::APP_CURSOR },
        triggers: true,
        mode: mode::TermMode::NONE,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_appcursormode_nomod_require_appcursor,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }, action: Action::from("\x1bOD"), mode: mode::TermMode::APP_CURSOR, notmode: mode::TermMode::NONE },
        triggers: true,
        mode: mode::TermMode::APP_CURSOR,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_appcursor,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }, action: Action::from("\x1bOD"), mode: mode::TermMode::APP_CURSOR, notmode: mode::TermMode::NONE },
        triggers: false,
        mode: mode::TermMode::NONE,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_appcursormode_appkeypadmode_nomod_require_appcursor,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }, action: Action::from("\x1bOD"), mode: mode::TermMode::APP_CURSOR, notmode: mode::TermMode::NONE },
        triggers: true,
        mode: mode::TermMode::APP_CURSOR | mode::TermMode::APP_KEYPAD,
        mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: false }
    }

    test_process_binding! {
        name: process_binding_fail_with_extra_mods,
        binding: Binding { trigger: KEY, mods: ModifiersState { shift: false, ctrl: false, alt: false, logo: true }, action: Action::from("arst"), mode: mode::TermMode::NONE, notmode: mode::TermMode::NONE },
        triggers: false,
        mode: mode::TermMode::NONE,
        mods: ModifiersState { shift: false, ctrl: false, alt: true, logo: true }
    }
}
