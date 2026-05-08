//! Parse `[[keyboard.bindings]]` entries from alacritty's config and match
//! them against egui input events.
//!
//! Coverage: the `chars` form is fully supported and a small set of named
//! actions is implemented (Paste, ScrollPageUp/Down, ScrollLineUp/Down,
//! ScrollToTop/Bottom, SpawnNewInstance, IncreaseFontSize, DecreaseFontSize,
//! ResetFontSize).  Other named actions and `command` bindings are accepted
//! but logged as unsupported.

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
    /// Literal byte sequence to send to the PTY.
    Chars(Vec<u8>),
    Named(NamedAction),
    /// Action we recognised but don't yet implement; applying it is a no-op
    /// with a warning logged once on first use.
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedAction {
    Paste,
    Copy,
    ScrollPageUp,
    ScrollPageDown,
    ScrollHalfPageUp,
    ScrollHalfPageDown,
    ScrollLineUp,
    ScrollLineDown,
    ScrollToTop,
    ScrollToBottom,
    SpawnNewInstance,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    Quit,
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
            // Mode-conditional bindings (vi/search modes etc.) are out of
            // scope for v1; skip silently rather than misinterpret.
            continue;
        }
        let Some(key) = parse_key(&r.key) else {
            log::warn!("ignoring binding for unknown key: {}", r.key);
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
    out
}

pub fn matches(bindings: &[KeyBinding], key: Key, mods: Modifiers) -> Option<&BindingAction> {
    bindings
        .iter()
        .find(|b| b.key == key && mods_match(b.mods, mods))
        .map(|b| &b.action)
}

/// Modifiers match if the binding's required mods are exactly the same as the
/// pressed mods.  This is alacritty's behaviour: `Control|Shift` doesn't fire
/// on a plain Control-key press, even though Control is a subset.
fn mods_match(required: Modifiers, pressed: Modifiers) -> bool {
    let mask = Modifiers {
        alt: pressed.alt,
        ctrl: pressed.ctrl,
        shift: pressed.shift,
        mac_cmd: pressed.mac_cmd,
        command: pressed.command,
    };
    let req = Modifiers {
        alt: required.alt,
        ctrl: required.ctrl,
        shift: required.shift,
        mac_cmd: required.mac_cmd,
        command: required.command,
    };
    mask == req
}

fn parse_key(name: &str) -> Option<Key> {
    let n = name.trim();
    if n.len() == 1 {
        let c = n.chars().next().unwrap().to_ascii_uppercase();
        return char_to_key(c);
    }
    Some(match n {
        "Return" | "Enter" => Key::Enter,
        "Space" => Key::Space,
        "Tab" => Key::Tab,
        "Backspace" => Key::Backspace,
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
        }
        _ => return None,
    })
}

fn char_to_key(c: char) -> Option<Key> {
    Some(match c {
        'A' => Key::A, 'B' => Key::B, 'C' => Key::C, 'D' => Key::D,
        'E' => Key::E, 'F' => Key::F, 'G' => Key::G, 'H' => Key::H,
        'I' => Key::I, 'J' => Key::J, 'K' => Key::K, 'L' => Key::L,
        'M' => Key::M, 'N' => Key::N, 'O' => Key::O, 'P' => Key::P,
        'Q' => Key::Q, 'R' => Key::R, 'S' => Key::S, 'T' => Key::T,
        'U' => Key::U, 'V' => Key::V, 'W' => Key::W, 'X' => Key::X,
        'Y' => Key::Y, 'Z' => Key::Z,
        '0' => Key::Num0, '1' => Key::Num1, '2' => Key::Num2,
        '3' => Key::Num3, '4' => Key::Num4, '5' => Key::Num5,
        '6' => Key::Num6, '7' => Key::Num7, '8' => Key::Num8,
        '9' => Key::Num9,
        _ => return None,
    })
}

fn f_key(n: u8) -> Option<Key> {
    Some(match n {
        1 => Key::F1, 2 => Key::F2, 3 => Key::F3, 4 => Key::F4,
        5 => Key::F5, 6 => Key::F6, 7 => Key::F7, 8 => Key::F8,
        9 => Key::F9, 10 => Key::F10, 11 => Key::F11, 12 => Key::F12,
        13 => Key::F13, 14 => Key::F14, 15 => Key::F15, 16 => Key::F16,
        17 => Key::F17, 18 => Key::F18, 19 => Key::F19, 20 => Key::F20,
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
        "Copy" => BindingAction::Named(Copy),
        "ScrollPageUp" => BindingAction::Named(ScrollPageUp),
        "ScrollPageDown" => BindingAction::Named(ScrollPageDown),
        "ScrollHalfPageUp" => BindingAction::Named(ScrollHalfPageUp),
        "ScrollHalfPageDown" => BindingAction::Named(ScrollHalfPageDown),
        "ScrollLineUp" => BindingAction::Named(ScrollLineUp),
        "ScrollLineDown" => BindingAction::Named(ScrollLineDown),
        "ScrollToTop" => BindingAction::Named(ScrollToTop),
        "ScrollToBottom" => BindingAction::Named(ScrollToBottom),
        "SpawnNewInstance" | "CreateNewWindow" | "CreateNewTab" => {
            BindingAction::Named(SpawnNewInstance)
        }
        "IncreaseFontSize" => BindingAction::Named(IncreaseFontSize),
        "DecreaseFontSize" => BindingAction::Named(DecreaseFontSize),
        "ResetFontSize" => BindingAction::Named(ResetFontSize),
        "Quit" => BindingAction::Named(Quit),
        other => BindingAction::Unsupported(other.to_string()),
    }
}

/// Decode common escape sequences (\n, \t, \r, \\, \", \xNN, \uNNNN).
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
            }
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Ok(b) = u32::from_str_radix(&hex, 16) {
                    if let Some(c) = char::from_u32(b) {
                        out.push(c);
                    }
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
