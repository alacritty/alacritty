use std::borrow::Cow;
use std::mem;

use winit::event::{ElementState, KeyEvent};
#[cfg(target_os = "macos")]
use winit::keyboard::ModifiersKeyState;
use winit::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};
#[cfg(target_os = "macos")]
use winit::platform::macos::OptionAsAlt;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::TermMode;
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

use crate::config::{Action, BindingKey, BindingMode};
use crate::event::TYPING_SEARCH_DELAY;
use crate::input::{ActionContext, Execute, Processor};
use crate::scheduler::{TimerId, Topic};

impl<T: EventListener, A: ActionContext<T>> Processor<T, A> {
    /// Process key input.
    pub fn key_input(&mut self, key: KeyEvent) {
        // IME input will be applied on commit and shouldn't trigger key bindings.
        if self.ctx.display().ime.preedit().is_some() {
            return;
        }

        let mode = *self.ctx.terminal().mode();
        let mods = self.ctx.modifiers().state();

        if key.state == ElementState::Released {
            self.key_release(key, mode, mods);
            return;
        }

        let text = key.text_with_all_modifiers().unwrap_or_default();

        // All key bindings are disabled while a hint is being selected.
        if self.ctx.display().hint_state.active() {
            for character in text.chars() {
                self.ctx.hint_input(character);
            }
            return;
        }

        // First key after inline search is captured.
        let inline_state = self.ctx.inline_search_state();
        if mem::take(&mut inline_state.char_pending) {
            if let Some(c) = text.chars().next() {
                inline_state.character = Some(c);

                // Immediately move to the captured character.
                self.ctx.inline_search_next();
            }

            // Ignore all other characters in `text`.
            return;
        }

        // Reset search delay when the user is still typing.
        self.reset_search_delay();

        // Key bindings suppress the character input.
        if self.process_key_bindings(&key) {
            return;
        }

        if self.ctx.search_active() {
            for character in text.chars() {
                self.ctx.search_input(character);
            }

            return;
        }

        // Vi mode on its own doesn't have any input, the search input was done before.
        if mode.contains(TermMode::VI) {
            return;
        }

        let build_key_sequence = Self::should_build_esc_key_sequence(&key, text, mode, mods);

        let bytes = if build_key_sequence {
            build_key_esc_sequence(key, mods, mode)
        } else {
            let mut bytes = Vec::with_capacity(text.len() + 1);
            if self.alt_send_esc() && text.len() == 1 {
                bytes.push(b'\x1b');
            }

            bytes.extend_from_slice(text.as_bytes());
            bytes
        };

        // Write only if we have something to write.
        if !bytes.is_empty() {
            self.ctx.on_terminal_input_start();
            self.ctx.write_to_pty(bytes);
        }
    }

    /// Check whether we should try to build escape sequence for the [`KeyEvent`].
    fn should_build_esc_key_sequence(
        key: &KeyEvent,
        text: &str,
        mode: TermMode,
        mods: ModifiersState,
    ) -> bool {
        if mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) {
            true
        } else if mode.contains(TermMode::DISAMBIGUATE_ESC_CODES) {
            let on_numpad = key.location == KeyLocation::Numpad;
            let is_escape = key.logical_key == Key::Named(NamedKey::Escape);
            is_escape || (!mods.is_empty() && mods != ModifiersState::SHIFT) || on_numpad
        } else {
            // `Delete` key always has text attached to it, but it's a named key, thus needs to be
            // excluded here as well.
            text.is_empty() || key.logical_key == Key::Named(NamedKey::Delete)
        }
    }

    /// Whether we should send `ESC` due to `Alt` being pressed.
    #[cfg(not(target_os = "macos"))]
    fn alt_send_esc(&mut self) -> bool {
        self.ctx.modifiers().state().alt_key()
    }

    #[cfg(target_os = "macos")]
    fn alt_send_esc(&mut self) -> bool {
        let option_as_alt = self.ctx.config().window.option_as_alt();
        self.ctx.modifiers().state().alt_key()
            && (option_as_alt == OptionAsAlt::Both
                || (option_as_alt == OptionAsAlt::OnlyLeft
                    && self.ctx.modifiers().lalt_state() == ModifiersKeyState::Pressed)
                || (option_as_alt == OptionAsAlt::OnlyRight
                    && self.ctx.modifiers().ralt_state() == ModifiersKeyState::Pressed))
    }

    /// Attempt to find a binding and execute its action.
    ///
    /// The provided mode, mods, and key must match what is allowed by a binding
    /// for its action to be executed.
    fn process_key_bindings(&mut self, key: &KeyEvent) -> bool {
        let mode = BindingMode::new(self.ctx.terminal().mode(), self.ctx.search_active());
        let mods = self.ctx.modifiers().state();

        // Don't suppress char if no bindings were triggered.
        let mut suppress_chars = None;

        for i in 0..self.ctx.config().key_bindings().len() {
            let binding = &self.ctx.config().key_bindings()[i];

            // We don't want the key without modifier, because it means something else most of
            // the time. However what we want is to manually lowercase the character to account
            // for both small and capital letters on regular characters at the same time.
            let logical_key = if let Key::Character(ch) = key.logical_key.as_ref() {
                Key::Character(ch.to_lowercase().into())
            } else {
                key.logical_key.clone()
            };

            let key = match (&binding.trigger, logical_key) {
                (BindingKey::Scancode(_), _) => BindingKey::Scancode(key.physical_key),
                (_, code) => BindingKey::Keycode { key: code, location: key.location.into() },
            };

            if binding.is_triggered_by(mode, mods, &key) {
                // Pass through the key if any of the bindings has the `ReceiveChar` action.
                *suppress_chars.get_or_insert(true) &= binding.action != Action::ReceiveChar;

                // Binding was triggered; run the action.
                binding.action.clone().execute(&mut self.ctx);
            }
        }

        suppress_chars.unwrap_or(false)
    }

    /// Handle key release.
    fn key_release(&mut self, key: KeyEvent, mode: TermMode, mods: ModifiersState) {
        if mode.contains(TermMode::REPORT_EVENT_TYPES)
            && !mode.contains(TermMode::VI)
            && !self.ctx.search_active()
            && !self.ctx.display().hint_state.active()
        {
            let bytes: Cow<'static, [u8]> = match key.logical_key.as_ref() {
                // NOTE echo the key back on release to follow kitty/foot behavior. When
                // KEYBOARD_REPORT_ALL_KEYS_AS_ESC is used, we build proper escapes for
                // the keys below.
                _ if mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) => {
                    build_key_esc_sequence(key, mods, mode).into()
                },
                // Winit uses different keys for `Backspace` so we expliictly specify the
                // values, instead of using what was passed to us from it.
                Key::Named(NamedKey::Tab) => [b'\t'].as_slice().into(),
                Key::Named(NamedKey::Enter) => [b'\r'].as_slice().into(),
                Key::Named(NamedKey::Backspace) => [b'\x7f'].as_slice().into(),
                Key::Named(NamedKey::Escape) => [b'\x1b'].as_slice().into(),
                _ => build_key_esc_sequence(key, mods, mode).into(),
            };

            self.ctx.write_to_pty(bytes);
        }
    }

    /// Reset search delay.
    fn reset_search_delay(&mut self) {
        if self.ctx.search_active() {
            let timer_id = TimerId::new(Topic::DelayedSearch, self.ctx.window().id());
            let scheduler = self.ctx.scheduler_mut();
            if let Some(timer) = scheduler.unschedule(timer_id) {
                scheduler.schedule(timer.event, TYPING_SEARCH_DELAY, false, timer.id);
            }
        }
    }
}

/// Build a key's keyboard escape sequence based on the given `key`, `mods`, and `mode`.
///
/// The key sequences for `APP_KEYPAD` and alike are handled inside the bindings.
#[inline(never)]
fn build_key_esc_sequence(key: KeyEvent, mods: ModifiersState, mode: TermMode) -> Vec<u8> {
    let modifiers = mods.into();

    let kitty_seq = mode.intersects(
        TermMode::REPORT_ALL_KEYS_AS_ESC
            | TermMode::DISAMBIGUATE_ESC_CODES
            | TermMode::REPORT_EVENT_TYPES,
    );

    let kitty_encode_all = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    // The `Press` is ignored here, since it's a default and no need to report it.
    let kitty_event_type = mode.contains(TermMode::REPORT_EVENT_TYPES)
        && (key.repeat || key.state == ElementState::Released);

    let context = KeyEscSequenceBuildingContext {
        mode,
        modifiers,
        kitty_seq,
        kitty_encode_all,
        kitty_event_type,
    };

    let sequence_base = context
        .prepare_numpad_base(&key)
        .or_else(|| context.prepare_named_base(&key))
        .or_else(|| context.prepare_control_and_mods(&key))
        .or_else(|| context.prepare_text_base(&key));

    let (payload, terminator) = match sequence_base {
        Some(KeyEscSequenceBase { payload, terminator }) => (payload, terminator),
        _ => return Vec::new(),
    };

    let mut payload = format!("\x1b[{}", payload);

    // Add modifiers information.
    if kitty_event_type
        || !modifiers.is_empty()
        || (mode.contains(TermMode::REPORT_ASSOCIATED_TEXT) && key.text.is_some())
    {
        payload.push_str(&format!(";{}", modifiers.esc_sequence_encoded()));
    }

    // Push event type.
    if kitty_event_type {
        payload.push(':');
        let event_type = match key.state {
            _ if key.repeat => '2',
            ElementState::Pressed => '1',
            ElementState::Released => '3',
        };
        payload.push(event_type);
    }

    // Associated text is not reported for the control/alt/logo key presses.
    if mode.contains(TermMode::REPORT_ASSOCIATED_TEXT)
        && key.state != ElementState::Released
        && (modifiers.is_empty() || modifiers == EscSequenceModifiers::SHIFT)
    {
        if let Some(text) = key.text {
            let mut codepoints = text.chars().map(u32::from);
            if let Some(codepoint) = codepoints.next() {
                payload.push_str(&format!(";{codepoint}"));
            }
            // Push the rest of the chars.
            for codepoint in codepoints {
                payload.push_str(&format!(":{codepoint}"));
            }
        }
    }

    payload.push(terminator);

    payload.into_bytes()
}

/// Helper to create esc sequence payloads from [`KeyEvent`].
pub struct KeyEscSequenceBuildingContext {
    mode: TermMode,
    /// The emitted sequence should follow the kitty keyboard protocol.
    kitty_seq: bool,
    /// Encode all the keys according to the protocol.
    kitty_encode_all: bool,
    /// Report event types.
    kitty_event_type: bool,
    modifiers: EscSequenceModifiers,
}

impl KeyEscSequenceBuildingContext {
    /// Prepare key with the text attached to it.
    fn prepare_text_base(&self, key: &KeyEvent) -> Option<KeyEscSequenceBase> {
        let character = match key.logical_key.as_ref() {
            Key::Character(character) => character,
            _ => return None,
        };

        // Character from winit is non-empty.
        if character.chars().count() == 1 {
            let character = character.chars().next().unwrap();
            let base_character = character.to_lowercase().next().unwrap();

            let codepoint = u32::from(character);
            let base_codepoint = u32::from(base_character);

            // NOTE: Base layouts are ignored, since winit doesn't expose this information
            // yet.
            let payload = if self.mode.contains(TermMode::REPORT_ALTERNATE_KEYS)
                && codepoint != base_codepoint
            {
                format!("{codepoint}:{base_codepoint}")
            } else {
                codepoint.to_string()
            };

            Some(KeyEscSequenceBase::new(payload.into(), 'u'))
        } else if self.kitty_encode_all
            && self.mode.contains(TermMode::REPORT_ASSOCIATED_TEXT)
            && key.text.is_some()
        {
            // When associated text is requested along the encode all and we have more than one
            // character, we should use codepoint '0' with 'u' suffix.
            Some(KeyEscSequenceBase::new("0".into(), 'u'))
        } else {
            None
        }
    }

    /// Prepare numpad key.
    ///
    /// `None` is returned when the key neither known or numpad.
    fn prepare_numpad_base(&self, key: &KeyEvent) -> Option<KeyEscSequenceBase> {
        if !self.kitty_seq || key.location != KeyLocation::Numpad {
            return None;
        }

        let base = match key.logical_key.as_ref() {
            Key::Character("0") => "57399",
            Key::Character("1") => "57400",
            Key::Character("2") => "57401",
            Key::Character("3") => "57402",
            Key::Character("4") => "57403",
            Key::Character("5") => "57404",
            Key::Character("6") => "57405",
            Key::Character("7") => "57406",
            Key::Character("8") => "57407",
            Key::Character("9") => "57408",
            Key::Character(".") => "57409",
            Key::Character("/") => "57410",
            Key::Character("*") => "57411",
            Key::Character("-") => "57412",
            Key::Character("+") => "57413",
            Key::Character("=") => "57415",
            Key::Named(named) => match named {
                NamedKey::Enter => "57414",
                NamedKey::ArrowLeft => "57417",
                NamedKey::ArrowRight => "57418",
                NamedKey::ArrowUp => "57419",
                NamedKey::ArrowDown => "57420",
                NamedKey::PageUp => "57421",
                NamedKey::PageDown => "57422",
                NamedKey::Home => "57423",
                NamedKey::End => "57424",
                NamedKey::Insert => "57425",
                NamedKey::Delete => "57426",
                // NOTE: we don't know where these keys actually are in winit input,
                // but once we know, the values are the following:
                // KP_SEPARATOR => "57415",
                // KP_BEGIN => "57427",
                _ => return None,
            },
            _ => return None,
        };

        Some(KeyEscSequenceBase::new(base.into(), 'u'))
    }

    /// Prepare [`NamedKey`] key.
    fn prepare_named_base(&self, key: &KeyEvent) -> Option<KeyEscSequenceBase> {
        let named = match key.logical_key {
            Key::Named(named) => named,
            _ => return None,
        };

        let omit_base = self.modifiers.is_empty() && !self.kitty_event_type;
        let (base, suffix) = match named {
            NamedKey::ArrowLeft => {
                let base = if omit_base { "" } else { "1" };
                (base, 'D')
            },
            NamedKey::ArrowRight => {
                let base = if omit_base { "" } else { "1" };
                (base, 'C')
            },
            NamedKey::ArrowUp => {
                let base = if omit_base { "" } else { "1" };
                (base, 'A')
            },
            NamedKey::ArrowDown => {
                let base = if omit_base { "" } else { "1" };
                (base, 'B')
            },
            NamedKey::Home => {
                let base = if omit_base { "" } else { "1" };
                (base, 'H')
            },
            NamedKey::End => {
                let base = if omit_base { "" } else { "1" };
                (base, 'F')
            },
            NamedKey::PageUp => ("5", '~'),
            NamedKey::PageDown => ("6", '~'),
            NamedKey::Insert => ("2", '~'),
            NamedKey::Delete => ("3", '~'),
            NamedKey::F1 => {
                let base = if self.kitty_seq && omit_base { "" } else { "1" };
                (base, 'P')
            },
            NamedKey::F2 => {
                let base = if self.kitty_seq && omit_base { "" } else { "1" };
                (base, 'Q')
            },
            NamedKey::F3 => {
                // F3 diverges from alacritty's terminfo for kitty protocol.
                if self.kitty_seq {
                    ("13", '~')
                } else {
                    ("1", 'R')
                }
            },
            NamedKey::F4 => {
                let base = if self.kitty_seq && omit_base { "" } else { "1" };
                (base, 'S')
            },
            NamedKey::F5 => ("15", '~'),
            NamedKey::F6 => ("17", '~'),
            NamedKey::F7 => ("18", '~'),
            NamedKey::F8 => ("19", '~'),
            NamedKey::F9 => ("20", '~'),
            NamedKey::F10 => ("21", '~'),
            NamedKey::F11 => ("23", '~'),
            NamedKey::F12 => ("24", '~'),
            NamedKey::F13 => ("57376", 'u'),
            NamedKey::F14 => ("57377", 'u'),
            NamedKey::F15 => ("57378", 'u'),
            NamedKey::F16 => ("57379", 'u'),
            NamedKey::F17 => ("57380", 'u'),
            NamedKey::F18 => ("57381", 'u'),
            NamedKey::F19 => ("57382", 'u'),
            NamedKey::F20 => ("57383", 'u'),
            NamedKey::F21 => ("57384", 'u'),
            NamedKey::F22 => ("57385", 'u'),
            NamedKey::F23 => ("57386", 'u'),
            NamedKey::F24 => ("57387", 'u'),
            NamedKey::F25 => ("57388", 'u'),
            NamedKey::F26 => ("57389", 'u'),
            NamedKey::F27 => ("57390", 'u'),
            NamedKey::F28 => ("57391", 'u'),
            NamedKey::F29 => ("57392", 'u'),
            NamedKey::F30 => ("57393", 'u'),
            NamedKey::F31 => ("57394", 'u'),
            NamedKey::F32 => ("57395", 'u'),
            NamedKey::F33 => ("57396", 'u'),
            NamedKey::F34 => ("57397", 'u'),
            NamedKey::F35 => ("57398", 'u'),
            NamedKey::ScrollLock => ("57359", 'u'),
            NamedKey::PrintScreen => ("57361", 'u'),
            NamedKey::Pause => ("57362", 'u'),
            NamedKey::ContextMenu => ("57363", 'u'),
            NamedKey::MediaPlay => ("57428", 'u'),
            NamedKey::MediaPause => ("57429", 'u'),
            NamedKey::MediaPlayPause => ("57430", 'u'),
            NamedKey::MediaStop => ("57432", 'u'),
            NamedKey::MediaFastForward => ("57433", 'u'),
            NamedKey::MediaRewind => ("57434", 'u'),
            NamedKey::MediaTrackNext => ("57435", 'u'),
            NamedKey::MediaTrackPrevious => ("57436", 'u'),
            NamedKey::MediaRecord => ("57437", 'u'),
            NamedKey::AudioVolumeDown => ("57438", 'u'),
            NamedKey::AudioVolumeUp => ("57439", 'u'),
            NamedKey::AudioVolumeMute => ("57440", 'u'),
            _ => return None,
        };

        Some(KeyEscSequenceBase::new(base.into(), suffix))
    }

    /// Prepare control characters (e.g. Enter) and modifiers.
    fn prepare_control_and_mods(&self, key: &KeyEvent) -> Option<KeyEscSequenceBase> {
        if !self.kitty_encode_all && !self.kitty_seq {
            return None;
        }

        let named = match key.logical_key {
            Key::Named(named) => named,
            _ => return None,
        };

        let base = match named {
            NamedKey::Tab => "9",
            NamedKey::Enter => "13",
            NamedKey::Escape => "27",
            NamedKey::Space => "32",
            NamedKey::Backspace => "127",
            _ => "",
        };

        if !self.kitty_encode_all && base.is_empty() {
            return None;
        }

        let left = key.location == KeyLocation::Left;
        #[rustfmt::skip]
        let base = match named {
            NamedKey::CapsLock => "57358",
            NamedKey::NumLock => "57360",
            NamedKey::Shift => if left { "57441" } else { "57447" },
            NamedKey::Control => if left { "57442" } else { "57448" },
            NamedKey::Alt => if left { "57443" } else { "57449" },
            NamedKey::Super => if left { "57444" } else { "57450" },
            NamedKey::Hyper => if left { "57445" } else { "57451" },
            NamedKey::Meta => if left { "57446" } else { "57452" },
            _ => base,
        };

        if base.is_empty() {
            None
        } else {
            Some(KeyEscSequenceBase::new(base.into(), 'u'))
        }
    }
}

pub struct KeyEscSequenceBase {
    /// The base of the payload.
    payload: Cow<'static, str>,
    /// The terminator used for escape, usually `u` or `~`.
    terminator: char,
}

impl KeyEscSequenceBase {
    fn new(payload: Cow<'static, str>, suffix: char) -> Self {
        Self { payload, terminator: suffix }
    }
}

bitflags::bitflags! {
    /// The modifiers encoding for escape sequence.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct EscSequenceModifiers : u8 {
        const SHIFT   = 0b0000_0001;
        const ALT     = 0b0000_0010;
        const CONTROL = 0b0000_0100;
        const SUPER   = 0b0000_1000;
        // NOTE: kitty protocol defines additional modifiers to what is present here, like
        // Capslock, but it's not a modifier as per winit.
    }
}

impl EscSequenceModifiers {
    /// Get the value which should be passed to escape sequence.
    pub fn esc_sequence_encoded(self) -> u8 {
        self.bits() + 1
    }
}

impl From<ModifiersState> for EscSequenceModifiers {
    fn from(mods: ModifiersState) -> Self {
        let mut modifiers = Self::empty();
        modifiers.set(Self::SHIFT, mods.shift_key());
        modifiers.set(Self::ALT, mods.alt_key());
        modifiers.set(Self::CONTROL, mods.control_key());
        modifiers.set(Self::SUPER, mods.super_key());
        modifiers
    }
}
