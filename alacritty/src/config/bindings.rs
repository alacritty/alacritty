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

use std::fmt;
use std::str::FromStr;

use glutin::event::{ModifiersState, MouseButton};
use log::error;
use serde::de::Error as SerdeError;
use serde::de::{self, MapAccess, Unexpected, Visitor};
use serde::{Deserialize, Deserializer};

use alacritty_terminal::config::LOG_TARGET_CONFIG;
use alacritty_terminal::term::TermMode;

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
    pub fn is_triggered_by(
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
            && (self.mods == mods || (relaxed && self.mods.relaxed_eq(mods)))
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

    /// Minimize the Alacritty window.
    Minimize,

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

impl From<&'static str> for Action {
    fn from(s: &'static str) -> Action {
        Action::Esc(s.into())
    }
}

pub trait RelaxedEq<T: ?Sized = Self> {
    fn relaxed_eq(&self, other: T) -> bool;
}

impl RelaxedEq for ModifiersState {
    // Make sure that modifiers in the config are always present,
    // but ignore surplus modifiers.
    fn relaxed_eq(&self, other: Self) -> bool {
        !*self | other == ModifiersState::all()
    }
}

macro_rules! bindings {
    (
        $ty:ident;
        $(
            $key:path
            $(,$mods:expr)*
            $(,+$mode:expr)*
            $(,~$notmode:expr)*
            ;$action:expr
        );*
        $(;)*
    ) => {{
        let mut v = Vec::new();

        $(
            let mut _mods = ModifiersState::empty();
            $(_mods = $mods;)*
            let mut _mode = TermMode::empty();
            $(_mode = $mode;)*
            let mut _notmode = TermMode::empty();
            $(_notmode = $notmode;)*

            v.push($ty {
                trigger: $key,
                mods: _mods,
                mode: _mode,
                notmode: _notmode,
                action: $action,
            });
        )*

        v
    }}
}

pub fn default_mouse_bindings() -> Vec<MouseBinding> {
    bindings!(
        MouseBinding;
        MouseButton::Middle; Action::PasteSelection;
    )
}

pub fn default_key_bindings() -> Vec<KeyBinding> {
    let mut bindings = bindings!(
        KeyBinding;
        Key::Paste; Action::Paste;
        Key::Copy;  Action::Copy;
        Key::L, ModifiersState::CTRL; Action::ClearLogNotice;
        Key::L, ModifiersState::CTRL; Action::Esc("\x0c".into());
        Key::PageUp,   ModifiersState::SHIFT, ~TermMode::ALT_SCREEN; Action::ScrollPageUp;
        Key::PageDown, ModifiersState::SHIFT, ~TermMode::ALT_SCREEN; Action::ScrollPageDown;
        Key::Home,     ModifiersState::SHIFT, ~TermMode::ALT_SCREEN; Action::ScrollToTop;
        Key::End,      ModifiersState::SHIFT, ~TermMode::ALT_SCREEN; Action::ScrollToBottom;
        Key::Home, +TermMode::APP_CURSOR; Action::Esc("\x1bOH".into());
        Key::Home, ~TermMode::APP_CURSOR; Action::Esc("\x1b[H".into());
        Key::Home, ModifiersState::SHIFT, +TermMode::ALT_SCREEN; Action::Esc("\x1b[1;2H".into());
        Key::End,  +TermMode::APP_CURSOR; Action::Esc("\x1bOF".into());
        Key::End,  ~TermMode::APP_CURSOR; Action::Esc("\x1b[F".into());
        Key::End,  ModifiersState::SHIFT, +TermMode::ALT_SCREEN; Action::Esc("\x1b[1;2F".into());
        Key::PageUp;   Action::Esc("\x1b[5~".into());
        Key::PageUp,   ModifiersState::SHIFT, +TermMode::ALT_SCREEN; Action::Esc("\x1b[5;2~".into());
        Key::PageDown; Action::Esc("\x1b[6~".into());
        Key::PageDown, ModifiersState::SHIFT, +TermMode::ALT_SCREEN; Action::Esc("\x1b[6;2~".into());
        Key::Tab,  ModifiersState::SHIFT; Action::Esc("\x1b[Z".into());
        Key::Back; Action::Esc("\x7f".into());
        Key::Back, ModifiersState::ALT; Action::Esc("\x1b\x7f".into());
        Key::Insert; Action::Esc("\x1b[2~".into());
        Key::Delete; Action::Esc("\x1b[3~".into());
        Key::Up,    +TermMode::APP_CURSOR; Action::Esc("\x1bOA".into());
        Key::Up,    ~TermMode::APP_CURSOR; Action::Esc("\x1b[A".into());
        Key::Down,  +TermMode::APP_CURSOR; Action::Esc("\x1bOB".into());
        Key::Down,  ~TermMode::APP_CURSOR; Action::Esc("\x1b[B".into());
        Key::Right, +TermMode::APP_CURSOR; Action::Esc("\x1bOC".into());
        Key::Right, ~TermMode::APP_CURSOR; Action::Esc("\x1b[C".into());
        Key::Left,  +TermMode::APP_CURSOR; Action::Esc("\x1bOD".into());
        Key::Left,  ~TermMode::APP_CURSOR; Action::Esc("\x1b[D".into());
        Key::F1;  Action::Esc("\x1bOP".into());
        Key::F2;  Action::Esc("\x1bOQ".into());
        Key::F3;  Action::Esc("\x1bOR".into());
        Key::F4;  Action::Esc("\x1bOS".into());
        Key::F5;  Action::Esc("\x1b[15~".into());
        Key::F6;  Action::Esc("\x1b[17~".into());
        Key::F7;  Action::Esc("\x1b[18~".into());
        Key::F8;  Action::Esc("\x1b[19~".into());
        Key::F9;  Action::Esc("\x1b[20~".into());
        Key::F10; Action::Esc("\x1b[21~".into());
        Key::F11; Action::Esc("\x1b[23~".into());
        Key::F12; Action::Esc("\x1b[24~".into());
        Key::F13; Action::Esc("\x1b[25~".into());
        Key::F14; Action::Esc("\x1b[26~".into());
        Key::F15; Action::Esc("\x1b[28~".into());
        Key::F16; Action::Esc("\x1b[29~".into());
        Key::F17; Action::Esc("\x1b[31~".into());
        Key::F18; Action::Esc("\x1b[32~".into());
        Key::F19; Action::Esc("\x1b[33~".into());
        Key::F20; Action::Esc("\x1b[34~".into());
        Key::NumpadEnter; Action::Esc("\n".into());
    );

    //   Code     Modifiers
    // ---------+---------------------------
    //    2     | Shift
    //    3     | Alt
    //    4     | Shift + Alt
    //    5     | Control
    //    6     | Shift + Control
    //    7     | Alt + Control
    //    8     | Shift + Alt + Control
    // ---------+---------------------------
    //
    // from: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-PC-Style-Function-Keys
    let mut modifiers = vec![
        ModifiersState::SHIFT,
        ModifiersState::ALT,
        ModifiersState::SHIFT | ModifiersState::ALT,
        ModifiersState::CTRL,
        ModifiersState::SHIFT | ModifiersState::CTRL,
        ModifiersState::ALT | ModifiersState::CTRL,
        ModifiersState::SHIFT | ModifiersState::ALT | ModifiersState::CTRL,
    ];

    for (index, mods) in modifiers.drain(..).enumerate() {
        let modifiers_code = index + 2;
        bindings.extend(bindings!(
            KeyBinding;
            Key::Delete, mods; Action::Esc(format!("\x1b[3;{}~", modifiers_code));
            Key::Up,     mods; Action::Esc(format!("\x1b[1;{}A", modifiers_code));
            Key::Down,   mods; Action::Esc(format!("\x1b[1;{}B", modifiers_code));
            Key::Right,  mods; Action::Esc(format!("\x1b[1;{}C", modifiers_code));
            Key::Left,   mods; Action::Esc(format!("\x1b[1;{}D", modifiers_code));
            Key::F1,     mods; Action::Esc(format!("\x1b[1;{}P", modifiers_code));
            Key::F2,     mods; Action::Esc(format!("\x1b[1;{}Q", modifiers_code));
            Key::F3,     mods; Action::Esc(format!("\x1b[1;{}R", modifiers_code));
            Key::F4,     mods; Action::Esc(format!("\x1b[1;{}S", modifiers_code));
            Key::F5,     mods; Action::Esc(format!("\x1b[15;{}~", modifiers_code));
            Key::F6,     mods; Action::Esc(format!("\x1b[17;{}~", modifiers_code));
            Key::F7,     mods; Action::Esc(format!("\x1b[18;{}~", modifiers_code));
            Key::F8,     mods; Action::Esc(format!("\x1b[19;{}~", modifiers_code));
            Key::F9,     mods; Action::Esc(format!("\x1b[20;{}~", modifiers_code));
            Key::F10,    mods; Action::Esc(format!("\x1b[21;{}~", modifiers_code));
            Key::F11,    mods; Action::Esc(format!("\x1b[23;{}~", modifiers_code));
            Key::F12,    mods; Action::Esc(format!("\x1b[24;{}~", modifiers_code));
            Key::F13,    mods; Action::Esc(format!("\x1b[25;{}~", modifiers_code));
            Key::F14,    mods; Action::Esc(format!("\x1b[26;{}~", modifiers_code));
            Key::F15,    mods; Action::Esc(format!("\x1b[28;{}~", modifiers_code));
            Key::F16,    mods; Action::Esc(format!("\x1b[29;{}~", modifiers_code));
            Key::F17,    mods; Action::Esc(format!("\x1b[31;{}~", modifiers_code));
            Key::F18,    mods; Action::Esc(format!("\x1b[32;{}~", modifiers_code));
            Key::F19,    mods; Action::Esc(format!("\x1b[33;{}~", modifiers_code));
            Key::F20,    mods; Action::Esc(format!("\x1b[34;{}~", modifiers_code));
        ));

        // We're adding the following bindings with `Shift` manually above, so skipping them here
        // modifiers_code != Shift
        if modifiers_code != 2 {
            bindings.extend(bindings!(
                KeyBinding;
                Key::Insert,   mods; Action::Esc(format!("\x1b[2;{}~", modifiers_code));
                Key::PageUp,   mods; Action::Esc(format!("\x1b[5;{}~", modifiers_code));
                Key::PageDown, mods; Action::Esc(format!("\x1b[6;{}~", modifiers_code));
                Key::End,      mods; Action::Esc(format!("\x1b[1;{}F", modifiers_code));
                Key::Home,     mods; Action::Esc(format!("\x1b[1;{}H", modifiers_code));
            ));
        }
    }

    bindings.extend(platform_key_bindings());

    bindings
}

#[cfg(not(any(target_os = "macos", test)))]
fn common_keybindings() -> Vec<KeyBinding> {
    bindings!(
        KeyBinding;
        Key::V,        ModifiersState::CTRL | ModifiersState::SHIFT; Action::Paste;
        Key::C,        ModifiersState::CTRL | ModifiersState::SHIFT; Action::Copy;
        Key::Insert,   ModifiersState::SHIFT; Action::PasteSelection;
        Key::Key0,     ModifiersState::CTRL;  Action::ResetFontSize;
        Key::Equals,   ModifiersState::CTRL;  Action::IncreaseFontSize;
        Key::Add,      ModifiersState::CTRL;  Action::IncreaseFontSize;
        Key::Subtract, ModifiersState::CTRL;  Action::DecreaseFontSize;
        Key::Minus,    ModifiersState::CTRL;  Action::DecreaseFontSize;
    )
}

#[cfg(not(any(target_os = "macos", target_os = "windows", test)))]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    common_keybindings()
}

#[cfg(all(target_os = "windows", not(test)))]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    let mut bindings = bindings!(
        KeyBinding;
        Key::Return, ModifiersState::ALT; Action::ToggleFullscreen;
    );
    bindings.extend(common_keybindings());
    bindings
}

#[cfg(all(target_os = "macos", not(test)))]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    bindings!(
        KeyBinding;
        Key::Key0,   ModifiersState::LOGO;  Action::ResetFontSize;
        Key::Equals, ModifiersState::LOGO;  Action::IncreaseFontSize;
        Key::Add,    ModifiersState::LOGO;  Action::IncreaseFontSize;
        Key::Minus,  ModifiersState::LOGO;  Action::DecreaseFontSize;
        Key::Insert, ModifiersState::SHIFT; Action::Esc("\x1b[2;2~".into());
        Key::F, ModifiersState::CTRL | ModifiersState::LOGO; Action::ToggleFullscreen;
        Key::K, ModifiersState::LOGO; Action::ClearHistory;
        Key::K, ModifiersState::LOGO; Action::Esc("\x0c".into());
        Key::V, ModifiersState::LOGO; Action::Paste;
        Key::C, ModifiersState::LOGO; Action::Copy;
        Key::H, ModifiersState::LOGO; Action::Hide;
        Key::M, ModifiersState::LOGO; Action::Minimize;
        Key::Q, ModifiersState::LOGO; Action::Quit;
        Key::W, ModifiersState::LOGO; Action::Quit;
    )
}

// Don't return any bindings for tests since they are commented-out by default
#[cfg(test)]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    vec![]
}

#[derive(Deserialize, Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Key {
    Scancode(u32),
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Snapshot,
    Scroll,
    Pause,
    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,
    Left,
    Up,
    Right,
    Down,
    Back,
    Return,
    Space,
    Compose,
    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    AbntC1,
    AbntC2,
    Add,
    Apostrophe,
    Apps,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    Decimal,
    Divide,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    Multiply,
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    OEM102,
    Period,
    PlayPause,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    Subtract,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
    Caret,
    Copy,
    Paste,
    Cut,
}

impl Key {
    pub fn from_glutin_input(key: glutin::event::VirtualKeyCode) -> Self {
        use glutin::event::VirtualKeyCode::*;
        // Thank you, vim macros and regex!
        match key {
            Key1 => Key::Key1,
            Key2 => Key::Key2,
            Key3 => Key::Key3,
            Key4 => Key::Key4,
            Key5 => Key::Key5,
            Key6 => Key::Key6,
            Key7 => Key::Key7,
            Key8 => Key::Key8,
            Key9 => Key::Key9,
            Key0 => Key::Key0,
            A => Key::A,
            B => Key::B,
            C => Key::C,
            D => Key::D,
            E => Key::E,
            F => Key::F,
            G => Key::G,
            H => Key::H,
            I => Key::I,
            J => Key::J,
            K => Key::K,
            L => Key::L,
            M => Key::M,
            N => Key::N,
            O => Key::O,
            P => Key::P,
            Q => Key::Q,
            R => Key::R,
            S => Key::S,
            T => Key::T,
            U => Key::U,
            V => Key::V,
            W => Key::W,
            X => Key::X,
            Y => Key::Y,
            Z => Key::Z,
            Escape => Key::Escape,
            F1 => Key::F1,
            F2 => Key::F2,
            F3 => Key::F3,
            F4 => Key::F4,
            F5 => Key::F5,
            F6 => Key::F6,
            F7 => Key::F7,
            F8 => Key::F8,
            F9 => Key::F9,
            F10 => Key::F10,
            F11 => Key::F11,
            F12 => Key::F12,
            F13 => Key::F13,
            F14 => Key::F14,
            F15 => Key::F15,
            F16 => Key::F16,
            F17 => Key::F17,
            F18 => Key::F18,
            F19 => Key::F19,
            F20 => Key::F20,
            F21 => Key::F21,
            F22 => Key::F22,
            F23 => Key::F23,
            F24 => Key::F24,
            Snapshot => Key::Snapshot,
            Scroll => Key::Scroll,
            Pause => Key::Pause,
            Insert => Key::Insert,
            Home => Key::Home,
            Delete => Key::Delete,
            End => Key::End,
            PageDown => Key::PageDown,
            PageUp => Key::PageUp,
            Left => Key::Left,
            Up => Key::Up,
            Right => Key::Right,
            Down => Key::Down,
            Back => Key::Back,
            Return => Key::Return,
            Space => Key::Space,
            Compose => Key::Compose,
            Numlock => Key::Numlock,
            Numpad0 => Key::Numpad0,
            Numpad1 => Key::Numpad1,
            Numpad2 => Key::Numpad2,
            Numpad3 => Key::Numpad3,
            Numpad4 => Key::Numpad4,
            Numpad5 => Key::Numpad5,
            Numpad6 => Key::Numpad6,
            Numpad7 => Key::Numpad7,
            Numpad8 => Key::Numpad8,
            Numpad9 => Key::Numpad9,
            AbntC1 => Key::AbntC1,
            AbntC2 => Key::AbntC2,
            Add => Key::Add,
            Apostrophe => Key::Apostrophe,
            Apps => Key::Apps,
            At => Key::At,
            Ax => Key::Ax,
            Backslash => Key::Backslash,
            Calculator => Key::Calculator,
            Capital => Key::Capital,
            Colon => Key::Colon,
            Comma => Key::Comma,
            Convert => Key::Convert,
            Decimal => Key::Decimal,
            Divide => Key::Divide,
            Equals => Key::Equals,
            Grave => Key::Grave,
            Kana => Key::Kana,
            Kanji => Key::Kanji,
            LAlt => Key::LAlt,
            LBracket => Key::LBracket,
            LControl => Key::LControl,
            LShift => Key::LShift,
            LWin => Key::LWin,
            Mail => Key::Mail,
            MediaSelect => Key::MediaSelect,
            MediaStop => Key::MediaStop,
            Minus => Key::Minus,
            Multiply => Key::Multiply,
            Mute => Key::Mute,
            MyComputer => Key::MyComputer,
            NavigateForward => Key::NavigateForward,
            NavigateBackward => Key::NavigateBackward,
            NextTrack => Key::NextTrack,
            NoConvert => Key::NoConvert,
            NumpadComma => Key::NumpadComma,
            NumpadEnter => Key::NumpadEnter,
            NumpadEquals => Key::NumpadEquals,
            OEM102 => Key::OEM102,
            Period => Key::Period,
            PlayPause => Key::PlayPause,
            Power => Key::Power,
            PrevTrack => Key::PrevTrack,
            RAlt => Key::RAlt,
            RBracket => Key::RBracket,
            RControl => Key::RControl,
            RShift => Key::RShift,
            RWin => Key::RWin,
            Semicolon => Key::Semicolon,
            Slash => Key::Slash,
            Sleep => Key::Sleep,
            Stop => Key::Stop,
            Subtract => Key::Subtract,
            Sysrq => Key::Sysrq,
            Tab => Key::Tab,
            Underline => Key::Underline,
            Unlabeled => Key::Unlabeled,
            VolumeDown => Key::VolumeDown,
            VolumeUp => Key::VolumeUp,
            Wake => Key::Wake,
            WebBack => Key::WebBack,
            WebFavorites => Key::WebFavorites,
            WebForward => Key::WebForward,
            WebHome => Key::WebHome,
            WebRefresh => Key::WebRefresh,
            WebSearch => Key::WebSearch,
            WebStop => Key::WebStop,
            Yen => Key::Yen,
            Caret => Key::Caret,
            Copy => Key::Copy,
            Paste => Key::Paste,
            Cut => Key::Cut,
        }
    }
}

struct ModeWrapper {
    pub mode: TermMode,
    pub not_mode: TermMode,
}

impl<'a> Deserialize<'a> for ModeWrapper {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        struct ModeVisitor;

        impl<'a> Visitor<'a> for ModeVisitor {
            type Value = ModeWrapper;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Combination of AppCursor | AppKeypad, possibly with negation (~)")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<ModeWrapper, E>
            where
                E: de::Error,
            {
                let mut res = ModeWrapper { mode: TermMode::empty(), not_mode: TermMode::empty() };

                for modifier in value.split('|') {
                    match modifier.trim().to_lowercase().as_str() {
                        "appcursor" => res.mode |= TermMode::APP_CURSOR,
                        "~appcursor" => res.not_mode |= TermMode::APP_CURSOR,
                        "appkeypad" => res.mode |= TermMode::APP_KEYPAD,
                        "~appkeypad" => res.not_mode |= TermMode::APP_KEYPAD,
                        "~alt" => res.not_mode |= TermMode::ALT_SCREEN,
                        "alt" => res.mode |= TermMode::ALT_SCREEN,
                        _ => error!(target: LOG_TARGET_CONFIG, "Unknown mode {:?}", modifier),
                    }
                }

                Ok(res)
            }
        }
        deserializer.deserialize_str(ModeVisitor)
    }
}

struct MouseButtonWrapper(MouseButton);

impl MouseButtonWrapper {
    fn into_inner(self) -> MouseButton {
        self.0
    }
}

impl<'a> Deserialize<'a> for MouseButtonWrapper {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        struct MouseButtonVisitor;

        impl<'a> Visitor<'a> for MouseButtonVisitor {
            type Value = MouseButtonWrapper;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Left, Right, Middle, or a number")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<MouseButtonWrapper, E>
            where
                E: de::Error,
            {
                match value {
                    "Left" => Ok(MouseButtonWrapper(MouseButton::Left)),
                    "Right" => Ok(MouseButtonWrapper(MouseButton::Right)),
                    "Middle" => Ok(MouseButtonWrapper(MouseButton::Middle)),
                    _ => {
                        if let Ok(index) = u8::from_str(value) {
                            Ok(MouseButtonWrapper(MouseButton::Other(index)))
                        } else {
                            Err(E::invalid_value(Unexpected::Str(value), &self))
                        }
                    },
                }
            }
        }

        deserializer.deserialize_str(MouseButtonVisitor)
    }
}

/// Bindings are deserialized into a `RawBinding` before being parsed as a
/// `KeyBinding` or `MouseBinding`.
#[derive(PartialEq, Eq)]
struct RawBinding {
    key: Option<Key>,
    mouse: Option<MouseButton>,
    mods: ModifiersState,
    mode: TermMode,
    notmode: TermMode,
    action: Action,
}

impl RawBinding {
    fn into_mouse_binding(self) -> ::std::result::Result<MouseBinding, Self> {
        if let Some(mouse) = self.mouse {
            Ok(Binding {
                trigger: mouse,
                mods: self.mods,
                action: self.action,
                mode: self.mode,
                notmode: self.notmode,
            })
        } else {
            Err(self)
        }
    }

    fn into_key_binding(self) -> ::std::result::Result<KeyBinding, Self> {
        if let Some(key) = self.key {
            Ok(KeyBinding {
                trigger: key,
                mods: self.mods,
                action: self.action,
                mode: self.mode,
                notmode: self.notmode,
            })
        } else {
            Err(self)
        }
    }
}

impl<'a> Deserialize<'a> for RawBinding {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        enum Field {
            Key,
            Mods,
            Mode,
            Action,
            Chars,
            Mouse,
            Command,
        }

        impl<'a> Deserialize<'a> for Field {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Field, D::Error>
            where
                D: Deserializer<'a>,
            {
                struct FieldVisitor;

                static FIELDS: &[&str] =
                    &["key", "mods", "mode", "action", "chars", "mouse", "command"];

                impl<'a> Visitor<'a> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("binding fields")
                    }

                    fn visit_str<E>(self, value: &str) -> ::std::result::Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "key" => Ok(Field::Key),
                            "mods" => Ok(Field::Mods),
                            "mode" => Ok(Field::Mode),
                            "action" => Ok(Field::Action),
                            "chars" => Ok(Field::Chars),
                            "mouse" => Ok(Field::Mouse),
                            "command" => Ok(Field::Command),
                            _ => Err(E::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_str(FieldVisitor)
            }
        }

        struct RawBindingVisitor;
        impl<'a> Visitor<'a> for RawBindingVisitor {
            type Value = RawBinding;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("binding specification")
            }

            fn visit_map<V>(self, mut map: V) -> ::std::result::Result<RawBinding, V::Error>
            where
                V: MapAccess<'a>,
            {
                let mut mods: Option<ModifiersState> = None;
                let mut key: Option<Key> = None;
                let mut chars: Option<String> = None;
                let mut action: Option<Action> = None;
                let mut mode: Option<TermMode> = None;
                let mut not_mode: Option<TermMode> = None;
                let mut mouse: Option<MouseButton> = None;
                let mut command: Option<CommandWrapper> = None;

                use ::serde::de::Error;

                while let Some(struct_key) = map.next_key::<Field>()? {
                    match struct_key {
                        Field::Key => {
                            if key.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("key"));
                            }

                            let val = map.next_value::<serde_yaml::Value>()?;
                            if val.is_u64() {
                                let scancode = val.as_u64().unwrap();
                                if scancode > u64::from(::std::u32::MAX) {
                                    return Err(<V::Error as Error>::custom(format!(
                                        "Invalid key binding, scancode too big: {}",
                                        scancode
                                    )));
                                }
                                key = Some(Key::Scancode(scancode as u32));
                            } else {
                                let k = Key::deserialize(val).map_err(V::Error::custom)?;
                                key = Some(k);
                            }
                        },
                        Field::Mods => {
                            if mods.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("mods"));
                            }

                            mods = Some(map.next_value::<ModsWrapper>()?.into_inner());
                        },
                        Field::Mode => {
                            if mode.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("mode"));
                            }

                            let mode_deserializer = map.next_value::<ModeWrapper>()?;
                            mode = Some(mode_deserializer.mode);
                            not_mode = Some(mode_deserializer.not_mode);
                        },
                        Field::Action => {
                            if action.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("action"));
                            }

                            action = Some(map.next_value::<Action>()?);
                        },
                        Field::Chars => {
                            if chars.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("chars"));
                            }

                            chars = Some(map.next_value()?);
                        },
                        Field::Mouse => {
                            if chars.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("mouse"));
                            }

                            mouse = Some(map.next_value::<MouseButtonWrapper>()?.into_inner());
                        },
                        Field::Command => {
                            if command.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("command"));
                            }

                            command = Some(map.next_value::<CommandWrapper>()?);
                        },
                    }
                }

                let action = match (action, chars, command) {
                    (Some(action), None, None) => action,
                    (None, Some(chars), None) => Action::Esc(chars),
                    (None, None, Some(cmd)) => match cmd {
                        CommandWrapper::Just(program) => Action::Command(program, vec![]),
                        CommandWrapper::WithArgs { program, args } => {
                            Action::Command(program, args)
                        },
                    },
                    (None, None, None) => {
                        return Err(V::Error::custom("must specify chars, action or command"));
                    },
                    _ => {
                        return Err(V::Error::custom("must specify only chars, action or command"))
                    },
                };

                let mode = mode.unwrap_or_else(TermMode::empty);
                let not_mode = not_mode.unwrap_or_else(TermMode::empty);
                let mods = mods.unwrap_or_else(ModifiersState::default);

                if mouse.is_none() && key.is_none() {
                    return Err(V::Error::custom("bindings require mouse button or key"));
                }

                Ok(RawBinding { mode, notmode: not_mode, action, key, mouse, mods })
            }
        }

        const FIELDS: &[&str] = &["key", "mods", "mode", "action", "chars", "mouse", "command"];

        deserializer.deserialize_struct("RawBinding", FIELDS, RawBindingVisitor)
    }
}

impl<'a> Deserialize<'a> for MouseBinding {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        let raw = RawBinding::deserialize(deserializer)?;
        raw.into_mouse_binding().map_err(|_| D::Error::custom("expected mouse binding"))
    }
}

impl<'a> Deserialize<'a> for KeyBinding {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        let raw = RawBinding::deserialize(deserializer)?;
        raw.into_key_binding().map_err(|_| D::Error::custom("expected key binding"))
    }
}

#[serde(untagged)]
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub enum CommandWrapper {
    Just(String),
    WithArgs {
        program: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl CommandWrapper {
    pub fn program(&self) -> &str {
        match self {
            CommandWrapper::Just(program) => program,
            CommandWrapper::WithArgs { program, .. } => program,
        }
    }

    pub fn args(&self) -> &[String] {
        match self {
            CommandWrapper::Just(_) => &[],
            CommandWrapper::WithArgs { args, .. } => args,
        }
    }
}

/// Newtype for implementing deserialize on glutin Mods
///
/// Our deserialize impl wouldn't be covered by a derive(Deserialize); see the
/// impl below.
#[derive(Debug, Copy, Clone, Hash, Default, Eq, PartialEq)]
pub struct ModsWrapper(ModifiersState);

impl ModsWrapper {
    pub fn into_inner(self) -> ModifiersState {
        self.0
    }
}

impl<'a> de::Deserialize<'a> for ModsWrapper {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: de::Deserializer<'a>,
    {
        struct ModsVisitor;

        impl<'a> Visitor<'a> for ModsVisitor {
            type Value = ModsWrapper;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Some subset of Command|Shift|Super|Alt|Option|Control")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<ModsWrapper, E>
            where
                E: de::Error,
            {
                let mut res = ModifiersState::empty();
                for modifier in value.split('|') {
                    match modifier.trim().to_lowercase().as_str() {
                        "command" | "super" => res.insert(ModifiersState::LOGO),
                        "shift" => res.insert(ModifiersState::SHIFT),
                        "alt" | "option" => res.insert(ModifiersState::ALT),
                        "control" => res.insert(ModifiersState::CTRL),
                        "none" => (),
                        _ => error!(target: LOG_TARGET_CONFIG, "Unknown modifier {:?}", modifier),
                    }
                }

                Ok(ModsWrapper(res))
            }
        }

        deserializer.deserialize_str(ModsVisitor)
    }
}

#[cfg(test)]
mod test {
    use glutin::event::ModifiersState;

    use alacritty_terminal::term::TermMode;

    use crate::config::{Action, Binding};

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
        superset_mods.mods = ModifiersState::all();
        let mut subset_mods = MockBinding::default();
        subset_mods.mods = ModifiersState::ALT;

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
        binding.mods = ModifiersState::ALT | ModifiersState::LOGO;

        let superset_mods = ModifiersState::all();
        let subset_mods = ModifiersState::empty();

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
}
