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
//!
//! TODO would be nice to generalize this so it could work with other windowing
//! APIs
//!
//! TODO handling xmodmap would be good
use std::borrow::Cow;

use copypasta::{Clipboard, Load, Store};
use glutin::{ElementState, VirtualKeyCode, MouseButton};
use glutin::{Mods, mods};
use glutin::{TouchPhase, MouseScrollDelta};

use config::Config;
use event_loop;
use index::{Line, Column, Side, Location};
use selection::Selection;
use term::mode::{self, TermMode};
use term::{self, Term};

/// Processes input from glutin.
///
/// An escape sequence may be emitted in case specific keys or key combinations
/// are activated.
///
/// TODO also need terminal state when processing input
pub struct Processor {
    key_bindings: Vec<KeyBinding>,
    mouse_bindings: Vec<MouseBinding>,
    mouse: Mouse,
    size_info: term::SizeInfo,
}

/// State of the mouse
pub struct Mouse {
    x: u32,
    y: u32,
    left_button_state: ElementState,
    scroll_px: i32,
    line: Line,
    column: Column,
    cell_side: Side
}

impl Default for Mouse {
    fn default() -> Mouse {
        Mouse {
            x: 0,
            y: 0,
            left_button_state: ElementState::Released,
            scroll_px: 0,
            line: Line(0),
            column: Column(0),
            cell_side: Side::Left,
        }
    }
}

/// Types that are notified of escape sequences from the `input::Processor`.
pub trait Notify {
    /// Notify that an escape sequence should be written to the pty
    ///
    /// TODO this needs to be able to error somehow
    fn notify<B: Into<Cow<'static, [u8]>>>(&mut self, B);
}

pub struct LoopNotifier(pub ::mio::channel::Sender<event_loop::Msg>);

impl Notify for LoopNotifier {
    fn notify<B>(&mut self, bytes: B)
        where B: Into<Cow<'static, [u8]>>
    {
        let bytes = bytes.into();
        match self.0.send(event_loop::Msg::Input(bytes)) {
            Ok(_) => (),
            Err(_) => panic!("expected send event loop msg"),
        }
    }
}

/// Describes a state and action to take in that state
///
/// This is the shared component of `MouseBinding` and `KeyBinding`
#[derive(Debug, Clone)]
pub struct Binding {
    /// Modifier keys required to activate binding
    pub mods: Mods,

    /// String to send to pty if mods and mode match
    pub action: Action,

    /// Terminal mode required to activate binding
    pub mode: TermMode,

    /// excluded terminal modes where the binding won't be activated
    pub notmode: TermMode,
}

#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub key: VirtualKeyCode,
    pub binding: Binding,
}

#[derive(Debug, Clone)]
pub struct MouseBinding {
    pub button: MouseButton,
    pub binding: Binding,
}

impl KeyBinding {
    #[inline]
    fn is_triggered_by(
        &self,
        mode: &TermMode,
        mods: &Mods,
        key: &VirtualKeyCode
    ) -> bool {
        // Check key first since bindings are stored in one big list. This is
        // the most likely item to fail so prioritizing it here allows more
        // checks to be short circuited.
        self.key == *key && self.binding.is_triggered_by(mode, mods)
    }

    #[inline]
    fn execute<'a, N: Notify>(&self, context: &mut ActionContext<'a, N>) {
        self.binding.action.execute(context)
    }
}

impl MouseBinding {
    #[inline]
    fn is_triggered_by(
        &self,
        mode: &TermMode,
        mods: &Mods,
        button: &MouseButton
    ) -> bool {
        // Check key first since bindings are stored in one big list. This is
        // the most likely item to fail so prioritizing it here allows more
        // checks to be short circuited.
        self.button == *button && self.binding.is_triggered_by(mode, mods)
    }

    #[inline]
    fn execute<'a, N: Notify>(&self, context: &mut ActionContext<'a, N>) {
        self.binding.action.execute(context)
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
            Action::Esc(ref s) => ctx.notifier.notify(s.clone().into_bytes()),
            Action::Copy => {
                // so... need access to terminal state. and the selection.
                unimplemented!();
            },
            Action::Paste | Action::PasteSelection => {
                let clip = Clipboard::new().expect("get clipboard");
                clip.load_selection()
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
                        err_println!("Error getting clipboard contents: {}", err);
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

impl Binding {
    /// Check if this binding is triggered by the current terminal mode,
    /// modifier keys, and key pressed.
    #[inline]
    pub fn is_triggered_by(
        &self,
        mode: &TermMode,
        mods: &Mods,
    ) -> bool {
        self.mode_matches(mode) &&
            self.not_mode_matches(mode) &&
            self.mods_match(mods)
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

pub struct ActionContext<'a, N: 'a> {
    pub notifier: &'a mut N,
    pub terminal: &'a Term,
    pub selection: &'a mut Selection
}

impl Processor {
    pub fn resize(&mut self, size_info: &term::SizeInfo) {
        self.size_info = size_info.to_owned();
    }

    pub fn new(config: &Config, size_info: &term::SizeInfo) -> Processor {
        Processor {
            key_bindings: config.key_bindings().to_vec(),
            mouse_bindings: config.mouse_bindings().to_vec(),
            mouse: Mouse::default(),
            size_info: size_info.to_owned(),
        }
    }

    #[inline]
    pub fn mouse_moved(&mut self, selection: &mut Selection, mode: TermMode, x: u32, y: u32) {
        // Record mouse position within window. Pixel coordinates are *not*
        // translated to grid coordinates here since grid coordinates are rarely
        // needed and the mouse position updates frequently.
        self.mouse.x = x;
        self.mouse.y = y;

        if let Some((line, column)) = self.size_info.pixels_to_coords(x as usize, y as usize) {
            self.mouse.line = line;
            self.mouse.column = column;

            let cell_x = x as usize % self.size_info.cell_width as usize;
            let half_cell_width = (self.size_info.cell_width / 2.0) as usize;

            self.mouse.cell_side = if cell_x > half_cell_width {
                Side::Right
            } else {
                Side::Left
            };

            if self.mouse.left_button_state == ElementState::Pressed &&
                !mode.contains(mode::MOUSE_REPORT_CLICK)
            {
                selection.update(Location {
                    line: line,
                    col: column
                }, self.mouse.cell_side);
            }
        }
    }

    pub fn mouse_report<'a, N: Notify>(
        &mut self,
        button: u8,
        context: &mut ActionContext<'a, N>
    ) {
        let (line, column) = (self.mouse.line, self.mouse.column);

        if line < Line(223) && column < Column(223) {
            let msg = vec![
                '\x1b' as u8,
                '[' as u8,
                'M' as u8,
                32 + button,
                32 + 1 + column.0 as u8,
                32 + 1 + line.0 as u8,
            ];

            context.notifier.notify(msg);
        }
    }

    pub fn on_mouse_press<'a, N: Notify>(
        &mut self,
        context: &mut ActionContext<'a, N>
    ) {
        if context.terminal.mode().contains(mode::MOUSE_REPORT_CLICK) {
            self.mouse_report(0, context);
            return;
        }

        context.selection.clear();
    }

    pub fn on_mouse_release<'a, N: Notify>(&mut self, context: &mut ActionContext<'a, N>) {
        if context.terminal.mode().contains(mode::MOUSE_REPORT_CLICK) {
            self.mouse_report(3, context);
            return;
        }
    }

    pub fn on_mouse_wheel<'a, N: Notify>(
        &mut self,
        context: &mut ActionContext<'a, N>,
        delta: MouseScrollDelta,
        phase: TouchPhase,
    ) {
        match delta {
            MouseScrollDelta::LineDelta(_columns, lines) => {
                let code = if lines > 0.0 {
                    64
                } else {
                    65
                };

                for _ in 0..(lines.abs() as usize) {
                    self.mouse_report(code, context);
                }
            },
            MouseScrollDelta::PixelDelta(_x, y) => {
                match phase {
                    TouchPhase::Started => {
                        // Reset offset to zero
                        self.mouse.scroll_px = 0;
                    },
                    TouchPhase::Moved => {
                        self.mouse.scroll_px += y as i32;
                        let height = self.size_info.cell_height as i32;

                        while self.mouse.scroll_px.abs() >= height {
                            let button = if self.mouse.scroll_px > 0 {
                                self.mouse.scroll_px -= height;
                                64
                            } else {
                                self.mouse.scroll_px += height;
                                65
                            };

                            self.mouse_report(button, context);
                        }
                    },
                    _ => (),
                }
            }
        }
    }

    pub fn mouse_input<'a, N: Notify>(
        &mut self,
        context: &mut ActionContext<'a, N>,
        state: ElementState,
        button: MouseButton,
    ) {
        if let MouseButton::Left = button {
            // TODO handle state changes
            if self.mouse.left_button_state != state {
                self.mouse.left_button_state = state;
                match state {
                    ElementState::Pressed => {
                        self.on_mouse_press(context);
                    },
                    ElementState::Released => {
                        self.on_mouse_release(context);
                    }
                }
            }
        }

        if let ElementState::Released = state {
            return;
        }

        Processor::process_mouse_bindings(
            context,
            &self.mouse_bindings[..],
            mods::NONE,
            button
        );
    }

    pub fn process_key<'a, N: Notify>(
        &mut self,
        context: &mut ActionContext<'a, N>,
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

            if Processor::process_key_bindings(context, &self.key_bindings[..], mods, key) {
                return;
            }

            // Didn't process a binding; print the provided character
            if let Some(string) = string {
                context.notifier.notify(string.into_bytes());
            }
        }
    }

    /// Attempts to find a binding and execute its action
    ///
    /// The provided mode, mods, and key must match what is allowed by a binding
    /// for its action to be executed.
    ///
    /// Returns true if an action is executed.
    fn process_key_bindings<'a, N: Notify>(
        context: &mut ActionContext<'a, N>,
        bindings: &[KeyBinding],
        mods: Mods,
        key: VirtualKeyCode
    ) -> bool {
        for binding in bindings {
            if binding.is_triggered_by(context.terminal.mode(), &mods, &key) {
                // binding was triggered; run the action
                binding.execute(context);
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
    fn process_mouse_bindings<'a, N: Notify>(
        context: &mut ActionContext<'a, N>,
        bindings: &[MouseBinding],
        mods: Mods,
        button: MouseButton
    ) -> bool {
        for binding in bindings {
            if binding.is_triggered_by(context.terminal.mode(), &mods, &button) {
                // binding was triggered; run the action
                binding.execute(context);
                return true;
            }
        }

        false
    }

    pub fn update_config(&mut self, config: &Config) {
        self.key_bindings = config.key_bindings().to_vec();
        self.mouse_bindings = config.mouse_bindings().to_vec();
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use glutin::{mods, VirtualKeyCode};

    use term::mode;

    use super::{Action, Processor, Binding, KeyBinding};

    /// Receiver that keeps a copy of any strings it is notified with
    #[derive(Default)]
    struct Receiver {
        pub got: Option<String>
    }

    impl super::Notify for Receiver {
        fn notify<B: Into<Cow<'static, [u8]>>>(&mut self, item: B) {
            self.got = Some(String::from_utf8(item.into().to_vec()).unwrap());
        }
    }

    const KEY: VirtualKeyCode = VirtualKeyCode::Key0;

    macro_rules! test_process_binding {
        {
            name: $name:ident,
            binding: $binding:expr,
            expect: $expect:expr,
            mode: $mode:expr,
            mods: $mods:expr
        } => {
            #[test]
            fn $name() {
                let bindings = &[$binding];

                let mut receiver = Receiver::default();

                Processor::process_key_bindings(
                    bindings, $mode, &mut receiver, $mods, KEY
                );
                assert_eq!(receiver.got, $expect);
            }
        }
    }

    test_process_binding! {
        name: process_binding_nomode_shiftmod_require_shift,
        binding: KeyBinding { key: KEY, binding: Binding { mods: mods::SHIFT, action: Action::from("\x1b[1;2D"), mode: mode::NONE, notmode: mode::NONE }},
        expect: Some(String::from("\x1b[1;2D")),
        mode: mode::NONE,
        mods: mods::SHIFT
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_shift,
        binding: KeyBinding { key: KEY, binding: Binding { mods: mods::SHIFT, action: Action::from("\x1b[1;2D"), mode: mode::NONE, notmode: mode::NONE }},
        expect: None,
        mode: mode::NONE,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_nomode_controlmod,
        binding: KeyBinding { key: KEY, binding: Binding { mods: mods::CONTROL, action: Action::from("\x1b[1;5D"), mode: mode::NONE, notmode: mode::NONE }},
        expect: Some(String::from("\x1b[1;5D")),
        mode: mode::NONE,
        mods: mods::CONTROL
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_not_appcursor,
        binding: KeyBinding { key: KEY, binding: Binding { mods: mods::ANY, action: Action::from("\x1b[D"), mode: mode::NONE, notmode: mode::APP_CURSOR }},
        expect: Some(String::from("\x1b[D")),
        mode: mode::NONE,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_appcursormode_nomod_require_appcursor,
        binding: KeyBinding { key: KEY, binding: Binding { mods: mods::ANY, action: Action::from("\x1bOD"), mode: mode::APP_CURSOR, notmode: mode::NONE }},
        expect: Some(String::from("\x1bOD")),
        mode: mode::APP_CURSOR,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_appcursor,
        binding: KeyBinding { key: KEY, binding: Binding { mods: mods::ANY, action: Action::from("\x1bOD"), mode: mode::APP_CURSOR, notmode: mode::NONE }},
        expect: None,
        mode: mode::NONE,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_appcursormode_appkeypadmode_nomod_require_appcursor,
        binding: KeyBinding { key: KEY, binding: Binding { mods: mods::ANY, action: Action::from("\x1bOD"), mode: mode::APP_CURSOR, notmode: mode::NONE }},
        expect: Some(String::from("\x1bOD")),
        mode: mode::APP_CURSOR | mode::APP_KEYPAD,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_fail_with_extra_mods,
        binding: KeyBinding { key: KEY, binding: Binding { mods: mods::SUPER, action: Action::from("arst"), mode: mode::NONE, notmode: mode::NONE }},
        expect: None,
        mode: mode::NONE,
        mods: mods::SUPER | mods::ALT
    }
}
