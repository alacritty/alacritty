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

        // Mask `Alt` modifier from input when we won't send esc.
        let mods = if self.alt_send_esc(&key, text) { mods } else { mods & !ModifiersState::ALT };

        let build_key_sequence = Self::should_build_sequence(&key, text, mode, mods);

        let bytes = if build_key_sequence {
            build_sequence(key, mods, mode)
        } else {
            let mut bytes = Vec::with_capacity(text.len() + 1);
            if mods.alt_key() {
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

    fn alt_send_esc(&mut self, key: &KeyEvent, text: &str) -> bool {
        #[cfg(not(target_os = "macos"))]
        let alt_send_esc = self.ctx.modifiers().state().alt_key();

        #[cfg(target_os = "macos")]
        let alt_send_esc = {
            let option_as_alt = self.ctx.config().window.option_as_alt();
            self.ctx.modifiers().state().alt_key()
                && (option_as_alt == OptionAsAlt::Both
                    || (option_as_alt == OptionAsAlt::OnlyLeft
                        && self.ctx.modifiers().lalt_state() == ModifiersKeyState::Pressed)
                    || (option_as_alt == OptionAsAlt::OnlyRight
                        && self.ctx.modifiers().ralt_state() == ModifiersKeyState::Pressed))
        };

        match key.logical_key {
            Key::Named(named) => {
                if named.to_text().is_some() {
                    alt_send_esc
                } else {
                    // Treat `Alt` as modifier for named keys without text, like ArrowUp.
                    self.ctx.modifiers().state().alt_key()
                }
            },
            _ => text.len() == 1 && alt_send_esc,
        }
    }

    /// Check whether we should try to build escape sequence for the [`KeyEvent`].
    fn should_build_sequence(
        key: &KeyEvent,
        text: &str,
        mode: TermMode,
        mods: ModifiersState,
    ) -> bool {
        if mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) {
            return true;
        }

        let disambiguate = mode.contains(TermMode::DISAMBIGUATE_ESC_CODES)
            && (key.logical_key == Key::Named(NamedKey::Escape)
                || (!mods.is_empty() && mods != ModifiersState::SHIFT)
                || key.location == KeyLocation::Numpad);

        match key.logical_key {
            _ if disambiguate => true,
            // Exclude all the named keys unless they have textual representation.
            Key::Named(named) => named.to_text().is_none(),
            _ => text.is_empty(),
        }
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

        // We don't want the key without modifier, because it means something else most of
        // the time. However what we want is to manually lowercase the character to account
        // for both small and capital letters on regular characters at the same time.
        let logical_key = if let Key::Character(ch) = key.logical_key.as_ref() {
            // Match `Alt` bindings without `Alt` being applied, otherwise they use the
            // composed chars, which are not intuitive to bind.
            //
            // On Windows, the `Ctrl + Alt` mangles `logical_key` to unidentified values, thus
            // preventing them from being used in bindings
            //
            // For more see https://github.com/rust-windowing/winit/issues/2945.
            if (cfg!(target_os = "macos") || (cfg!(windows) && mods.control_key()))
                && mods.alt_key()
            {
                key.key_without_modifiers()
            } else {
                Key::Character(ch.to_lowercase().into())
            }
        } else {
            key.logical_key.clone()
        };

        for i in 0..self.ctx.config().key_bindings().len() {
            let binding = &self.ctx.config().key_bindings()[i];

            let key = match (&binding.trigger, &logical_key) {
                (BindingKey::Scancode(_), _) => BindingKey::Scancode(key.physical_key),
                (_, code) => {
                    BindingKey::Keycode { key: code.clone(), location: key.location.into() }
                },
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
        if !mode.contains(TermMode::REPORT_EVENT_TYPES)
            || mode.contains(TermMode::VI)
            || self.ctx.search_active()
            || self.ctx.display().hint_state.active()
        {
            return;
        }

        // Mask `Alt` modifier from input when we won't send esc.
        let text = key.text_with_all_modifiers().unwrap_or_default();
        let mods = if self.alt_send_esc(&key, text) { mods } else { mods & !ModifiersState::ALT };

        let bytes: Cow<'static, [u8]> = match key.logical_key.as_ref() {
            // NOTE: Echo the key back on release to follow kitty/foot behavior. When
            // KEYBOARD_REPORT_ALL_KEYS_AS_ESC is used, we build proper escapes for
            // the keys below.
            _ if mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) => {
                build_sequence(key, mods, mode).into()
            },
            // Winit uses different keys for `Backspace` so we expliictly specify the
            // values, instead of using what was passed to us from it.
            Key::Named(NamedKey::Tab) => [b'\t'].as_slice().into(),
            Key::Named(NamedKey::Enter) => [b'\r'].as_slice().into(),
            Key::Named(NamedKey::Backspace) => [b'\x7f'].as_slice().into(),
            Key::Named(NamedKey::Escape) => [b'\x1b'].as_slice().into(),
            _ => build_sequence(key, mods, mode).into(),
        };

        self.ctx.write_to_pty(bytes);
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
fn build_sequence(key: KeyEvent, mods: ModifiersState, mode: TermMode) -> Vec<u8> {
    let mut modifiers = mods.into();

    let kitty_seq = mode.intersects(
        TermMode::REPORT_ALL_KEYS_AS_ESC
            | TermMode::DISAMBIGUATE_ESC_CODES
            | TermMode::REPORT_EVENT_TYPES,
    );

    let kitty_encode_all = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    // The default parameter is 1, so we can omit it.
    let kitty_event_type = mode.contains(TermMode::REPORT_EVENT_TYPES)
        && (key.repeat || key.state == ElementState::Released);

    let context =
        SequenceBuilder { mode, modifiers, kitty_seq, kitty_encode_all, kitty_event_type };

    let sequence_base = context
        .try_build_numpad(&key)
        .or_else(|| context.try_build_named(&key))
        .or_else(|| context.try_build_control_char_or_mod(&key, &mut modifiers))
        .or_else(|| context.try_build_textual(&key));

    let (payload, terminator) = match sequence_base {
        Some(SequenceBase { payload, terminator }) => (payload, terminator),
        _ => return Vec::new(),
    };

    let mut payload = format!("\x1b[{}", payload);

    // Add modifiers information.
    if kitty_event_type
        || !modifiers.is_empty()
        || (mode.contains(TermMode::REPORT_ASSOCIATED_TEXT) && key.text.is_some())
    {
        payload.push_str(&format!(";{}", modifiers.encode_esc_sequence()));
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

    // Associated text is not reported when the control/alt/logo is pressesed.
    if mode.contains(TermMode::REPORT_ASSOCIATED_TEXT)
        && key.state != ElementState::Released
        && (modifiers.is_empty() || modifiers == SequenceModifiers::SHIFT)
    {
        if let Some(text) = key.text {
            let mut codepoints = text.chars().map(u32::from);
            if let Some(codepoint) = codepoints.next() {
                payload.push_str(&format!(";{codepoint}"));
            }
            for codepoint in codepoints {
                payload.push_str(&format!(":{codepoint}"));
            }
        }
    }

    payload.push(terminator.encode_esc_sequence());

    payload.into_bytes()
}

/// Helper to build escape sequence payloads from [`KeyEvent`].
pub struct SequenceBuilder {
    mode: TermMode,
    /// The emitted sequence should follow the kitty keyboard protocol.
    kitty_seq: bool,
    /// Encode all the keys according to the protocol.
    kitty_encode_all: bool,
    /// Report event types.
    kitty_event_type: bool,
    modifiers: SequenceModifiers,
}

impl SequenceBuilder {
    /// Try building sequence from the event's emitting text.
    fn try_build_textual(&self, key: &KeyEvent) -> Option<SequenceBase> {
        let character = match key.logical_key.as_ref() {
            Key::Character(character) => character,
            _ => return None,
        };

        if character.chars().count() == 1 {
            let character = character.chars().next().unwrap();
            let base_character = character.to_lowercase().next().unwrap();

            let alternate_key_code = u32::from(character);
            let mut unicode_key_code = u32::from(base_character);

            // Try to get the base for keys which change based on modifier, like `1` for `!`.
            match key.key_without_modifiers().as_ref() {
                Key::Character(unmodded) if alternate_key_code == unicode_key_code => {
                    unicode_key_code = u32::from(unmodded.chars().next().unwrap_or(base_character));
                },
                _ => (),
            }

            // NOTE: Base layouts are ignored, since winit doesn't expose this information
            // yet.
            let payload = if self.mode.contains(TermMode::REPORT_ALTERNATE_KEYS)
                && alternate_key_code != unicode_key_code
            {
                format!("{unicode_key_code}:{alternate_key_code}")
            } else {
                alternate_key_code.to_string()
            };

            Some(SequenceBase::new(payload.into(), SequenceTerminator::Kitty))
        } else if self.kitty_encode_all
            && self.mode.contains(TermMode::REPORT_ASSOCIATED_TEXT)
            && key.text.is_some()
        {
            // Fallback when need to report text, but we don't have any key associated with this
            // text.
            Some(SequenceBase::new("0".into(), SequenceTerminator::Kitty))
        } else {
            None
        }
    }

    /// Try building from numpad key.
    ///
    /// `None` is returned when the key is neither known nor numpad.
    fn try_build_numpad(&self, key: &KeyEvent) -> Option<SequenceBase> {
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

        Some(SequenceBase::new(base.into(), SequenceTerminator::Kitty))
    }

    /// Try building from [`NamedKey`].
    fn try_build_named(&self, key: &KeyEvent) -> Option<SequenceBase> {
        let named = match key.logical_key {
            Key::Named(named) => named,
            _ => return None,
        };

        // The default parameter is 1, so we can omit it.
        let one_based = if self.modifiers.is_empty() && !self.kitty_event_type { "" } else { "1" };
        let (base, terminator) = match named {
            NamedKey::PageUp => ("5", SequenceTerminator::Normal('~')),
            NamedKey::PageDown => ("6", SequenceTerminator::Normal('~')),
            NamedKey::Insert => ("2", SequenceTerminator::Normal('~')),
            NamedKey::Delete => ("3", SequenceTerminator::Normal('~')),
            NamedKey::Home => (one_based, SequenceTerminator::Normal('H')),
            NamedKey::End => (one_based, SequenceTerminator::Normal('F')),
            NamedKey::ArrowLeft => (one_based, SequenceTerminator::Normal('D')),
            NamedKey::ArrowRight => (one_based, SequenceTerminator::Normal('C')),
            NamedKey::ArrowUp => (one_based, SequenceTerminator::Normal('A')),
            NamedKey::ArrowDown => (one_based, SequenceTerminator::Normal('B')),
            NamedKey::F1 => (one_based, SequenceTerminator::Normal('P')),
            NamedKey::F2 => (one_based, SequenceTerminator::Normal('Q')),
            NamedKey::F3 => {
                // F3 in kitty protocol diverges from alacritty's terminfo.
                if self.kitty_seq {
                    ("13", SequenceTerminator::Normal('~'))
                } else {
                    (one_based, SequenceTerminator::Normal('R'))
                }
            },
            NamedKey::F4 => (one_based, SequenceTerminator::Normal('S')),
            NamedKey::F5 => ("15", SequenceTerminator::Normal('~')),
            NamedKey::F6 => ("17", SequenceTerminator::Normal('~')),
            NamedKey::F7 => ("18", SequenceTerminator::Normal('~')),
            NamedKey::F8 => ("19", SequenceTerminator::Normal('~')),
            NamedKey::F9 => ("20", SequenceTerminator::Normal('~')),
            NamedKey::F10 => ("21", SequenceTerminator::Normal('~')),
            NamedKey::F11 => ("23", SequenceTerminator::Normal('~')),
            NamedKey::F12 => ("24", SequenceTerminator::Normal('~')),
            NamedKey::F13 => ("57376", SequenceTerminator::Kitty),
            NamedKey::F14 => ("57377", SequenceTerminator::Kitty),
            NamedKey::F15 => ("57378", SequenceTerminator::Kitty),
            NamedKey::F16 => ("57379", SequenceTerminator::Kitty),
            NamedKey::F17 => ("57380", SequenceTerminator::Kitty),
            NamedKey::F18 => ("57381", SequenceTerminator::Kitty),
            NamedKey::F19 => ("57382", SequenceTerminator::Kitty),
            NamedKey::F20 => ("57383", SequenceTerminator::Kitty),
            NamedKey::F21 => ("57384", SequenceTerminator::Kitty),
            NamedKey::F22 => ("57385", SequenceTerminator::Kitty),
            NamedKey::F23 => ("57386", SequenceTerminator::Kitty),
            NamedKey::F24 => ("57387", SequenceTerminator::Kitty),
            NamedKey::F25 => ("57388", SequenceTerminator::Kitty),
            NamedKey::F26 => ("57389", SequenceTerminator::Kitty),
            NamedKey::F27 => ("57390", SequenceTerminator::Kitty),
            NamedKey::F28 => ("57391", SequenceTerminator::Kitty),
            NamedKey::F29 => ("57392", SequenceTerminator::Kitty),
            NamedKey::F30 => ("57393", SequenceTerminator::Kitty),
            NamedKey::F31 => ("57394", SequenceTerminator::Kitty),
            NamedKey::F32 => ("57395", SequenceTerminator::Kitty),
            NamedKey::F33 => ("57396", SequenceTerminator::Kitty),
            NamedKey::F34 => ("57397", SequenceTerminator::Kitty),
            NamedKey::F35 => ("57398", SequenceTerminator::Kitty),
            NamedKey::ScrollLock => ("57359", SequenceTerminator::Kitty),
            NamedKey::PrintScreen => ("57361", SequenceTerminator::Kitty),
            NamedKey::Pause => ("57362", SequenceTerminator::Kitty),
            NamedKey::ContextMenu => ("57363", SequenceTerminator::Kitty),
            NamedKey::MediaPlay => ("57428", SequenceTerminator::Kitty),
            NamedKey::MediaPause => ("57429", SequenceTerminator::Kitty),
            NamedKey::MediaPlayPause => ("57430", SequenceTerminator::Kitty),
            NamedKey::MediaStop => ("57432", SequenceTerminator::Kitty),
            NamedKey::MediaFastForward => ("57433", SequenceTerminator::Kitty),
            NamedKey::MediaRewind => ("57434", SequenceTerminator::Kitty),
            NamedKey::MediaTrackNext => ("57435", SequenceTerminator::Kitty),
            NamedKey::MediaTrackPrevious => ("57436", SequenceTerminator::Kitty),
            NamedKey::MediaRecord => ("57437", SequenceTerminator::Kitty),
            NamedKey::AudioVolumeDown => ("57438", SequenceTerminator::Kitty),
            NamedKey::AudioVolumeUp => ("57439", SequenceTerminator::Kitty),
            NamedKey::AudioVolumeMute => ("57440", SequenceTerminator::Kitty),
            _ => return None,
        };

        Some(SequenceBase::new(base.into(), terminator))
    }

    /// Try building escape from control characters (e.g. Enter) and modifiers.
    fn try_build_control_char_or_mod(
        &self,
        key: &KeyEvent,
        mods: &mut SequenceModifiers,
    ) -> Option<SequenceBase> {
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

        // Fail when the key is not a named control character and the active mode prohibits us
        // from encoding modifier keys.
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

        // NOTE: Kitty's protocol mandates that the modifier state is applied before
        // key press, however winit sends them after the key press, so for modifiers
        // itself apply the state based on keysyms and not the _actual_ modifiers
        // state, which is how kitty is doing so and what is suggested in such case.
        let press = key.state.is_pressed();
        match named {
            NamedKey::Shift => mods.set(SequenceModifiers::SHIFT, press),
            NamedKey::Control => mods.set(SequenceModifiers::CONTROL, press),
            NamedKey::Alt => mods.set(SequenceModifiers::ALT, press),
            NamedKey::Super => mods.set(SequenceModifiers::SUPER, press),
            _ => (),
        }

        if base.is_empty() {
            None
        } else {
            Some(SequenceBase::new(base.into(), SequenceTerminator::Kitty))
        }
    }
}

pub struct SequenceBase {
    /// The base of the payload, which is the `number` and optionally an alt base from the kitty
    /// spec.
    payload: Cow<'static, str>,
    terminator: SequenceTerminator,
}

impl SequenceBase {
    fn new(payload: Cow<'static, str>, terminator: SequenceTerminator) -> Self {
        Self { payload, terminator }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SequenceTerminator {
    /// The normal key esc sequence terminator defined by xterm/dec.
    Normal(char),
    /// The terminator is for kitty escape sequence.
    Kitty,
}

impl SequenceTerminator {
    fn encode_esc_sequence(self) -> char {
        match self {
            SequenceTerminator::Normal(char) => char,
            SequenceTerminator::Kitty => 'u',
        }
    }
}

bitflags::bitflags! {
    /// The modifiers encoding for escape sequence.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SequenceModifiers : u8 {
        const SHIFT   = 0b0000_0001;
        const ALT     = 0b0000_0010;
        const CONTROL = 0b0000_0100;
        const SUPER   = 0b0000_1000;
        // NOTE: Kitty protocol defines additional modifiers to what is present here, like
        // Capslock, but it's not a modifier as per winit.
    }
}

impl SequenceModifiers {
    /// Get the value which should be passed to escape sequence.
    pub fn encode_esc_sequence(self) -> u8 {
        self.bits() + 1
    }
}

impl From<ModifiersState> for SequenceModifiers {
    fn from(mods: ModifiersState) -> Self {
        let mut modifiers = Self::empty();
        modifiers.set(Self::SHIFT, mods.shift_key());
        modifiers.set(Self::ALT, mods.alt_key());
        modifiers.set(Self::CONTROL, mods.control_key());
        modifiers.set(Self::SUPER, mods.super_key());
        modifiers
    }
}
