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
use glutin::{ElementState, VirtualKeyCode};

use term::mode::{self, TermMode};

/// Modifier keys
///
/// Contains a bitflags for modifier keys which are now namespaced thanks to
/// this module wrapper.
mod modifier {
    use glutin::ElementState;

    bitflags! {
        /// Flags indicating active modifier keys
        pub flags Keys: u8 {
            /// Left shift
            const SHIFT_LEFT    = 0b00000001,
            /// Right shift
            const SHIFT_RIGHT   = 0b00000010,
            /// Left meta
            const META_LEFT     = 0b00000100,
            /// Right meta
            const META_RIGHT    = 0b00001000,
            /// Left control
            const CONTROL_LEFT  = 0b00010000,
            /// Right control
            const CONTROL_RIGHT = 0b00100000,
            /// Left alt
            const ALT_LEFT      = 0b01000000,
            /// Right alt
            const ALT_RIGHT     = 0b10000000,
            /// Any shift key
            const SHIFT         = SHIFT_LEFT.bits
                                | SHIFT_RIGHT.bits,
            /// Any control key
            const CONTROL       = CONTROL_LEFT.bits
                                | CONTROL_RIGHT.bits,
            /// Any alt key
            const ALT           = ALT_LEFT.bits
                                | ALT_RIGHT.bits,
            /// Any meta key
            const META          = META_LEFT.bits
                                | META_RIGHT.bits,
            /// Any mod
            const ANY           = 0b11111111,
            /// No mod
            const NONE          = 0b00000000,
        }
    }

    impl Default for Keys {
        fn default() -> Keys {
            Keys::empty()
        }
    }

    impl Keys {
        /// Take appropriate action given a modifier key and its state
        #[inline]
        pub fn update(&mut self, state: ElementState, key: Keys) {
            match state {
                ElementState::Pressed => self.insert(key),
                ElementState::Released => self.remove(key),
            }
        }
    }
}

/// Processes input from glutin.
///
/// An escape sequence may be emitted in case specific keys or key combinations
/// are activated.
///
/// TODO also need terminal state when processing input
#[derive(Default)]
pub struct Processor {
    /// Active modifier keys
    mods: modifier::Keys,
}

/// Types that are notified of escape sequences from the input::Processor.
pub trait Notify {
    /// Notify that an escape sequence should be written to the pty
    fn notify(&mut self, &str);
}

/// Describes a key combination that should emit a control sequence
///
/// The actual triggering key is omitted here since bindings are grouped by the trigger key.
#[derive(Debug)]
pub struct Binding {
    /// Modifier keys required to activate binding
    mods: modifier::Keys,
    /// String to send to pty if mods and mode match
    send: &'static str,
    /// Terminal mode required to activate binding
    mode: TermMode,
    /// excluded terminal modes where the binding won't be activated
    notmode: TermMode,
}

/// Bindings for the LEFT key.
static LEFT_BINDINGS: &'static [Binding] = &[
    Binding { mods: modifier::SHIFT,   send: "\x1b[1;2D", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::CONTROL, send: "\x1b[1;5D", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::ALT,     send: "\x1b[1;3D", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::ANY,     send: "\x1b[D",    mode: mode::ANY,        notmode: mode::APP_CURSOR },
    Binding { mods: modifier::ANY,     send: "\x1bOD",    mode: mode::APP_CURSOR, notmode: mode::NONE },
];

/// Bindings for the RIGHT key
static RIGHT_BINDINGS: &'static [Binding] = &[
    Binding { mods: modifier::SHIFT,   send: "\x1b[1;2C", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::CONTROL, send: "\x1b[1;5C", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::ALT,     send: "\x1b[1;3C", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::ANY,     send: "\x1b[C",    mode: mode::ANY,        notmode: mode::APP_CURSOR },
    Binding { mods: modifier::ANY,     send: "\x1bOC",    mode: mode::APP_CURSOR, notmode: mode::NONE },
];

/// Bindings for the UP key
static UP_BINDINGS: &'static [Binding] = &[
    Binding { mods: modifier::SHIFT,   send: "\x1b[1;2A", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::CONTROL, send: "\x1b[1;5A", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::ALT,     send: "\x1b[1;3A", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::ANY,     send: "\x1b[A",    mode: mode::ANY,        notmode: mode::APP_CURSOR },
    Binding { mods: modifier::ANY,     send: "\x1bOA",    mode: mode::APP_CURSOR, notmode: mode::NONE },
];

/// Bindings for the DOWN key
static DOWN_BINDINGS: &'static [Binding] = &[
    Binding { mods: modifier::SHIFT,   send: "\x1b[1;2B", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::CONTROL, send: "\x1b[1;5B", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::ALT,     send: "\x1b[1;3B", mode: mode::ANY,        notmode: mode::NONE },
    Binding { mods: modifier::ANY,     send: "\x1b[B",    mode: mode::ANY,        notmode: mode::APP_CURSOR },
    Binding { mods: modifier::ANY,     send: "\x1bOB",    mode: mode::APP_CURSOR, notmode: mode::NONE },
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

    pub fn process<N>(&mut self,
                      state: ElementState,
                      key: Option<VirtualKeyCode>,
                      notifier: &mut N,
                      mode: TermMode)
        where N: Notify
    {
        if let Some(key) = key {

            // Handle state updates
            match key {
                VirtualKeyCode::LAlt => self.mods.update(state, modifier::ALT_LEFT),
                VirtualKeyCode::RAlt => self.mods.update(state, modifier::ALT_RIGHT),
                VirtualKeyCode::LShift => self.mods.update(state, modifier::SHIFT_LEFT),
                VirtualKeyCode::RShift => self.mods.update(state, modifier::SHIFT_RIGHT),
                VirtualKeyCode::LControl => self.mods.update(state, modifier::CONTROL_LEFT),
                VirtualKeyCode::RControl => self.mods.update(state, modifier::CONTROL_RIGHT),
                VirtualKeyCode::LWin => self.mods.update(state, modifier::META_LEFT),
                VirtualKeyCode::RWin => self.mods.update(state, modifier::META_RIGHT),
                _ => ()
            }

            // Ignore release events
            if state == ElementState::Released {
                return;
            }

            let bindings = match key {
                VirtualKeyCode::Left => LEFT_BINDINGS,
                VirtualKeyCode::Up => UP_BINDINGS,
                VirtualKeyCode::Down => DOWN_BINDINGS,
                VirtualKeyCode::Right => RIGHT_BINDINGS,
                // Mode keys ignored now
                VirtualKeyCode::LAlt | VirtualKeyCode::RAlt | VirtualKeyCode::LShift |
                VirtualKeyCode::RShift | VirtualKeyCode::LControl | VirtualKeyCode::RControl |
                VirtualKeyCode::LWin | VirtualKeyCode::RWin => return,
                // All of the alphanumeric keys get passed through here as well, but there's no work
                // to be done for them.
                VirtualKeyCode::A | VirtualKeyCode::B | VirtualKeyCode::C | VirtualKeyCode::D |
                VirtualKeyCode::E | VirtualKeyCode::F | VirtualKeyCode::G | VirtualKeyCode::H |
                VirtualKeyCode::I | VirtualKeyCode::J | VirtualKeyCode::K | VirtualKeyCode::L |
                VirtualKeyCode::M | VirtualKeyCode::N | VirtualKeyCode::O | VirtualKeyCode::P |
                VirtualKeyCode::Q | VirtualKeyCode::R | VirtualKeyCode::S | VirtualKeyCode::T |
                VirtualKeyCode::U | VirtualKeyCode::V | VirtualKeyCode::W | VirtualKeyCode::X |
                VirtualKeyCode::Y | VirtualKeyCode::Z => return,
                VirtualKeyCode::Key1 | VirtualKeyCode::Key2 | VirtualKeyCode::Key3 |
                VirtualKeyCode::Key4 | VirtualKeyCode::Key5 | VirtualKeyCode::Key6 |
                VirtualKeyCode::Key7 | VirtualKeyCode::Key8 | VirtualKeyCode::Key9 |
                VirtualKeyCode::Key0 => return,
                // Log something by default
                _ => {
                    println!("Unhandled key: {:?}; state: {:?}; mods: {:?}",
                             key, state, self.mods);
                    return;
                },
            };

            self.process_bindings(bindings, mode, notifier);
        }
    }

    fn process_bindings<N>(&self, bindings: &[Binding], mode: TermMode, notifier: &mut N)
        where N: Notify
    {
        // Check each binding
        for binding in bindings {
            // TermMode positive
            if binding.mode.is_all() || mode.intersects(binding.mode) {
                // TermMode negative
                if binding.notmode.is_empty() || !mode.intersects(binding.notmode) {
                    // Modifier keys
                    if binding.mods.is_all() || self.mods.intersects(binding.mods) {
                        // everything matches
                        notifier.notify(binding.send);
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use term::mode::{self, TermMode};

    use super::Processor;
    use super::modifier;
    use super::Binding;

    /// Receiver that keeps a copy of any strings it is notified with
    #[derive(Default)]
    struct Receiver {
        pub got: Option<String>
    }

    impl super::Notify for Receiver {
        fn notify(&mut self, s: &str) {
            self.got = Some(String::from(s));
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

                let mut processor = Processor::new();
                processor.mods.insert($mods);
                let mut receiver = Receiver::default();

                processor.process_bindings(bindings, $mode, &mut receiver);
                assert_eq!(receiver.got, $expect);
            }
        }
    }

    test_process_binding! {
        name: process_binding_nomode_shiftmod_require_shift,
        binding: Binding { mods: modifier::SHIFT, send: "\x1b[1;2D", mode: mode::ANY, notmode: mode::NONE },
        expect: Some(String::from("\x1b[1;2D")),
        mode: mode::NONE,
        mods: modifier::SHIFT
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_shift,
        binding: Binding { mods: modifier::SHIFT, send: "\x1b[1;2D", mode: mode::ANY, notmode: mode::NONE },
        expect: None,
        mode: mode::NONE,
        mods: modifier::NONE
    }

    test_process_binding! {
        name: process_binding_nomode_controlmod,
        binding: Binding { mods: modifier::CONTROL, send: "\x1b[1;5D", mode: mode::ANY, notmode: mode::NONE },
        expect: Some(String::from("\x1b[1;5D")),
        mode: mode::NONE,
        mods: modifier::CONTROL
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_not_appcursor,
        binding: Binding { mods: modifier::ANY, send: "\x1b[D", mode: mode::ANY, notmode: mode::APP_CURSOR },
        expect: Some(String::from("\x1b[D")),
        mode: mode::NONE,
        mods: modifier::NONE
    }

    test_process_binding! {
        name: process_binding_appcursormode_nomod_require_appcursor,
        binding: Binding { mods: modifier::ANY, send: "\x1bOD", mode: mode::APP_CURSOR, notmode: mode::NONE },
        expect: Some(String::from("\x1bOD")),
        mode: mode::APP_CURSOR,
        mods: modifier::NONE
    }

    test_process_binding! {
        name: process_binding_nomode_nomod_require_appcursor,
        binding: Binding { mods: modifier::ANY, send: "\x1bOD", mode: mode::APP_CURSOR, notmode: mode::NONE },
        expect: None,
        mode: mode::NONE,
        mods: modifier::NONE
    }
}
