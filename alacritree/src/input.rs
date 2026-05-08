use egui::{Event, Key, Modifiers};

/// Translate an egui input event into bytes to write to the PTY.
///
/// Returns `None` when the event isn't something the terminal cares about.
pub fn event_to_bytes(event: &Event) -> Option<Vec<u8>> {
    match event {
        // Plain typed text — this already accounts for Shift on letters and
        // dead-key composition done by the OS.  Skip it when Ctrl/Alt are held;
        // those are handled below as escape/control sequences.
        Event::Text(text) if !text.is_empty() => Some(text.as_bytes().to_vec()),
        Event::Key { key, pressed: true, modifiers, repeat: _, .. } => key_to_bytes(*key, *modifiers),
        Event::Paste(s) => Some(s.as_bytes().to_vec()),
        _ => None,
    }
}

fn key_to_bytes(key: Key, mods: Modifiers) -> Option<Vec<u8>> {
    if mods.ctrl && !mods.alt {
        if let Some(b) = ctrl_byte(key) {
            return Some(vec![b]);
        }
    }

    let bytes: &[u8] = match key {
        Key::Enter => b"\r",
        Key::Tab => b"\t",
        Key::Backspace => b"\x7f",
        Key::Escape => b"\x1b",
        Key::ArrowUp => b"\x1b[A",
        Key::ArrowDown => b"\x1b[B",
        Key::ArrowRight => b"\x1b[C",
        Key::ArrowLeft => b"\x1b[D",
        Key::Home => b"\x1b[H",
        Key::End => b"\x1b[F",
        Key::PageUp => b"\x1b[5~",
        Key::PageDown => b"\x1b[6~",
        Key::Insert => b"\x1b[2~",
        Key::Delete => b"\x1b[3~",
        Key::F1 => b"\x1bOP",
        Key::F2 => b"\x1bOQ",
        Key::F3 => b"\x1bOR",
        Key::F4 => b"\x1bOS",
        Key::F5 => b"\x1b[15~",
        Key::F6 => b"\x1b[17~",
        Key::F7 => b"\x1b[18~",
        Key::F8 => b"\x1b[19~",
        Key::F9 => b"\x1b[20~",
        Key::F10 => b"\x1b[21~",
        Key::F11 => b"\x1b[23~",
        Key::F12 => b"\x1b[24~",
        _ => return None,
    };

    if mods.alt {
        // Alt+key — prefix with ESC, the long-standing meta-key convention.
        let mut out = Vec::with_capacity(bytes.len() + 1);
        out.push(0x1b);
        out.extend_from_slice(bytes);
        Some(out)
    } else {
        Some(bytes.to_vec())
    }
}

fn ctrl_byte(key: Key) -> Option<u8> {
    // Map Ctrl+letter / Ctrl+symbol to the canonical control byte.
    let b = match key {
        Key::A => 0x01, Key::B => 0x02, Key::C => 0x03, Key::D => 0x04,
        Key::E => 0x05, Key::F => 0x06, Key::G => 0x07, Key::H => 0x08,
        Key::I => 0x09, Key::J => 0x0a, Key::K => 0x0b, Key::L => 0x0c,
        Key::M => 0x0d, Key::N => 0x0e, Key::O => 0x0f, Key::P => 0x10,
        Key::Q => 0x11, Key::R => 0x12, Key::S => 0x13, Key::T => 0x14,
        Key::U => 0x15, Key::V => 0x16, Key::W => 0x17, Key::X => 0x18,
        Key::Y => 0x19, Key::Z => 0x1a,
        Key::OpenBracket => 0x1b,
        Key::Backslash => 0x1c,
        Key::CloseBracket => 0x1d,
        Key::Space => 0x00,
        _ => return None,
    };
    Some(b)
}
