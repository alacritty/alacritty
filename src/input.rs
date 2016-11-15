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

use copypasta::{Clipboard, Load};
use glutin::{ElementState, VirtualKeyCode, MouseButton};
use glutin::{Mods, mods};

use config::Config;
use event_loop;
use term::mode::{self, TermMode};
use util::encode_char;

/// Processes input from glutin.
///
/// An escape sequence may be emitted in case specific keys or key combinations
/// are activated.
///
/// TODO also need terminal state when processing input
#[derive(Default)]
pub struct Processor {
    key_bindings: Vec<KeyBinding>,
    mouse_bindings: Vec<MouseBinding>,
}

/// Types that are notified of escape sequences from the input::Processor.
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
/// This is the shared component of MouseBinding and KeyBinding
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
    fn execute<N: Notify>(&self, notifier: &mut N) {
        self.binding.action.execute(notifier)
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
    fn execute<N: Notify>(&self, notifier: &mut N) {
        self.binding.action.execute(notifier)
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Write an escape sequence
    Esc(String),

    /// Paste contents of system clipboard
    Paste,

    /// Paste contents of selection buffer
    PasteSelection,
}

impl Action {
    #[inline]
    fn execute<N: Notify>(&self, notifier: &mut N) {
        match *self {
            Action::Esc(ref s) => notifier.notify(s.clone().into_bytes()),
            Action::Paste | Action::PasteSelection => {
                println!("paste request");
                let clip = Clipboard::new().expect("get clipboard");
                clip.load_selection()
                    .map(|contents| {
                        println!("got contents");
                        notifier.notify(contents.into_bytes())
                    })
                    .unwrap_or_else(|err| {
                        err_println!("Error getting clipboard contents: {}", err);
                    });

                println!("ok");
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

//   key               mods            escape      appkey appcursor crlf
//
// notes: appkey = DECPAM (application keypad mode); not enabled is "normal keypad"
//     appcursor = DECCKM (application cursor mode);
//          crlf = LNM    (Linefeed/new line); wtf is this

impl Processor {
    pub fn new(config: &Config) -> Processor {
        Processor {
            key_bindings: config.key_bindings().to_vec(),
            mouse_bindings: config.mouse_bindings().to_vec(),
        }
    }

    pub fn mouse_input<N: Notify>(
        &mut self,
        state: ElementState,
        button: MouseButton,
        notifier: &mut N,
        mode: TermMode
    ) {
        if let ElementState::Released = state {
            return;
        }

        Processor::process_mouse_bindings(
            &self.mouse_bindings[..],
            mode,
            notifier,
            mods::NONE,
            button
        );
    }

    pub fn process_key<N: Notify>(
        &mut self,
        state: ElementState,
        key: Option<VirtualKeyCode>,
        mods: Mods,
        notifier: &mut N,
        mode: TermMode,
        string: Option<String>,
    ) {
        if let Some(key) = key {
            // Ignore release events
            if state == ElementState::Released {
                return;
            }

            if Processor::process_key_bindings(&self.key_bindings[..], mode, notifier, mods, key) {
                return;
            }

            // Didn't process a binding; print the provided character
            if let Some(string) = string {
                notifier.notify(string.into_bytes());
            }
        }
    }

    /// Attempts to find a binding and execute its action
    ///
    /// The provided mode, mods, and key must match what is allowed by a binding
    /// for its action to be executed.
    ///
    /// Returns true if an action is executed.
    fn process_key_bindings<N>(
        bindings: &[KeyBinding],
        mode: TermMode,
        notifier: &mut N,
        mods: Mods,
        key: VirtualKeyCode
    ) -> bool
        where N: Notify
    {
        for binding in bindings {
            if binding.is_triggered_by(&mode, &mods, &key) {
                // binding was triggered; run the action
                binding.execute(notifier);
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
    fn process_mouse_bindings<N>(
        bindings: &[MouseBinding],
        mode: TermMode,
        notifier: &mut N,
        mods: Mods,
        button: MouseButton
    ) -> bool
        where N: Notify
    {
        for binding in bindings {
            if binding.is_triggered_by(&mode, &mods, &button) {
                // binding was triggered; run the action
                binding.execute(notifier);
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
