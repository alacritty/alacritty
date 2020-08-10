#![allow(clippy::enum_glob_use)]

use std::fmt::{self, Debug, Display};

use glutin::event::VirtualKeyCode::*;
use glutin::event::{ModifiersState, MouseButton, VirtualKeyCode};
use serde::de::Error as SerdeError;
use serde::de::{self, MapAccess, Unexpected, Visitor};
use serde::{Deserialize, Deserializer};
use serde_yaml::Value as SerdeValue;

use alacritty_terminal::config::Program;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vi_mode::ViMotion;

/// Describes a state and action to take in that state.
///
/// This is the shared component of `MouseBinding` and `KeyBinding`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding<T> {
    /// Modifier keys required to activate binding.
    pub mods: ModifiersState,

    /// String to send to PTY if mods and mode match.
    pub action: Action,

    /// Terminal mode required to activate binding.
    pub mode: TermMode,

    /// excluded terminal modes where the binding won't be activated.
    pub notmode: TermMode,

    /// This property is used as part of the trigger detection code.
    ///
    /// For example, this might be a key like "G", or a mouse button.
    pub trigger: T,
}

/// Bindings that are triggered by a keyboard key.
pub type KeyBinding = Binding<Key>;

/// Bindings that are triggered by a mouse button.
pub type MouseBinding = Binding<MouseButton>;

impl<T: Eq> Binding<T> {
    #[inline]
    pub fn is_triggered_by(&self, mode: TermMode, mods: ModifiersState, input: &T) -> bool {
        // Check input first since bindings are stored in one big list. This is
        // the most likely item to fail so prioritizing it here allows more
        // checks to be short circuited.
        self.trigger == *input
            && self.mods == mods
            && mode.contains(self.mode)
            && !mode.intersects(self.notmode)
    }

    #[inline]
    pub fn triggers_match(&self, binding: &Binding<T>) -> bool {
        // Check the binding's key and modifiers.
        if self.trigger != binding.trigger || self.mods != binding.mods {
            return false;
        }

        let selfmode = if self.mode.is_empty() { TermMode::ANY } else { self.mode };
        let bindingmode = if binding.mode.is_empty() { TermMode::ANY } else { binding.mode };

        if !selfmode.intersects(bindingmode) {
            return false;
        }

        // The bindings are never active at the same time when the required modes of one binding
        // are part of the forbidden bindings of the other.
        if self.mode.intersects(binding.notmode) || binding.mode.intersects(self.notmode) {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum Action {
    /// Write an escape sequence.
    #[serde(skip)]
    Esc(String),

    /// Run given command.
    #[serde(skip)]
    Command(Program),

    /// Move vi mode cursor.
    #[serde(skip)]
    ViMotion(ViMotion),

    /// Perform vi mode action.
    #[serde(skip)]
    ViAction(ViAction),

    /// Paste contents of system clipboard.
    Paste,

    /// Store current selection into clipboard.
    Copy,

    #[cfg(not(any(target_os = "macos", windows)))]
    /// Store current selection into selection buffer.
    CopySelection,

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

    /// Scroll half a page up.
    ScrollHalfPageUp,

    /// Scroll half a page down.
    ScrollHalfPageDown,

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

    /// Toggle simple fullscreen on macOS.
    #[cfg(target_os = "macos")]
    ToggleSimpleFullscreen,

    /// Clear active selection.
    ClearSelection,

    /// Toggle vi mode.
    ToggleViMode,

    /// Allow receiving char input.
    ReceiveChar,

    /// Start a forward buffer search.
    SearchForward,

    /// Start a backward buffer search.
    SearchBackward,

    /// No action.
    None,
}

impl From<&'static str> for Action {
    fn from(s: &'static str) -> Action {
        Action::Esc(s.into())
    }
}

/// Display trait used for error logging.
impl Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::ViMotion(motion) => motion.fmt(f),
            Action::ViAction(action) => action.fmt(f),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Vi mode specific actions.
#[derive(Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub enum ViAction {
    /// Toggle normal vi selection.
    ToggleNormalSelection,
    /// Toggle line vi selection.
    ToggleLineSelection,
    /// Toggle block vi selection.
    ToggleBlockSelection,
    /// Toggle semantic vi selection.
    ToggleSemanticSelection,
    /// Jump to the beginning of the next match.
    SearchNext,
    /// Jump to the beginning of the previous match.
    SearchPrevious,
    /// Jump to the next start of a match to the left of the origin.
    SearchStart,
    /// Jump to the next end of a match to the right of the origin.
    SearchEnd,
    /// Launch the URL below the vi mode cursor.
    Open,
}

impl From<ViAction> for Action {
    fn from(action: ViAction) -> Self {
        Self::ViAction(action)
    }
}

impl From<ViMotion> for Action {
    fn from(motion: ViMotion) -> Self {
        Self::ViMotion(motion)
    }
}

macro_rules! bindings {
    (
        KeyBinding;
        $(
            $key:ident
            $(,$mods:expr)*
            $(,+$mode:expr)*
            $(,~$notmode:expr)*
            ;$action:expr
        );*
        $(;)*
    ) => {{
        bindings!(
            KeyBinding;
            $(
                Key::Keycode($key)
                $(,$mods)*
                $(,+$mode)*
                $(,~$notmode)*
                ;$action
            );*
        )
    }};
    (
        $ty:ident;
        $(
            $key:expr
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
            $(_mode.insert($mode);)*
            let mut _notmode = TermMode::empty();
            $(_notmode.insert($notmode);)*

            v.push($ty {
                trigger: $key,
                mods: _mods,
                mode: _mode,
                notmode: _notmode,
                action: $action.into(),
            });
        )*

        v
    }};
}

pub fn default_mouse_bindings() -> Vec<MouseBinding> {
    bindings!(
        MouseBinding;
        MouseButton::Middle, ~TermMode::VI; Action::PasteSelection;
    )
}

pub fn default_key_bindings() -> Vec<KeyBinding> {
    let mut bindings = bindings!(
        KeyBinding;
        Copy;  Action::Copy;
        Copy,  +TermMode::VI; Action::ClearSelection;
        Paste, ~TermMode::VI; Action::Paste;
        L, ModifiersState::CTRL; Action::ClearLogNotice;
        L,    ModifiersState::CTRL,  ~TermMode::VI; Action::Esc("\x0c".into());
        Tab,  ModifiersState::SHIFT, ~TermMode::VI; Action::Esc("\x1b[Z".into());
        Back, ModifiersState::ALT,   ~TermMode::VI; Action::Esc("\x1b\x7f".into());
        Back, ModifiersState::SHIFT, ~TermMode::VI; Action::Esc("\x7f".into());
        Home,     ModifiersState::SHIFT, ~TermMode::ALT_SCREEN; Action::ScrollToTop;
        End,      ModifiersState::SHIFT, ~TermMode::ALT_SCREEN; Action::ScrollToBottom;
        PageUp,   ModifiersState::SHIFT, ~TermMode::ALT_SCREEN; Action::ScrollPageUp;
        PageDown, ModifiersState::SHIFT, ~TermMode::ALT_SCREEN; Action::ScrollPageDown;
        Home,     ModifiersState::SHIFT, +TermMode::ALT_SCREEN, ~TermMode::VI;
            Action::Esc("\x1b[1;2H".into());
        End,      ModifiersState::SHIFT, +TermMode::ALT_SCREEN, ~TermMode::VI;
            Action::Esc("\x1b[1;2F".into());
        PageUp,   ModifiersState::SHIFT, +TermMode::ALT_SCREEN, ~TermMode::VI;
            Action::Esc("\x1b[5;2~".into());
        PageDown, ModifiersState::SHIFT, +TermMode::ALT_SCREEN, ~TermMode::VI;
            Action::Esc("\x1b[6;2~".into());
        Home,  +TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1bOH".into());
        Home,  ~TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1b[H".into());
        End,   +TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1bOF".into());
        End,   ~TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1b[F".into());
        Up,    +TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1bOA".into());
        Up,    ~TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1b[A".into());
        Down,  +TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1bOB".into());
        Down,  ~TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1b[B".into());
        Right, +TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1bOC".into());
        Right, ~TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1b[C".into());
        Left,  +TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1bOD".into());
        Left,  ~TermMode::APP_CURSOR, ~TermMode::VI; Action::Esc("\x1b[D".into());
        Back,        ~TermMode::VI; Action::Esc("\x7f".into());
        Insert,      ~TermMode::VI; Action::Esc("\x1b[2~".into());
        Delete,      ~TermMode::VI; Action::Esc("\x1b[3~".into());
        PageUp,      ~TermMode::VI; Action::Esc("\x1b[5~".into());
        PageDown,    ~TermMode::VI; Action::Esc("\x1b[6~".into());
        F1,          ~TermMode::VI; Action::Esc("\x1bOP".into());
        F2,          ~TermMode::VI; Action::Esc("\x1bOQ".into());
        F3,          ~TermMode::VI; Action::Esc("\x1bOR".into());
        F4,          ~TermMode::VI; Action::Esc("\x1bOS".into());
        F5,          ~TermMode::VI; Action::Esc("\x1b[15~".into());
        F6,          ~TermMode::VI; Action::Esc("\x1b[17~".into());
        F7,          ~TermMode::VI; Action::Esc("\x1b[18~".into());
        F8,          ~TermMode::VI; Action::Esc("\x1b[19~".into());
        F9,          ~TermMode::VI; Action::Esc("\x1b[20~".into());
        F10,         ~TermMode::VI; Action::Esc("\x1b[21~".into());
        F11,         ~TermMode::VI; Action::Esc("\x1b[23~".into());
        F12,         ~TermMode::VI; Action::Esc("\x1b[24~".into());
        F13,         ~TermMode::VI; Action::Esc("\x1b[25~".into());
        F14,         ~TermMode::VI; Action::Esc("\x1b[26~".into());
        F15,         ~TermMode::VI; Action::Esc("\x1b[28~".into());
        F16,         ~TermMode::VI; Action::Esc("\x1b[29~".into());
        F17,         ~TermMode::VI; Action::Esc("\x1b[31~".into());
        F18,         ~TermMode::VI; Action::Esc("\x1b[32~".into());
        F19,         ~TermMode::VI; Action::Esc("\x1b[33~".into());
        F20,         ~TermMode::VI; Action::Esc("\x1b[34~".into());
        NumpadEnter, ~TermMode::VI; Action::Esc("\n".into());
        Space, ModifiersState::SHIFT | ModifiersState::CTRL, +TermMode::VI; Action::ScrollToBottom;
        Space, ModifiersState::SHIFT | ModifiersState::CTRL; Action::ToggleViMode;
        Escape,                        +TermMode::VI; Action::ClearSelection;
        I,                             +TermMode::VI; Action::ScrollToBottom;
        I,                             +TermMode::VI; Action::ToggleViMode;
        C,      ModifiersState::CTRL,  +TermMode::VI; Action::ToggleViMode;
        Y,      ModifiersState::CTRL,  +TermMode::VI; Action::ScrollLineUp;
        E,      ModifiersState::CTRL,  +TermMode::VI; Action::ScrollLineDown;
        G,                             +TermMode::VI; Action::ScrollToTop;
        G,      ModifiersState::SHIFT, +TermMode::VI; Action::ScrollToBottom;
        B,      ModifiersState::CTRL,  +TermMode::VI; Action::ScrollPageUp;
        F,      ModifiersState::CTRL,  +TermMode::VI; Action::ScrollPageDown;
        U,      ModifiersState::CTRL,  +TermMode::VI; Action::ScrollHalfPageUp;
        D,      ModifiersState::CTRL,  +TermMode::VI; Action::ScrollHalfPageDown;
        Y,                             +TermMode::VI; Action::Copy;
        Y,                             +TermMode::VI; Action::ClearSelection;
        Slash,                         +TermMode::VI; Action::SearchForward;
        Slash,  ModifiersState::SHIFT, +TermMode::VI; Action::SearchBackward;
        V,                             +TermMode::VI; ViAction::ToggleNormalSelection;
        V,      ModifiersState::SHIFT, +TermMode::VI; ViAction::ToggleLineSelection;
        V,      ModifiersState::CTRL,  +TermMode::VI; ViAction::ToggleBlockSelection;
        V,      ModifiersState::ALT,   +TermMode::VI; ViAction::ToggleSemanticSelection;
        N,                             +TermMode::VI; ViAction::SearchNext;
        N,      ModifiersState::SHIFT, +TermMode::VI; ViAction::SearchPrevious;
        Return,                        +TermMode::VI; ViAction::Open;
        K,                             +TermMode::VI; ViMotion::Up;
        J,                             +TermMode::VI; ViMotion::Down;
        H,                             +TermMode::VI; ViMotion::Left;
        L,                             +TermMode::VI; ViMotion::Right;
        Up,                            +TermMode::VI; ViMotion::Up;
        Down,                          +TermMode::VI; ViMotion::Down;
        Left,                          +TermMode::VI; ViMotion::Left;
        Right,                         +TermMode::VI; ViMotion::Right;
        Key0,                          +TermMode::VI; ViMotion::First;
        Key4,   ModifiersState::SHIFT, +TermMode::VI; ViMotion::Last;
        Key6,   ModifiersState::SHIFT, +TermMode::VI; ViMotion::FirstOccupied;
        H,      ModifiersState::SHIFT, +TermMode::VI; ViMotion::High;
        M,      ModifiersState::SHIFT, +TermMode::VI; ViMotion::Middle;
        L,      ModifiersState::SHIFT, +TermMode::VI; ViMotion::Low;
        B,                             +TermMode::VI; ViMotion::SemanticLeft;
        W,                             +TermMode::VI; ViMotion::SemanticRight;
        E,                             +TermMode::VI; ViMotion::SemanticRightEnd;
        B,      ModifiersState::SHIFT, +TermMode::VI; ViMotion::WordLeft;
        W,      ModifiersState::SHIFT, +TermMode::VI; ViMotion::WordRight;
        E,      ModifiersState::SHIFT, +TermMode::VI; ViMotion::WordRightEnd;
        Key5,   ModifiersState::SHIFT, +TermMode::VI; ViMotion::Bracket;
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
            Delete, mods, ~TermMode::VI; Action::Esc(format!("\x1b[3;{}~", modifiers_code));
            Up,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}A", modifiers_code));
            Down,   mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}B", modifiers_code));
            Right,  mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}C", modifiers_code));
            Left,   mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}D", modifiers_code));
            F1,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}P", modifiers_code));
            F2,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}Q", modifiers_code));
            F3,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}R", modifiers_code));
            F4,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}S", modifiers_code));
            F5,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[15;{}~", modifiers_code));
            F6,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[17;{}~", modifiers_code));
            F7,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[18;{}~", modifiers_code));
            F8,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[19;{}~", modifiers_code));
            F9,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[20;{}~", modifiers_code));
            F10,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[21;{}~", modifiers_code));
            F11,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[23;{}~", modifiers_code));
            F12,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[24;{}~", modifiers_code));
            F13,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[25;{}~", modifiers_code));
            F14,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[26;{}~", modifiers_code));
            F15,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[28;{}~", modifiers_code));
            F16,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[29;{}~", modifiers_code));
            F17,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[31;{}~", modifiers_code));
            F18,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[32;{}~", modifiers_code));
            F19,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[33;{}~", modifiers_code));
            F20,    mods, ~TermMode::VI; Action::Esc(format!("\x1b[34;{}~", modifiers_code));
        ));

        // We're adding the following bindings with `Shift` manually above, so skipping them here.
        if modifiers_code != 2 {
            bindings.extend(bindings!(
                KeyBinding;
                Insert,   mods, ~TermMode::VI; Action::Esc(format!("\x1b[2;{}~", modifiers_code));
                PageUp,   mods, ~TermMode::VI; Action::Esc(format!("\x1b[5;{}~", modifiers_code));
                PageDown, mods, ~TermMode::VI; Action::Esc(format!("\x1b[6;{}~", modifiers_code));
                End,      mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}F", modifiers_code));
                Home,     mods, ~TermMode::VI; Action::Esc(format!("\x1b[1;{}H", modifiers_code));
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
        V,        ModifiersState::CTRL | ModifiersState::SHIFT, ~TermMode::VI; Action::Paste;
        C,        ModifiersState::CTRL | ModifiersState::SHIFT; Action::Copy;
        F,        ModifiersState::CTRL | ModifiersState::SHIFT; Action::SearchForward;
        B,        ModifiersState::CTRL | ModifiersState::SHIFT; Action::SearchBackward;
        C,        ModifiersState::CTRL | ModifiersState::SHIFT, +TermMode::VI; Action::ClearSelection;
        Insert,   ModifiersState::SHIFT, ~TermMode::VI; Action::PasteSelection;
        Key0,     ModifiersState::CTRL;  Action::ResetFontSize;
        Equals,   ModifiersState::CTRL;  Action::IncreaseFontSize;
        Add,      ModifiersState::CTRL;  Action::IncreaseFontSize;
        Subtract, ModifiersState::CTRL;  Action::DecreaseFontSize;
        Minus,    ModifiersState::CTRL;  Action::DecreaseFontSize;
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
        Return, ModifiersState::ALT; Action::ToggleFullscreen;
    );
    bindings.extend(common_keybindings());
    bindings
}

#[cfg(all(target_os = "macos", not(test)))]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    bindings!(
        KeyBinding;
        Key0,   ModifiersState::LOGO; Action::ResetFontSize;
        Equals, ModifiersState::LOGO; Action::IncreaseFontSize;
        Add,    ModifiersState::LOGO; Action::IncreaseFontSize;
        Minus,  ModifiersState::LOGO; Action::DecreaseFontSize;
        Insert, ModifiersState::SHIFT, ~TermMode::VI; Action::Esc("\x1b[2;2~".into());
        K, ModifiersState::LOGO, ~TermMode::VI; Action::Esc("\x0c".into());
        V, ModifiersState::LOGO, ~TermMode::VI; Action::Paste;
        N, ModifiersState::LOGO; Action::SpawnNewInstance;
        F, ModifiersState::CTRL | ModifiersState::LOGO; Action::ToggleFullscreen;
        K, ModifiersState::LOGO; Action::ClearHistory;
        C, ModifiersState::LOGO; Action::Copy;
        C, ModifiersState::LOGO, +TermMode::VI; Action::ClearSelection;
        H, ModifiersState::LOGO; Action::Hide;
        M, ModifiersState::LOGO; Action::Minimize;
        Q, ModifiersState::LOGO; Action::Quit;
        W, ModifiersState::LOGO; Action::Quit;
        F, ModifiersState::LOGO; Action::SearchForward;
        B, ModifiersState::LOGO; Action::SearchBackward;
    )
}

// Don't return any bindings for tests since they are commented-out by default.
#[cfg(test)]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    vec![]
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Key {
    Scancode(u32),
    Keycode(VirtualKeyCode),
}

impl<'a> Deserialize<'a> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        let value = SerdeValue::deserialize(deserializer)?;
        match u32::deserialize(value.clone()) {
            Ok(scancode) => Ok(Key::Scancode(scancode)),
            Err(_) => {
                let keycode = VirtualKeyCode::deserialize(value).map_err(D::Error::custom)?;
                Ok(Key::Keycode(keycode))
            },
        }
    }
}

struct ModeWrapper {
    pub mode: TermMode,
    pub not_mode: TermMode,
}

impl<'a> Deserialize<'a> for ModeWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        struct ModeVisitor;

        impl<'a> Visitor<'a> for ModeVisitor {
            type Value = ModeWrapper;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a combination of AppCursor | AppKeypad | Alt | Vi, possibly with negation (~)",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<ModeWrapper, E>
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
                        "alt" => res.mode |= TermMode::ALT_SCREEN,
                        "~alt" => res.not_mode |= TermMode::ALT_SCREEN,
                        "vi" => res.mode |= TermMode::VI,
                        "~vi" => res.not_mode |= TermMode::VI,
                        _ => return Err(E::invalid_value(Unexpected::Str(modifier), &self)),
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
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        struct MouseButtonVisitor;

        impl<'a> Visitor<'a> for MouseButtonVisitor {
            type Value = MouseButtonWrapper;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Left, Right, Middle, or a number from 0 to 255")
            }

            fn visit_u64<E>(self, value: u64) -> Result<MouseButtonWrapper, E>
            where
                E: de::Error,
            {
                match value {
                    0..=255 => Ok(MouseButtonWrapper(MouseButton::Other(value as u8))),
                    _ => Err(E::invalid_value(Unexpected::Unsigned(value), &self)),
                }
            }

            fn visit_str<E>(self, value: &str) -> Result<MouseButtonWrapper, E>
            where
                E: de::Error,
            {
                match value {
                    "Left" => Ok(MouseButtonWrapper(MouseButton::Left)),
                    "Right" => Ok(MouseButtonWrapper(MouseButton::Right)),
                    "Middle" => Ok(MouseButtonWrapper(MouseButton::Middle)),
                    _ => Err(E::invalid_value(Unexpected::Str(value), &self)),
                }
            }
        }

        deserializer.deserialize_any(MouseButtonVisitor)
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
    fn into_mouse_binding(self) -> Result<MouseBinding, Self> {
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

    fn into_key_binding(self) -> Result<KeyBinding, Self> {
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
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        const FIELDS: &[&str] = &["key", "mods", "mode", "action", "chars", "mouse", "command"];

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
            fn deserialize<D>(deserializer: D) -> Result<Field, D::Error>
            where
                D: Deserializer<'a>,
            {
                struct FieldVisitor;

                impl<'a> Visitor<'a> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("binding fields")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
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

            fn visit_map<V>(self, mut map: V) -> Result<RawBinding, V::Error>
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
                let mut command: Option<Program> = None;

                use de::Error;

                while let Some(struct_key) = map.next_key::<Field>()? {
                    match struct_key {
                        Field::Key => {
                            if key.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("key"));
                            }

                            let val = map.next_value::<SerdeValue>()?;
                            if val.is_u64() {
                                let scancode = val.as_u64().unwrap();
                                if scancode > u64::from(std::u32::MAX) {
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

                            let value = map.next_value::<SerdeValue>()?;

                            action = if let Ok(vi_action) = ViAction::deserialize(value.clone()) {
                                Some(vi_action.into())
                            } else if let Ok(vi_motion) = ViMotion::deserialize(value.clone()) {
                                Some(vi_motion.into())
                            } else {
                                match Action::deserialize(value.clone()).map_err(V::Error::custom) {
                                    Ok(action) => Some(action),
                                    Err(err) => {
                                        let value = match value {
                                            SerdeValue::String(string) => string,
                                            SerdeValue::Mapping(map) if map.len() == 1 => {
                                                match map.into_iter().next() {
                                                    Some((
                                                        SerdeValue::String(string),
                                                        SerdeValue::Null,
                                                    )) => string,
                                                    _ => return Err(err),
                                                }
                                            },
                                            _ => return Err(err),
                                        };
                                        return Err(V::Error::custom(format!(
                                            "unknown keyboard action `{}`",
                                            value
                                        )));
                                    },
                                }
                            };
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

                            command = Some(map.next_value::<Program>()?);
                        },
                    }
                }

                let mode = mode.unwrap_or_else(TermMode::empty);
                let not_mode = not_mode.unwrap_or_else(TermMode::empty);
                let mods = mods.unwrap_or_else(ModifiersState::default);

                let action = match (action, chars, command) {
                    (Some(action @ Action::ViMotion(_)), None, None)
                    | (Some(action @ Action::ViAction(_)), None, None) => {
                        if !mode.intersects(TermMode::VI) || not_mode.intersects(TermMode::VI) {
                            return Err(V::Error::custom(format!(
                                "action `{}` is only available in vi mode, try adding `mode: Vi`",
                                action,
                            )));
                        }
                        action
                    },
                    (Some(action), None, None) => action,
                    (None, Some(chars), None) => Action::Esc(chars),
                    (None, None, Some(cmd)) => Action::Command(cmd),
                    _ => {
                        return Err(V::Error::custom(
                            "must specify exactly one of chars, action or command",
                        ))
                    },
                };

                if mouse.is_none() && key.is_none() {
                    return Err(V::Error::custom("bindings require mouse button or key"));
                }

                Ok(RawBinding { mode, notmode: not_mode, action, key, mouse, mods })
            }
        }

        deserializer.deserialize_struct("RawBinding", FIELDS, RawBindingVisitor)
    }
}

impl<'a> Deserialize<'a> for MouseBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        let raw = RawBinding::deserialize(deserializer)?;
        raw.into_mouse_binding()
            .map_err(|_| D::Error::custom("expected mouse binding, got key binding"))
    }
}

impl<'a> Deserialize<'a> for KeyBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        let raw = RawBinding::deserialize(deserializer)?;
        raw.into_key_binding()
            .map_err(|_| D::Error::custom("expected key binding, got mouse binding"))
    }
}

/// Newtype for implementing deserialize on glutin Mods.
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
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'a>,
    {
        struct ModsVisitor;

        impl<'a> Visitor<'a> for ModsVisitor {
            type Value = ModsWrapper;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a subset of Shift|Control|Super|Command|Alt|Option")
            }

            fn visit_str<E>(self, value: &str) -> Result<ModsWrapper, E>
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
                        _ => return Err(E::invalid_value(Unexpected::Str(modifier), &self)),
                    }
                }

                Ok(ModsWrapper(res))
            }
        }

        deserializer.deserialize_str(ModsVisitor)
    }
}

#[cfg(test)]
mod tests {
    use glutin::event::ModifiersState;

    use alacritty_terminal::term::TermMode;

    use crate::config::{Action, Binding};

    type MockBinding = Binding<usize>;

    impl Default for MockBinding {
        fn default() -> Self {
            Self {
                mods: Default::default(),
                action: Action::None,
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
        assert!(b2.triggers_match(&b1));
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
        assert!(b2.triggers_match(&b1));
    }

    #[test]
    fn binding_matches_modes() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::ALT_SCREEN | TermMode::APP_KEYPAD;
        let mut b2 = MockBinding::default();
        b2.mode = TermMode::APP_KEYPAD;

        assert!(b1.triggers_match(&b2));
        assert!(b2.triggers_match(&b1));
    }

    #[test]
    fn binding_matches_partial_intersection() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::ALT_SCREEN | TermMode::APP_KEYPAD;
        let mut b2 = MockBinding::default();
        b2.mode = TermMode::APP_KEYPAD | TermMode::APP_CURSOR;

        assert!(b1.triggers_match(&b2));
        assert!(b2.triggers_match(&b1));
    }

    #[test]
    fn binding_mismatches_notmode() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::ALT_SCREEN;
        let mut b2 = MockBinding::default();
        b2.notmode = TermMode::ALT_SCREEN;

        assert!(!b1.triggers_match(&b2));
        assert!(!b2.triggers_match(&b1));
    }

    #[test]
    fn binding_mismatches_unrelated() {
        let mut b1 = MockBinding::default();
        b1.mode = TermMode::ALT_SCREEN;
        let mut b2 = MockBinding::default();
        b2.mode = TermMode::APP_KEYPAD;

        assert!(!b1.triggers_match(&b2));
        assert!(!b2.triggers_match(&b1));
    }

    #[test]
    fn binding_matches_notmodes() {
        let mut subset_notmodes = MockBinding::default();
        let mut superset_notmodes = MockBinding::default();
        subset_notmodes.notmode = TermMode::VI | TermMode::APP_CURSOR;
        superset_notmodes.notmode = TermMode::APP_CURSOR;

        assert!(subset_notmodes.triggers_match(&superset_notmodes));
        assert!(superset_notmodes.triggers_match(&subset_notmodes));
    }

    #[test]
    fn binding_matches_mode_notmode() {
        let mut b1 = MockBinding::default();
        let mut b2 = MockBinding::default();
        b1.mode = TermMode::VI;
        b1.notmode = TermMode::APP_CURSOR;
        b2.notmode = TermMode::APP_CURSOR;

        assert!(b1.triggers_match(&b2));
        assert!(b2.triggers_match(&b1));
    }

    #[test]
    fn binding_trigger_input() {
        let mut binding = MockBinding::default();
        binding.trigger = 13;

        let mods = binding.mods;
        let mode = binding.mode;

        assert!(binding.is_triggered_by(mode, mods, &13));
        assert!(!binding.is_triggered_by(mode, mods, &32));
    }

    #[test]
    fn binding_trigger_mods() {
        let mut binding = MockBinding::default();
        binding.mods = ModifiersState::ALT | ModifiersState::LOGO;

        let superset_mods = ModifiersState::all();
        let subset_mods = ModifiersState::empty();

        let t = binding.trigger;
        let mode = binding.mode;

        assert!(binding.is_triggered_by(mode, binding.mods, &t));
        assert!(!binding.is_triggered_by(mode, superset_mods, &t));
        assert!(!binding.is_triggered_by(mode, subset_mods, &t));
    }

    #[test]
    fn binding_trigger_modes() {
        let mut binding = MockBinding::default();
        binding.mode = TermMode::ALT_SCREEN;

        let t = binding.trigger;
        let mods = binding.mods;

        assert!(!binding.is_triggered_by(TermMode::INSERT, mods, &t));
        assert!(binding.is_triggered_by(TermMode::ALT_SCREEN, mods, &t));
        assert!(binding.is_triggered_by(TermMode::ALT_SCREEN | TermMode::INSERT, mods, &t));
    }

    #[test]
    fn binding_trigger_notmodes() {
        let mut binding = MockBinding::default();
        binding.notmode = TermMode::ALT_SCREEN;

        let t = binding.trigger;
        let mods = binding.mods;

        assert!(binding.is_triggered_by(TermMode::INSERT, mods, &t));
        assert!(!binding.is_triggered_by(TermMode::ALT_SCREEN, mods, &t));
        assert!(!binding.is_triggered_by(TermMode::ALT_SCREEN | TermMode::INSERT, mods, &t));
    }
}
