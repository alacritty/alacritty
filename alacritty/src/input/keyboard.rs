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

use KeyEscSequenceTerminator::{Kitty, Tinfo};

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
        && (modifiers.is_empty() || modifiers == KeyEscSequenceModifiers::SHIFT)
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

    payload.push(terminator.esc_sequence_encoded());

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
    modifiers: KeyEscSequenceModifiers,
}

impl KeyEscSequenceBuildingContext {
    /// Prepare key with the text attached to it.
    fn prepare_text_base(&self, key: &KeyEvent) -> Option<KeyEscSequenceBase> {
        let character = match key.logical_key.as_ref() {
            Key::Character(character) => character,
            _ => return None,
        };

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

            Some(KeyEscSequenceBase::new(payload.into(), Kitty))
        } else if self.kitty_encode_all
            && self.mode.contains(TermMode::REPORT_ASSOCIATED_TEXT)
            && key.text.is_some()
        {
            // Fallback when need to report text, but we don't have any key associated with this
            // text.
            Some(KeyEscSequenceBase::new("0".into(), Kitty))
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
                _ => return None,
            },
            _ => return None,
        };

        Some(KeyEscSequenceBase::new(base.into(), Kitty))
    }

    /// Prepare [`NamedKey`] key.
    fn prepare_named_base(&self, key: &KeyEvent) -> Option<KeyEscSequenceBase> {
        let named = match key.logical_key {
            Key::Named(named) => named,
            _ => return None,
        };

        // Omit payload when the base is 1, since it's a common thing to do and some clients break
        // if don't do so.
        let one_based = if self.modifiers.is_empty() && !self.kitty_event_type { "" } else { "1" };
        let (base, terminator) = match named {
            NamedKey::PageUp => ("5", Tinfo('~')),
            NamedKey::PageDown => ("6", Tinfo('~')),
            NamedKey::Insert => ("2", Tinfo('~')),
            NamedKey::Delete => ("3", Tinfo('~')),
            NamedKey::Home => (one_based, Tinfo('H')),
            NamedKey::End => (one_based, Tinfo('F')),
            NamedKey::ArrowLeft => (one_based, Tinfo('D')),
            NamedKey::ArrowRight => (one_based, Tinfo('C')),
            NamedKey::ArrowUp => (one_based, Tinfo('A')),
            NamedKey::ArrowDown => (one_based, Tinfo('B')),
            NamedKey::F1 => (one_based, Tinfo('P')),
            NamedKey::F2 => (one_based, Tinfo('Q')),
            NamedKey::F3 => {
                // F3 in kitty protocol diverges from alacritty's terminfo.
                if self.kitty_seq {
                    ("13", Tinfo('~'))
                } else {
                    (one_based, Tinfo('R'))
                }
            },
            NamedKey::F4 => (one_based, Tinfo('S')),
            NamedKey::F5 => ("15", Tinfo('~')),
            NamedKey::F6 => ("17", Tinfo('~')),
            NamedKey::F7 => ("18", Tinfo('~')),
            NamedKey::F8 => ("19", Tinfo('~')),
            NamedKey::F9 => ("20", Tinfo('~')),
            NamedKey::F10 => ("21", Tinfo('~')),
            NamedKey::F11 => ("23", Tinfo('~')),
            NamedKey::F12 => ("24", Tinfo('~')),
            NamedKey::F13 => ("57376", Kitty),
            NamedKey::F14 => ("57377", Kitty),
            NamedKey::F15 => ("57378", Kitty),
            NamedKey::F16 => ("57379", Kitty),
            NamedKey::F17 => ("57380", Kitty),
            NamedKey::F18 => ("57381", Kitty),
            NamedKey::F19 => ("57382", Kitty),
            NamedKey::F20 => ("57383", Kitty),
            NamedKey::F21 => ("57384", Kitty),
            NamedKey::F22 => ("57385", Kitty),
            NamedKey::F23 => ("57386", Kitty),
            NamedKey::F24 => ("57387", Kitty),
            NamedKey::F25 => ("57388", Kitty),
            NamedKey::F26 => ("57389", Kitty),
            NamedKey::F27 => ("57390", Kitty),
            NamedKey::F28 => ("57391", Kitty),
            NamedKey::F29 => ("57392", Kitty),
            NamedKey::F30 => ("57393", Kitty),
            NamedKey::F31 => ("57394", Kitty),
            NamedKey::F32 => ("57395", Kitty),
            NamedKey::F33 => ("57396", Kitty),
            NamedKey::F34 => ("57397", Kitty),
            NamedKey::F35 => ("57398", Kitty),
            NamedKey::ScrollLock => ("57359", Kitty),
            NamedKey::PrintScreen => ("57361", Kitty),
            NamedKey::Pause => ("57362", Kitty),
            NamedKey::ContextMenu => ("57363", Kitty),
            NamedKey::MediaPlay => ("57428", Kitty),
            NamedKey::MediaPause => ("57429", Kitty),
            NamedKey::MediaPlayPause => ("57430", Kitty),
            NamedKey::MediaStop => ("57432", Kitty),
            NamedKey::MediaFastForward => ("57433", Kitty),
            NamedKey::MediaRewind => ("57434", Kitty),
            NamedKey::MediaTrackNext => ("57435", Kitty),
            NamedKey::MediaTrackPrevious => ("57436", Kitty),
            NamedKey::MediaRecord => ("57437", Kitty),
            NamedKey::AudioVolumeDown => ("57438", Kitty),
            NamedKey::AudioVolumeUp => ("57439", Kitty),
            NamedKey::AudioVolumeMute => ("57440", Kitty),
            _ => return None,
        };

        Some(KeyEscSequenceBase::new(base.into(), terminator))
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

        let base = match (named, key.location) {
            (NamedKey::Shift, KeyLocation::Left) => "57441",
            (NamedKey::Control, KeyLocation::Left) => "57442",
            (NamedKey::Alt, KeyLocation::Left) => "57443",
            (NamedKey::Super, KeyLocation::Left) => "57444",
            (NamedKey::Hyper, KeyLocation::Left) => "57445",
            (NamedKey::Meta, KeyLocation::Left) => "57446",
            (NamedKey::Shift, _) => "57447",
            (NamedKey::Control, _) => "57448",
            (NamedKey::Alt, _) => "57449",
            (NamedKey::Super, _) => "57450",
            (NamedKey::Hyper, _) => "57451",
            (NamedKey::Meta, _) => "57452",
            (NamedKey::CapsLock, _) => "57358",
            (NamedKey::NumLock, _) => "57360",
            _ => base,
        };

        if base.is_empty() {
            None
        } else {
            Some(KeyEscSequenceBase::new(base.into(), Kitty))
        }
    }
}

pub struct KeyEscSequenceBase {
    /// The base of the payload, which is the `number` and optionally an alt base from the kitty
    /// spec.
    payload: Cow<'static, str>,
    terminator: KeyEscSequenceTerminator,
}

impl KeyEscSequenceBase {
    fn new(payload: Cow<'static, str>, terminator: KeyEscSequenceTerminator) -> Self {
        Self { payload, terminator }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum KeyEscSequenceTerminator {
    /// The terminator is based on terminfo.
    Tinfo(char),
    /// The terminator is for kitty escape sequence.
    Kitty,
}

impl KeyEscSequenceTerminator {
    fn esc_sequence_encoded(self) -> char {
        match self {
            KeyEscSequenceTerminator::Tinfo(char) => char,
            KeyEscSequenceTerminator::Kitty => 'u',
        }
    }
}

bitflags::bitflags! {
    /// The modifiers encoding for escape sequence.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct KeyEscSequenceModifiers : u8 {
        const SHIFT   = 0b0000_0001;
        const ALT     = 0b0000_0010;
        const CONTROL = 0b0000_0100;
        const SUPER   = 0b0000_1000;
        // NOTE: kitty protocol defines additional modifiers to what is present here, like
        // Capslock, but it's not a modifier as per winit.
    }
}

impl KeyEscSequenceModifiers {
    /// Get the value which should be passed to escape sequence.
    pub fn esc_sequence_encoded(self) -> u8 {
        self.bits() + 1
    }
}

impl From<ModifiersState> for KeyEscSequenceModifiers {
    fn from(mods: ModifiersState) -> Self {
        let mut modifiers = Self::empty();
        modifiers.set(Self::SHIFT, mods.shift_key());
        modifiers.set(Self::ALT, mods.alt_key());
        modifiers.set(Self::CONTROL, mods.control_key());
        modifiers.set(Self::SUPER, mods.super_key());
        modifiers
    }
}
