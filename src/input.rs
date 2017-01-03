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
use std::mem;

use copypasta::{Clipboard, Load, Store};
use glutin::{ElementState, VirtualKeyCode, MouseButton};
use glutin::{Mods, mods};
use glutin::{TouchPhase, MouseScrollDelta};

use event::{Mouse, Notify};
use index::{Line, Column, Side, Point};
use selection::Selection;
use term::mode::{self, TermMode};
use term::{self, Term};
use util::fmt::Red;

/// Processes input from glutin.
///
/// An escape sequence may be emitted in case specific keys or key combinations
/// are activated.
///
/// TODO also need terminal state when processing input
pub struct Processor<'a, N: 'a> {
    pub key_bindings: &'a [KeyBinding],
    pub mouse_bindings: &'a [MouseBinding],
    pub ctx: ActionContext<'a, N>,
}

pub struct ActionContext<'a, N: 'a> {
    pub notifier: &'a mut N,
    pub terminal: &'a mut Term,
    pub selection: &'a mut Selection,
    pub mouse: &'a mut Mouse,
    pub size_info: &'a term::SizeInfo,
}

/// Describes a state and action to take in that state
///
/// This is the shared component of `MouseBinding` and `KeyBinding`
#[derive(Debug, Clone)]
pub struct Binding<T> {
    /// Modifier keys required to activate binding
    pub mods: Mods,

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
        mode: &TermMode,
        mods: &Mods,
        input: &T
    ) -> bool {
        // Check input first since bindings are stored in one big list. This is
        // the most likely item to fail so prioritizing it here allows more
        // checks to be short circuited.
        self.trigger == *input &&
            self.mode_matches(mode) &&
            self.not_mode_matches(mode) &&
            self.mods_match(mods)
    }
}

impl<T> Binding<T> {
    /// Execute the action associate with this binding
    #[inline]
    fn execute<'a, N: Notify>(&self, ctx: &mut ActionContext<'a, N>) {
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

    #[inline]
    fn mods_match(&self, mods: &Mods) -> bool {
        self.mods.is_all() || *mods == self.mods
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
}

impl Action {
    #[inline]
    fn execute<'a, N: Notify>(&self, ctx: &mut ActionContext<'a, N>) {
        match *self {
            Action::Esc(ref s) => {
                ctx.notifier.notify(s.clone().into_bytes())
            },
            Action::Copy => {
                if let Some(selection) = ctx.selection.span() {
                    let buf = ctx.terminal.string_from_selection(&selection);
                    if !buf.is_empty() {
                        Clipboard::new()
                            .and_then(|mut clipboard| clipboard.store_primary(buf))
                            .unwrap_or_else(|err| {
                                err_println!("Error storing selection to clipboard. {}", Red(err));
                            });
                    }
                }
            },
            Action::Paste |
            Action::PasteSelection => {
                Clipboard::new()
                    .and_then(|clipboard| clipboard.load_selection())
                    .map(|contents| {
                        if ctx.terminal.mode().contains(mode::BRACKETED_PASTE) {
                            ctx.notifier.notify(&b"\x1b[200~"[..]);
                            ctx.notifier.notify(contents.into_bytes());
                            ctx.notifier.notify(&b"\x1b[201~"[..]);
                        } else {
                            ctx.notifier.notify(contents.into_bytes());
                        }
                    })
                    .unwrap_or_else(|err| {
                        err_println!("Error loading data from clipboard. {}", Red(err));
                    });
            },
        }
    }
}

impl From<&'static str> for Action {
    fn from(s: &'static str) -> Action {
        Action::Esc(s.into())
    }
}

impl<'a, N: Notify + 'a> Processor<'a, N> {
    #[inline]
    pub fn mouse_moved(&mut self, x: u32, y: u32) {
        self.ctx.mouse.x = x;
        self.ctx.mouse.y = y;

        if let Some(point) = self.ctx.size_info.pixels_to_coords(x as usize, y as usize) {
            let prev_line = mem::replace(&mut self.ctx.mouse.line, point.line);
            let prev_col = mem::replace(&mut self.ctx.mouse.column, point.col);

            let cell_x = x as usize % self.ctx.size_info.cell_width as usize;
            let half_cell_width = (self.ctx.size_info.cell_width / 2.0) as usize;

            self.ctx.mouse.cell_side = if cell_x > half_cell_width {
                Side::Right
            } else {
                Side::Left
            };

            if self.ctx.mouse.left_button_state == ElementState::Pressed {
                let report_mode = mode::MOUSE_REPORT_CLICK | mode::MOUSE_MOTION;
                if !self.ctx.terminal.mode().intersects(report_mode) {
                    self.ctx.selection.update(Point {
                        line: point.line,
                        col: point.col
                    }, self.ctx.mouse.cell_side);
                } else if self.ctx.terminal.mode().contains(mode::MOUSE_MOTION) {
                    // Only report motion when changing cells
                    if prev_line != self.ctx.mouse.line || prev_col != self.ctx.mouse.column {
                        self.mouse_report(0 + 32);
                    }
                }
            }
        }
    }

    pub fn mouse_moved_cells(&mut self) {

    }

    pub fn normal_mouse_report(&mut self, button: u8) {
        let (line, column) = (self.ctx.mouse.line, self.ctx.mouse.column);

        if line < Line(223) && column < Column(223) {
            let msg = vec![
                '\x1b' as u8,
                '[' as u8,
                'M' as u8,
                32 + button,
                32 + 1 + column.0 as u8,
                32 + 1 + line.0 as u8,
            ];

            self.ctx.notifier.notify(msg);
        }
    }

    pub fn sgr_mouse_report(&mut self, button: u8, release: bool) {
        let (line, column) = (self.ctx.mouse.line, self.ctx.mouse.column);
        let c = if release { 'm' } else { 'M' };

        let msg = format!("\x1b[<{};{};{}{}", button, column + 1, line + 1, c);
        self.ctx.notifier.notify(msg.into_bytes());
    }

    pub fn mouse_report(&mut self, button: u8) {
        if self.ctx.terminal.mode().contains(mode::SGR_MOUSE) {
            let release = self.ctx.mouse.left_button_state != ElementState::Pressed;
            self.sgr_mouse_report(button, release);
        } else {
            self.normal_mouse_report(button);
        }
    }

    pub fn on_mouse_press(&mut self) {
        if self.ctx.terminal.mode().intersects(mode::MOUSE_REPORT_CLICK | mode::MOUSE_MOTION) {
            self.mouse_report(0);
            return;
        }

        self.ctx.selection.clear();
    }

    pub fn on_mouse_release(&mut self) {
        if self.ctx.terminal.mode().intersects(mode::MOUSE_REPORT_CLICK | mode::MOUSE_MOTION) {
            self.mouse_report(3);
            return;
        }

        if let Some(selection) = self.ctx.selection.span() {
            let buf = self.ctx.terminal.string_from_selection(&selection);
            if !buf.is_empty() {
                Clipboard::new()
                    .and_then(|mut clipboard| clipboard.store_selection(buf))
                    .unwrap_or_else(|err| {
                        err_println!("Error storing selection to clipboard. {}", Red(err));
                    });
            }
        }
    }

    pub fn on_mouse_wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase) {
        match delta {
            MouseScrollDelta::LineDelta(_columns, lines) => {
                let code = if lines > 0.0 {
                    64
                } else {
                    65
                };

                for _ in 0..(lines.abs() as usize) {
                    self.mouse_report(code);
                }
            },
            MouseScrollDelta::PixelDelta(_x, y) => {
                match phase {
                    TouchPhase::Started => {
                        // Reset offset to zero
                        self.ctx.mouse.scroll_px = 0;
                    },
                    TouchPhase::Moved => {
                        self.ctx.mouse.scroll_px += y as i32;
                        let height = self.ctx.size_info.cell_height as i32;

                        while self.ctx.mouse.scroll_px.abs() >= height {
                            let button = if self.ctx.mouse.scroll_px > 0 {
                                self.ctx.mouse.scroll_px -= height;
                                64
                            } else {
                                self.ctx.mouse.scroll_px += height;
                                65
                            };

                            self.mouse_report(button);
                        }
                    },
                    _ => (),
                }
            }
        }
    }

    pub fn mouse_input(&mut self, state: ElementState, button: MouseButton) {
        if let MouseButton::Left = button {
            let state = mem::replace(&mut self.ctx.mouse.left_button_state, state);
            if self.ctx.mouse.left_button_state != state {
                match self.ctx.mouse.left_button_state {
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

        self.process_mouse_bindings(mods::NONE, button);
    }

    pub fn process_key(
        &mut self,
        state: ElementState,
        key: Option<VirtualKeyCode>,
        mods: Mods,
        string: Option<String>,
    ) {
        if let Some(key) = key {
            // Ignore release events
            if state == ElementState::Released {
                return;
            }

            if self.process_key_bindings(mods, key) {
                return;
            }

            // Didn't process a binding; print the provided character
            if let Some(string) = string {
                self.ctx.notifier.notify(string.into_bytes());
                self.ctx.selection.clear();
            }
        }
    }

    /// Attempts to find a binding and execute its action
    ///
    /// The provided mode, mods, and key must match what is allowed by a binding
    /// for its action to be executed.
    ///
    /// Returns true if an action is executed.
    fn process_key_bindings(&mut self, mods: Mods, key: VirtualKeyCode) -> bool {
        for binding in self.key_bindings {
            if binding.is_triggered_by(self.ctx.terminal.mode(), &mods, &key) {
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
    fn process_mouse_bindings(&mut self, mods: Mods, button: MouseButton) -> bool {
        for binding in self.mouse_bindings {
            if binding.is_triggered_by(self.ctx.terminal.mode(), &mods, &button) {
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
    use glutin::{mods, VirtualKeyCode};

    use term::mode;

    use super::{Action, Binding};

    const KEY: VirtualKeyCode = VirtualKeyCode::Key0;

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
                    assert!($binding.is_triggered_by(&$mode, &$mods, &KEY));
                } else {
                    assert!(!$binding.is_triggered_by(&$mode, &$mods, &KEY));
                }
            }
        }
    }

    test_process_binding! {
        name: process_binding_nomode_shiftmod_require_shift,
        binding: Binding { trigger: KEY, mods: mods::SHIFT, action: Action::from("\x1b[1;2D"), mode: mode::NONE, notmode: mode::NONE },
        triggers: true,
        mode: mode::NONE,
        mods: mods::SHIFT
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_shift,
        binding: Binding { trigger: KEY, mods: mods::SHIFT, action: Action::from("\x1b[1;2D"), mode: mode::NONE, notmode: mode::NONE },
        triggers: false,
        mode: mode::NONE,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_nomode_controlmod,
        binding: Binding { trigger: KEY, mods: mods::CONTROL, action: Action::from("\x1b[1;5D"), mode: mode::NONE, notmode: mode::NONE },
        triggers: true,
        mode: mode::NONE,
        mods: mods::CONTROL
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_not_appcursor,
        binding: Binding { trigger: KEY, mods: mods::ANY, action: Action::from("\x1b[D"), mode: mode::NONE, notmode: mode::APP_CURSOR },
        triggers: true,
        mode: mode::NONE,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_appcursormode_nomod_require_appcursor,
        binding: Binding { trigger: KEY, mods: mods::ANY, action: Action::from("\x1bOD"), mode: mode::APP_CURSOR, notmode: mode::NONE },
        triggers: true,
        mode: mode::APP_CURSOR,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_appcursor,
        binding: Binding { trigger: KEY, mods: mods::ANY, action: Action::from("\x1bOD"), mode: mode::APP_CURSOR, notmode: mode::NONE },
        triggers: false,
        mode: mode::NONE,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_appcursormode_appkeypadmode_nomod_require_appcursor,
        binding: Binding { trigger: KEY, mods: mods::ANY, action: Action::from("\x1bOD"), mode: mode::APP_CURSOR, notmode: mode::NONE },
        triggers: true,
        mode: mode::APP_CURSOR | mode::APP_KEYPAD,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_fail_with_extra_mods,
        binding: Binding { trigger: KEY, mods: mods::SUPER, action: Action::from("arst"), mode: mode::NONE, notmode: mode::NONE },
        triggers: false,
        mode: mode::NONE,
        mods: mods::SUPER | mods::ALT
    }
}
