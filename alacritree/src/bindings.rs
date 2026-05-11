//! Parse `[[keyboard.bindings]]` from alacritty's config and match them
//! against egui input events.

use egui::{Key, Modifiers};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub key: Key,
    pub mods: Modifiers,
    pub action: BindingAction,
}

#[derive(Debug, Clone)]
pub enum BindingAction {
    Chars(Vec<u8>),
    Named(NamedAction),
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedAction {
    Paste,
    PasteSelection,
    Copy,
    ScrollPageUp,
    ScrollPageDown,
    ScrollHalfPageUp,
    ScrollHalfPageDown,
    ScrollLineUp,
    ScrollLineDown,
    ScrollToTop,
    ScrollToBottom,
    ClearHistory,
    SpawnNewInstance,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    ToggleFullscreen,
    ToggleMaximized,
    Minimize,
    SelectNextTab,
    SelectPreviousTab,
    /// 1-indexed.
    SelectTab(u8),
    SelectLastTab,
    Quit,
    /// Used to unbind a key — consumes the press without acting on it.
    NoOp,
}

#[derive(Debug, Deserialize)]
pub struct RawBinding {
    pub key: String,
    #[serde(default)]
    pub mods: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub chars: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub command: Option<toml::Value>,
}

pub fn parse_bindings(raw: Vec<RawBinding>) -> Vec<KeyBinding> {
    let mut out = Vec::with_capacity(raw.len());
    for r in raw {
        if r.mode.is_some() {
            // vi/search-mode bindings need terminal-mode tracking we don't have.
            continue;
        }
        let Some(key) = parse_key(&r.key) else {
            if !is_silent_unsupported_key(&r.key) {
                log::warn!("ignoring binding for unknown key: {}", r.key);
            }
            continue;
        };
        let mods = r.mods.as_deref().map_or(Modifiers::NONE, parse_mods);
        let action = if let Some(chars) = r.chars {
            BindingAction::Chars(unescape(&chars).into_bytes())
        } else if let Some(action) = r.action {
            parse_action(&action)
        } else if r.command.is_some() {
            BindingAction::Unsupported("command".into())
        } else {
            continue;
        };
        out.push(KeyBinding { key, mods, action });
    }
    // Append alacritty's hardcoded defaults at the end so user-supplied bindings
    // (parsed first) take precedence — `matches` returns the first hit.
    out.extend(default_bindings());
    out
}

/// Alacritty's hardcoded default key bindings.  Alacritty merges these with
/// the user's TOML at runtime; without them, configs that rely on bindings
/// like `Ctrl+Shift+V → Paste` (never written explicitly because they're
/// "always there" in alacritty) silently do nothing.
fn default_bindings() -> Vec<KeyBinding> {
    use NamedAction::*;
    let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
    let ctrl = Modifiers::CTRL;
    let shift = Modifiers::SHIFT;
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;

    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut b = vec![
        KeyBinding { key: Key::V, mods: ctrl_shift, action: BindingAction::Named(Paste) },
        KeyBinding { key: Key::C, mods: ctrl_shift, action: BindingAction::Named(Copy) },
        KeyBinding { key: Key::Insert, mods: shift, action: BindingAction::Named(PasteSelection) },
        KeyBinding { key: Key::Num0, mods: ctrl, action: BindingAction::Named(ResetFontSize) },
        KeyBinding { key: Key::Equals, mods: ctrl, action: BindingAction::Named(IncreaseFontSize) },
        KeyBinding { key: Key::Plus, mods: ctrl, action: BindingAction::Named(IncreaseFontSize) },
        KeyBinding { key: Key::Minus, mods: ctrl, action: BindingAction::Named(DecreaseFontSize) },
        KeyBinding { key: Key::Home, mods: shift, action: BindingAction::Named(ScrollToTop) },
        KeyBinding { key: Key::End, mods: shift, action: BindingAction::Named(ScrollToBottom) },
        KeyBinding { key: Key::PageUp, mods: shift, action: BindingAction::Named(ScrollPageUp) },
        KeyBinding {
            key: Key::PageDown,
            mods: shift,
            action: BindingAction::Named(ScrollPageDown),
        },
        // Alacritty emits CSI Z for Shift+Tab and ESC + CSI Z for Alt+Shift+Tab
        // so apps that handle reverse-tab (readline, vim, etc.) keep working.
        KeyBinding { key: Key::Tab, mods: shift, action: BindingAction::Chars(b"\x1b[Z".to_vec()) },
        KeyBinding {
            key: Key::Tab,
            mods: alt_shift,
            action: BindingAction::Chars(b"\x1b\x1b[Z".to_vec()),
        },
    ];

    // macOS uses Cmd instead of Ctrl+Shift for clipboard / window actions.
    #[cfg(target_os = "macos")]
    {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let cmd_ctrl = Modifiers::COMMAND | Modifiers::CTRL;
        b.extend([
            KeyBinding { key: Key::V, mods: cmd, action: BindingAction::Named(Paste) },
            KeyBinding { key: Key::C, mods: cmd, action: BindingAction::Named(Copy) },
            KeyBinding { key: Key::N, mods: cmd, action: BindingAction::Named(SpawnNewInstance) },
            KeyBinding { key: Key::T, mods: cmd, action: BindingAction::Named(SpawnNewInstance) },
            KeyBinding { key: Key::Num0, mods: cmd, action: BindingAction::Named(ResetFontSize) },
            KeyBinding {
                key: Key::Equals,
                mods: cmd,
                action: BindingAction::Named(IncreaseFontSize),
            },
            KeyBinding {
                key: Key::Plus,
                mods: cmd,
                action: BindingAction::Named(IncreaseFontSize),
            },
            KeyBinding {
                key: Key::Minus,
                mods: cmd,
                action: BindingAction::Named(DecreaseFontSize),
            },
            KeyBinding {
                key: Key::CloseBracket,
                mods: cmd_shift,
                action: BindingAction::Named(SelectNextTab),
            },
            KeyBinding {
                key: Key::OpenBracket,
                mods: cmd_shift,
                action: BindingAction::Named(SelectPreviousTab),
            },
            KeyBinding { key: Key::Num1, mods: cmd, action: BindingAction::Named(SelectTab(1)) },
            KeyBinding { key: Key::Num2, mods: cmd, action: BindingAction::Named(SelectTab(2)) },
            KeyBinding { key: Key::Num3, mods: cmd, action: BindingAction::Named(SelectTab(3)) },
            KeyBinding { key: Key::Num4, mods: cmd, action: BindingAction::Named(SelectTab(4)) },
            KeyBinding { key: Key::Num5, mods: cmd, action: BindingAction::Named(SelectTab(5)) },
            KeyBinding { key: Key::Num6, mods: cmd, action: BindingAction::Named(SelectTab(6)) },
            KeyBinding { key: Key::Num7, mods: cmd, action: BindingAction::Named(SelectTab(7)) },
            KeyBinding { key: Key::Num8, mods: cmd, action: BindingAction::Named(SelectTab(8)) },
            KeyBinding { key: Key::Num9, mods: cmd, action: BindingAction::Named(SelectLastTab) },
            KeyBinding {
                key: Key::F,
                mods: cmd_ctrl,
                action: BindingAction::Named(ToggleFullscreen),
            },
            KeyBinding { key: Key::M, mods: cmd, action: BindingAction::Named(Minimize) },
            KeyBinding { key: Key::K, mods: cmd, action: BindingAction::Named(ClearHistory) },
            KeyBinding { key: Key::Q, mods: cmd, action: BindingAction::Named(Quit) },
        ]);
    }

    b
}

pub fn matches(bindings: &[KeyBinding], key: Key, mods: Modifiers) -> Option<&BindingAction> {
    bindings.iter().find(|b| b.key == key && mods_match(b.mods, mods)).map(|b| &b.action)
}

/// Alacritty semantics: `Control|Shift` does not fire on Ctrl alone even though
/// the modifier sets overlap.  Use egui's `matches_exact`, which requires
/// alt/shift to match the pattern exactly while doing the platform-aware
/// ctrl/cmd dance — egui-winit on Linux populates both `ctrl` and `command` on
/// every Ctrl press, so a naive field-by-field eq would never match.
fn mods_match(required: Modifiers, pressed: Modifiers) -> bool {
    pressed.matches_exact(required)
}

fn parse_key(name: &str) -> Option<Key> {
    let n = name.trim();
    if n.len() == 1 {
        let c = n.chars().next().unwrap().to_ascii_uppercase();
        return char_to_key(c);
    }
    if n == "NumpadEnter" {
        // egui-winit maps both `KeyCode::Enter` and `KeyCode::NumpadEnter` to
        // `egui::Key::Enter`, so we can't tell them apart.  Aliasing NumpadEnter
        // to Enter would silently fire NumpadEnter bindings on the regular
        // Return key — drop the binding instead.
        log::warn!("ignoring NumpadEnter binding: egui cannot distinguish it from Return");
        return None;
    }
    Some(match n {
        "Return" | "Enter" => Key::Enter,
        "Space" => Key::Space,
        "Tab" => Key::Tab,
        "Backspace" | "Back" => Key::Backspace,
        "Escape" | "Esc" => Key::Escape,
        "Insert" => Key::Insert,
        "Delete" => Key::Delete,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Up" => Key::ArrowUp,
        "Down" => Key::ArrowDown,
        "Left" => Key::ArrowLeft,
        "Right" => Key::ArrowRight,
        "Minus" => Key::Minus,
        "Equals" | "Equal" => Key::Equals,
        "Plus" => Key::Plus,
        "Comma" => Key::Comma,
        "Period" => Key::Period,
        "Slash" => Key::Slash,
        "Backslash" => Key::Backslash,
        "Semicolon" => Key::Semicolon,
        "Apostrophe" | "Quote" => Key::Quote,
        "LBracket" | "LeftBracket" => Key::OpenBracket,
        "RBracket" | "RightBracket" => Key::CloseBracket,
        "Grave" | "Backtick" => Key::Backtick,
        // F1..F35.
        n if n.starts_with('F') => {
            let num: u8 = n[1..].parse().ok()?;
            return f_key(num);
        },
        _ => return None,
    })
}

fn char_to_key(c: char) -> Option<Key> {
    Some(match c {
        'A' => Key::A,
        'B' => Key::B,
        'C' => Key::C,
        'D' => Key::D,
        'E' => Key::E,
        'F' => Key::F,
        'G' => Key::G,
        'H' => Key::H,
        'I' => Key::I,
        'J' => Key::J,
        'K' => Key::K,
        'L' => Key::L,
        'M' => Key::M,
        'N' => Key::N,
        'O' => Key::O,
        'P' => Key::P,
        'Q' => Key::Q,
        'R' => Key::R,
        'S' => Key::S,
        'T' => Key::T,
        'U' => Key::U,
        'V' => Key::V,
        'W' => Key::W,
        'X' => Key::X,
        'Y' => Key::Y,
        'Z' => Key::Z,
        '0' => Key::Num0,
        '1' => Key::Num1,
        '2' => Key::Num2,
        '3' => Key::Num3,
        '4' => Key::Num4,
        '5' => Key::Num5,
        '6' => Key::Num6,
        '7' => Key::Num7,
        '8' => Key::Num8,
        '9' => Key::Num9,
        _ => return None,
    })
}

/// Winit key names that egui doesn't model.  Default alacritty configs include
/// a handful of these, so swallow them silently rather than logging noise.
fn is_silent_unsupported_key(name: &str) -> bool {
    matches!(
        name.trim(),
        "Paste"
            | "Copy"
            | "Cut"
            | "Find"
            | "Help"
            | "Undo"
            | "BrowserBack"
            | "BrowserForward"
            | "BrowserRefresh"
            | "BrowserStop"
            | "BrowserHome"
            | "BrowserSearch"
            | "BrowserFavorites"
            | "MediaPlayPause"
            | "MediaStop"
            | "MediaTrackNext"
            | "MediaTrackPrevious"
            | "VolumeUp"
            | "VolumeDown"
            | "VolumeMute"
            // `parse_key` already logs a dedicated message explaining why
            // NumpadEnter is dropped; suppress the generic "unknown key" follow-up.
            | "NumpadEnter"
    )
}

fn f_key(n: u8) -> Option<Key> {
    Some(match n {
        1 => Key::F1,
        2 => Key::F2,
        3 => Key::F3,
        4 => Key::F4,
        5 => Key::F5,
        6 => Key::F6,
        7 => Key::F7,
        8 => Key::F8,
        9 => Key::F9,
        10 => Key::F10,
        11 => Key::F11,
        12 => Key::F12,
        13 => Key::F13,
        14 => Key::F14,
        15 => Key::F15,
        16 => Key::F16,
        17 => Key::F17,
        18 => Key::F18,
        19 => Key::F19,
        20 => Key::F20,
        _ => return None,
    })
}

fn parse_mods(s: &str) -> Modifiers {
    let mut m = Modifiers::NONE;
    for token in s.split('|') {
        match token.trim() {
            "Control" | "Ctrl" => m.ctrl = true,
            "Shift" => m.shift = true,
            "Alt" | "Option" => m.alt = true,
            "Super" | "Command" | "Meta" => m.command = true,
            other => log::warn!("unknown modifier '{other}'"),
        }
    }
    m
}

fn parse_action(name: &str) -> BindingAction {
    use NamedAction::*;
    match name {
        "Paste" => BindingAction::Named(Paste),
        "PasteSelection" => BindingAction::Named(PasteSelection),
        "Copy" => BindingAction::Named(Copy),
        "ScrollPageUp" => BindingAction::Named(ScrollPageUp),
        "ScrollPageDown" => BindingAction::Named(ScrollPageDown),
        "ScrollHalfPageUp" => BindingAction::Named(ScrollHalfPageUp),
        "ScrollHalfPageDown" => BindingAction::Named(ScrollHalfPageDown),
        "ScrollLineUp" => BindingAction::Named(ScrollLineUp),
        "ScrollLineDown" => BindingAction::Named(ScrollLineDown),
        "ScrollToTop" => BindingAction::Named(ScrollToTop),
        "ScrollToBottom" => BindingAction::Named(ScrollToBottom),
        "ClearHistory" => BindingAction::Named(ClearHistory),
        "SpawnNewInstance" | "CreateNewWindow" | "CreateNewTab" => {
            BindingAction::Named(SpawnNewInstance)
        },
        "IncreaseFontSize" => BindingAction::Named(IncreaseFontSize),
        "DecreaseFontSize" => BindingAction::Named(DecreaseFontSize),
        "ResetFontSize" => BindingAction::Named(ResetFontSize),
        "ToggleFullscreen" => BindingAction::Named(ToggleFullscreen),
        "ToggleMaximized" => BindingAction::Named(ToggleMaximized),
        "Minimize" => BindingAction::Named(Minimize),
        "SelectNextTab" => BindingAction::Named(SelectNextTab),
        "SelectPreviousTab" => BindingAction::Named(SelectPreviousTab),
        "SelectTab1" => BindingAction::Named(SelectTab(1)),
        "SelectTab2" => BindingAction::Named(SelectTab(2)),
        "SelectTab3" => BindingAction::Named(SelectTab(3)),
        "SelectTab4" => BindingAction::Named(SelectTab(4)),
        "SelectTab5" => BindingAction::Named(SelectTab(5)),
        "SelectTab6" => BindingAction::Named(SelectTab(6)),
        "SelectTab7" => BindingAction::Named(SelectTab(7)),
        "SelectTab8" => BindingAction::Named(SelectTab(8)),
        "SelectTab9" => BindingAction::Named(SelectTab(9)),
        "SelectLastTab" => BindingAction::Named(SelectLastTab),
        "Quit" => BindingAction::Named(Quit),
        "None" => BindingAction::Named(NoOp),
        other => BindingAction::Unsupported(other.to_string()),
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('e') => out.push('\u{1b}'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                if let Ok(b) = u8::from_str_radix(&hex, 16) {
                    out.push(b as char);
                }
            },
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Ok(b) = u32::from_str_radix(&hex, 16) {
                    if let Some(c) = char::from_u32(b) {
                        out.push(c);
                    }
                }
            },
            Some(other) => {
                out.push('\\');
                out.push(other);
            },
            None => out.push('\\'),
        }
    }
    out
}
