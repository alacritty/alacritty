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

use term::mode::{self, TermMode};
use event_loop;

/// Processes input from glutin.
///
/// An escape sequence may be emitted in case specific keys or key combinations
/// are activated.
///
/// TODO also need terminal state when processing input
#[derive(Default)]
pub struct Processor;

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

/// Describes a key combination that should emit a control sequence
///
/// The actual triggering key is omitted here since bindings are grouped by the trigger key.
#[derive(Debug)]
pub struct Binding {
    /// Modifier keys required to activate binding
    mods: Mods,
    /// String to send to pty if mods and mode match
    action: Action,
    /// Terminal mode required to activate binding
    mode: TermMode,
    /// excluded terminal modes where the binding won't be activated
    notmode: TermMode,
}

#[derive(Debug)]
pub enum Action {
    /// Write an escape sequence
    Esc(&'static str),

    /// Paste contents of system clipboard
    Paste,

    /// Send a char to pty
    Char(char)
}

/// Bindings for the LEFT key.
static LEFT_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::SHIFT,   action: Action::Esc("\x1b[1;2D"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::CONTROL, action: Action::Esc("\x1b[1;5D"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::ALT,     action: Action::Esc("\x1b[1;3D"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::ANY,     action: Action::Esc("\x1b[D"),    mode: mode::ANY,        notmode: mode::APP_CURSOR },
    Binding { mods: mods::ANY,     action: Action::Esc("\x1bOD"),    mode: mode::APP_CURSOR, notmode: mode::NONE },
];

/// Bindings for the RIGHT key
static RIGHT_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::SHIFT,   action: Action::Esc("\x1b[1;2C"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::CONTROL, action: Action::Esc("\x1b[1;5C"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::ALT,     action: Action::Esc("\x1b[1;3C"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::ANY,     action: Action::Esc("\x1b[C"),    mode: mode::ANY,        notmode: mode::APP_CURSOR },
    Binding { mods: mods::ANY,     action: Action::Esc("\x1bOC"),    mode: mode::APP_CURSOR, notmode: mode::NONE },
];

/// Bindings for the UP key
static UP_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::SHIFT,   action: Action::Esc("\x1b[1;2A"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::CONTROL, action: Action::Esc("\x1b[1;5A"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::ALT,     action: Action::Esc("\x1b[1;3A"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::ANY,     action: Action::Esc("\x1b[A"),    mode: mode::ANY,        notmode: mode::APP_CURSOR },
    Binding { mods: mods::ANY,     action: Action::Esc("\x1bOA"),    mode: mode::APP_CURSOR, notmode: mode::NONE },
];

/// Bindings for the DOWN key
static DOWN_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::SHIFT,   action: Action::Esc("\x1b[1;2B"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::CONTROL, action: Action::Esc("\x1b[1;5B"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::ALT,     action: Action::Esc("\x1b[1;3B"), mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: mods::ANY,     action: Action::Esc("\x1b[B"),    mode: mode::ANY,        notmode: mode::APP_CURSOR },
    Binding { mods: mods::ANY,     action: Action::Esc("\x1bOB"),    mode: mode::APP_CURSOR, notmode: mode::NONE },
];

/// Bindings for the F1 key
static F1_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1bOP"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F2 key
static F2_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1bOQ"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F3 key
static F3_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1bOR"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F4 key
static F4_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1bOS"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F5 key
static F5_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[15~"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F6 key
static F6_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[17~"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F7 key
static F7_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[18~"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F8 key
static F8_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[19~"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F9 key
static F9_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[20~"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F10 key
static F10_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[21~"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F11 key
static F11_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[23~"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the F11 key
static F12_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[24~"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the H key
///
/// Control-H sends 0x08 normally, but we capture that in ReceivedCharacter
/// since DEL and BACKSPACE are inverted. This binding is a work around to that
/// capture.
static H_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::CONTROL, action: Action::Esc("\x08"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the V Key
///
/// Cmd-V on macOS should trigger a paste
#[cfg(target_os="macos")]
static V_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::SUPER, action: Action::Paste, mode: mode::ANY, notmode: mode::NONE },
    Binding { mods: mods::NONE, action: Action::Char('v'), mode: mode::ANY, notmode: mode::NONE },
];

#[cfg(not(target_os="macos"))]
static V_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::NONE, action: Action::Char('v'), mode: mode::ANY, notmode: mode::NONE },
];

#[cfg(target_os="linux")]
static MOUSE_MIDDLE_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Paste, mode: mode::ANY, notmode: mode::NONE },
];

#[cfg(not(target_os="linux"))]
static MOUSE_MIDDLE_BINDINGS: &'static [Binding] = &[];

static MOUSE_LEFT_BINDINGS: &'static [Binding] = &[];
static MOUSE_RIGHT_BINDINGS: &'static [Binding] = &[];

/// Bindings for the Backspace key
static BACKSPACE_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x7f"), mode: mode::ANY, notmode: mode::NONE },
];

/// Bindings for the Delete key
static DELETE_BINDINGS: &'static [Binding] = &[
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[3~"), mode: mode::APP_KEYPAD, notmode: mode::NONE },
    Binding { mods: mods::ANY, action: Action::Esc("\x1b[P"), mode: mode::ANY, notmode: mode::APP_KEYPAD },
];

//   key               mods            escape      appkey appcursor crlf
//
// notes: appkey = DECPAM (application keypad mode); not enabled is "normal keypad"
//     appcursor = DECCKM (application cursor mode);
//          crlf = LNM    (Linefeed/new line); wtf is this

impl Processor {
    pub fn new() -> Processor {
        Default::default()
    }

    pub fn mouse_input<N: Notify>(
        &mut self,
        state: ElementState,
        input: MouseButton,
        notifier: &mut N,
        mode: TermMode
    ) {
        if let ElementState::Released = state {
            return;
        }

        let bindings = match input {
            MouseButton::Middle => MOUSE_MIDDLE_BINDINGS,
            MouseButton::Left => MOUSE_LEFT_BINDINGS,
            MouseButton::Right => MOUSE_RIGHT_BINDINGS,
            MouseButton::Other(_index) => return,
        };

        self.process_bindings(bindings, mode, notifier, mods::NONE);
    }

    pub fn process_key<N: Notify>(
        &mut self,
        state: ElementState,
        key: Option<VirtualKeyCode>,
        mods: Mods,
        notifier: &mut N,
        mode: TermMode
    ) {
        if let Some(key) = key {
            // Ignore release events
            if state == ElementState::Released {
                return;
            }

            let bindings = match key {
                // Arrows
                VirtualKeyCode::Left => LEFT_BINDINGS,
                VirtualKeyCode::Up => UP_BINDINGS,
                VirtualKeyCode::Down => DOWN_BINDINGS,
                VirtualKeyCode::Right => RIGHT_BINDINGS,
                // Function keys
                VirtualKeyCode::F1 => F1_BINDINGS,
                VirtualKeyCode::F2 => F2_BINDINGS,
                VirtualKeyCode::F3 => F3_BINDINGS,
                VirtualKeyCode::F4 => F4_BINDINGS,
                VirtualKeyCode::F5 => F5_BINDINGS,
                VirtualKeyCode::F6 => F6_BINDINGS,
                VirtualKeyCode::F7 => F7_BINDINGS,
                VirtualKeyCode::F8 => F8_BINDINGS,
                VirtualKeyCode::F9 => F9_BINDINGS,
                VirtualKeyCode::F10 => F10_BINDINGS,
                VirtualKeyCode::F11 => F11_BINDINGS,
                VirtualKeyCode::F12 => F12_BINDINGS,
                VirtualKeyCode::Back => BACKSPACE_BINDINGS,
                VirtualKeyCode::Delete => DELETE_BINDINGS,
                VirtualKeyCode::H => H_BINDINGS,
                VirtualKeyCode::V => V_BINDINGS,
                // Mode keys ignored now
                VirtualKeyCode::LAlt | VirtualKeyCode::RAlt | VirtualKeyCode::LShift |
                VirtualKeyCode::RShift | VirtualKeyCode::LControl | VirtualKeyCode::RControl |
                VirtualKeyCode::LWin | VirtualKeyCode::RWin => return,
                // All of the alphanumeric keys get passed through here as well, but there's no work
                // to be done for them.
                VirtualKeyCode::A | VirtualKeyCode::B | VirtualKeyCode::C | VirtualKeyCode::D |
                VirtualKeyCode::E | VirtualKeyCode::F | VirtualKeyCode::G |
                VirtualKeyCode::I | VirtualKeyCode::J | VirtualKeyCode::K | VirtualKeyCode::L |
                VirtualKeyCode::M | VirtualKeyCode::N | VirtualKeyCode::O | VirtualKeyCode::P |
                VirtualKeyCode::Q | VirtualKeyCode::R | VirtualKeyCode::S | VirtualKeyCode::T |
                VirtualKeyCode::U | VirtualKeyCode::W | VirtualKeyCode::X |
                VirtualKeyCode::Y | VirtualKeyCode::Z => return,
                VirtualKeyCode::Key1 | VirtualKeyCode::Key2 | VirtualKeyCode::Key3 |
                VirtualKeyCode::Key4 | VirtualKeyCode::Key5 | VirtualKeyCode::Key6 |
                VirtualKeyCode::Key7 | VirtualKeyCode::Key8 | VirtualKeyCode::Key9 |
                VirtualKeyCode::Key0 => return,
                // Log something by default
                _ => {
                    println!("Unhandled key: {:?}; state: {:?}; mods: {:?}",
                             key, state, mods);
                    return;
                },
            };

            self.process_bindings(bindings, mode, notifier, mods);
        }
    }

    fn process_bindings<N>(&self,
                           bindings: &[Binding],
                           mode: TermMode,
                           notifier: &mut N,
                           mods: Mods)
        where N: Notify
    {
        // Check each binding
        for binding in bindings {
            // TermMode positive
            if binding.mode.is_all() || mode.intersects(binding.mode) {
                // TermMode negative
                if binding.notmode.is_empty() || !mode.intersects(binding.notmode) {
                    // Modifier keys
                    if binding.mods.is_all() || mods == binding.mods {
                        // everything matches; run the binding action
                        match binding.action {
                            Action::Esc(s) => notifier.notify(s.as_bytes()),
                            Action::Paste => {
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
                            Action::Char(c) => {
                                // TODO encode_utf8 returns an iterator with "as_slice"
                                //      https://github.com/rust-lang/rust/issues/27784 has some
                                //      discussion about this API changing to `write_utf8` which
                                //      requires passing a &mut [u8] to be written into.
                                let encoded = c.encode_utf8();
                                notifier.notify(encoded.as_slice().to_vec());
                            }
                        }

                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use glutin::mods;

    use term::mode;

    use super::Action;
    use super::Processor;
    use super::Binding;

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

                let processor = Processor::new();
                let mut receiver = Receiver::default();

                processor.process_bindings(bindings, $mode, &mut receiver, $mods);
                assert_eq!(receiver.got, $expect);
            }
        }
    }

    test_process_binding! {
        name: process_binding_nomode_shiftmod_require_shift,
        binding: Binding { mods: mods::SHIFT, action: Action::Esc("\x1b[1;2D"), mode: mode::ANY, notmode: mode::NONE },
        expect: Some(String::from("\x1b[1;2D")),
        mode: mode::NONE,
        mods: mods::SHIFT
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_shift,
        binding: Binding { mods: mods::SHIFT, action: Action::Esc("\x1b[1;2D"), mode: mode::ANY, notmode: mode::NONE },
        expect: None,
        mode: mode::NONE,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_nomode_controlmod,
        binding: Binding { mods: mods::CONTROL, action: Action::Esc("\x1b[1;5D"), mode: mode::ANY, notmode: mode::NONE },
        expect: Some(String::from("\x1b[1;5D")),
        mode: mode::NONE,
        mods: mods::CONTROL
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_not_appcursor,
        binding: Binding { mods: mods::ANY, action: Action::Esc("\x1b[D"), mode: mode::ANY, notmode: mode::APP_CURSOR },
        expect: Some(String::from("\x1b[D")),
        mode: mode::NONE,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_appcursormode_nomod_require_appcursor,
        binding: Binding { mods: mods::ANY, action: Action::Esc("\x1bOD"), mode: mode::APP_CURSOR, notmode: mode::NONE },
        expect: Some(String::from("\x1bOD")),
        mode: mode::APP_CURSOR,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_appcursor,
        binding: Binding { mods: mods::ANY, action: Action::Esc("\x1bOD"), mode: mode::APP_CURSOR, notmode: mode::NONE },
        expect: None,
        mode: mode::NONE,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_appcursormode_appkeypadmode_nomod_require_appcursor,
        binding: Binding { mods: mods::ANY, action: Action::Esc("\x1bOD"), mode: mode::APP_CURSOR, notmode: mode::NONE },
        expect: Some(String::from("\x1bOD")),
        mode: mode::APP_CURSOR | mode::APP_KEYPAD,
        mods: mods::NONE
    }

    test_process_binding! {
        name: process_binding_fail_with_extra_mods,
        binding: Binding { mods: mods::SUPER, action: Action::Esc("arst"), mode: mode::ANY, notmode: mode::NONE },
        expect: None,
        mode: mode::NONE,
        mods: mods::SUPER | mods::ALT
    }
}
