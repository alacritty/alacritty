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
use glutin::{MouseButton, ModifiersState};

use crate::input::{MouseBinding, KeyBinding, Action};
use crate::term::TermMode;
use super::Key;

macro_rules! bindings {
    (
        $ty:ident;
        $(
            $key:path
            $(,[$($mod:ident: $enabled:expr),*])*
            $(,+$mode:expr)*
            $(,~$notmode:expr)*
            ;$action:expr
        );*
        $(;)*
    ) => {{
        let mut v = Vec::new();

        $(
            let mut _mods = ModifiersState {
                $($($mod: $enabled),*,)*
                ..Default::default()
            };
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
        Key::Copy; Action::Copy;
        Key::L, [ctrl: true]; Action::ClearLogNotice;
        Key::L, [ctrl: true]; Action::Esc("\x0c".into());
        Key::Home, [alt: true]; Action::Esc("\x1b[1;3H".into());
        Key::Home, +TermMode::APP_CURSOR; Action::Esc("\x1bOH".into());
        Key::Home, ~TermMode::APP_CURSOR; Action::Esc("\x1b[H".into());
        Key::End, [alt: true]; Action::Esc("\x1b[1;3F".into());
        Key::End, +TermMode::APP_CURSOR; Action::Esc("\x1bOF".into());
        Key::End, ~TermMode::APP_CURSOR; Action::Esc("\x1b[F".into());
        Key::PageUp, [shift: true], ~TermMode::ALT_SCREEN; Action::ScrollPageUp;
        Key::PageUp, [shift: true], +TermMode::ALT_SCREEN; Action::Esc("\x1b[5;2~".into());
        Key::PageUp, [ctrl: true]; Action::Esc("\x1b[5;5~".into());
        Key::PageUp, [alt: true]; Action::Esc("\x1b[5;3~".into());
        Key::PageUp; Action::Esc("\x1b[5~".into());
        Key::PageDown, [shift: true], ~TermMode::ALT_SCREEN; Action::ScrollPageDown;
        Key::PageDown, [shift: true], +TermMode::ALT_SCREEN; Action::Esc("\x1b[6;2~".into());
        Key::PageDown, [ctrl: true]; Action::Esc("\x1b[6;5~".into());
        Key::PageDown, [alt: true]; Action::Esc("\x1b[6;3~".into());
        Key::PageDown; Action::Esc("\x1b[6~".into());
        Key::Tab, [shift: true]; Action::Esc("\x1b[Z".into());
        Key::Back; Action::Esc("\x7f".into());
        Key::Back, [alt: true]; Action::Esc("\x1b\x7f".into());
        Key::Insert; Action::Esc("\x1b[2~".into());
        Key::Delete; Action::Esc("\x1b[3~".into());
        Key::Left, [shift: true]; Action::Esc("\x1b[1;2D".into());
        Key::Left, [ctrl: true]; Action::Esc("\x1b[1;5D".into());
        Key::Left, [alt: true]; Action::Esc("\x1b[1;3D".into());
        Key::Left, ~TermMode::APP_CURSOR; Action::Esc("\x1b[D".into());
        Key::Left, +TermMode::APP_CURSOR; Action::Esc("\x1bOD".into());
        Key::Right, [shift: true]; Action::Esc("\x1b[1;2C".into());
        Key::Right, [ctrl: true]; Action::Esc("\x1b[1;5C".into());
        Key::Right, [alt: true]; Action::Esc("\x1b[1;3C".into());
        Key::Right, ~TermMode::APP_CURSOR; Action::Esc("\x1b[C".into());
        Key::Right, +TermMode::APP_CURSOR; Action::Esc("\x1bOC".into());
        Key::Up, [shift: true]; Action::Esc("\x1b[1;2A".into());
        Key::Up, [ctrl: true]; Action::Esc("\x1b[1;5A".into());
        Key::Up, [alt: true]; Action::Esc("\x1b[1;3A".into());
        Key::Up, ~TermMode::APP_CURSOR; Action::Esc("\x1b[A".into());
        Key::Up, +TermMode::APP_CURSOR; Action::Esc("\x1bOA".into());
        Key::Down, [shift: true]; Action::Esc("\x1b[1;2B".into());
        Key::Down, [ctrl: true]; Action::Esc("\x1b[1;5B".into());
        Key::Down, [alt: true]; Action::Esc("\x1b[1;3B".into());
        Key::Down, ~TermMode::APP_CURSOR; Action::Esc("\x1b[B".into());
        Key::Down, +TermMode::APP_CURSOR; Action::Esc("\x1bOB".into());
        Key::F1; Action::Esc("\x1bOP".into());
        Key::F2; Action::Esc("\x1bOQ".into());
        Key::F3; Action::Esc("\x1bOR".into());
        Key::F4; Action::Esc("\x1bOS".into());
        Key::F5; Action::Esc("\x1b[15~".into());
        Key::F6; Action::Esc("\x1b[17~".into());
        Key::F7; Action::Esc("\x1b[18~".into());
        Key::F8; Action::Esc("\x1b[19~".into());
        Key::F9; Action::Esc("\x1b[20~".into());
        Key::F10; Action::Esc("\x1b[21~".into());
        Key::F11; Action::Esc("\x1b[23~".into());
        Key::F12; Action::Esc("\x1b[24~".into());
        Key::F1, [shift: true]; Action::Esc("\x1b[1;2P".into());
        Key::F2, [shift: true]; Action::Esc("\x1b[1;2Q".into());
        Key::F3, [shift: true]; Action::Esc("\x1b[1;2R".into());
        Key::F4, [shift: true]; Action::Esc("\x1b[1;2S".into());
        Key::F5, [shift: true]; Action::Esc("\x1b[15;2~".into());
        Key::F6, [shift: true]; Action::Esc("\x1b[17;2~".into());
        Key::F7, [shift: true]; Action::Esc("\x1b[18;2~".into());
        Key::F8, [shift: true]; Action::Esc("\x1b[19;2~".into());
        Key::F9, [shift: true]; Action::Esc("\x1b[20;2~".into());
        Key::F10, [shift: true]; Action::Esc("\x1b[21;2~".into());
        Key::F11, [shift: true]; Action::Esc("\x1b[23;2~".into());
        Key::F12, [shift: true]; Action::Esc("\x1b[24;2~".into());
        Key::F1, [ctrl: true]; Action::Esc("\x1b[1;5P".into());
        Key::F2, [ctrl: true]; Action::Esc("\x1b[1;5Q".into());
        Key::F3, [ctrl: true]; Action::Esc("\x1b[1;5R".into());
        Key::F4, [ctrl: true]; Action::Esc("\x1b[1;5S".into());
        Key::F5, [ctrl: true]; Action::Esc("\x1b[15;5~".into());
        Key::F6, [ctrl: true]; Action::Esc("\x1b[17;5~".into());
        Key::F7, [ctrl: true]; Action::Esc("\x1b[18;5~".into());
        Key::F8, [ctrl: true]; Action::Esc("\x1b[19;5~".into());
        Key::F9, [ctrl: true]; Action::Esc("\x1b[20;5~".into());
        Key::F10, [ctrl: true]; Action::Esc("\x1b[21;5~".into());
        Key::F11, [ctrl: true]; Action::Esc("\x1b[23;5~".into());
        Key::F12, [ctrl: true]; Action::Esc("\x1b[24;5~".into());
        Key::F1, [alt: true]; Action::Esc("\x1b[1;6P".into());
        Key::F2, [alt: true]; Action::Esc("\x1b[1;6Q".into());
        Key::F3, [alt: true]; Action::Esc("\x1b[1;6R".into());
        Key::F4, [alt: true]; Action::Esc("\x1b[1;6S".into());
        Key::F5, [alt: true]; Action::Esc("\x1b[15;6~".into());
        Key::F6, [alt: true]; Action::Esc("\x1b[17;6~".into());
        Key::F7, [alt: true]; Action::Esc("\x1b[18;6~".into());
        Key::F8, [alt: true]; Action::Esc("\x1b[19;6~".into());
        Key::F9, [alt: true]; Action::Esc("\x1b[20;6~".into());
        Key::F10, [alt: true]; Action::Esc("\x1b[21;6~".into());
        Key::F11, [alt: true]; Action::Esc("\x1b[23;6~".into());
        Key::F12, [alt: true]; Action::Esc("\x1b[24;6~".into());
        Key::F1, [logo: true]; Action::Esc("\x1b[1;3P".into());
        Key::F2, [logo: true]; Action::Esc("\x1b[1;3Q".into());
        Key::F3, [logo: true]; Action::Esc("\x1b[1;3R".into());
        Key::F4, [logo: true]; Action::Esc("\x1b[1;3S".into());
        Key::F5, [logo: true]; Action::Esc("\x1b[15;3~".into());
        Key::F6, [logo: true]; Action::Esc("\x1b[17;3~".into());
        Key::F7, [logo: true]; Action::Esc("\x1b[18;3~".into());
        Key::F8, [logo: true]; Action::Esc("\x1b[19;3~".into());
        Key::F9, [logo: true]; Action::Esc("\x1b[20;3~".into());
        Key::F10, [logo: true]; Action::Esc("\x1b[21;3~".into());
        Key::F11, [logo: true]; Action::Esc("\x1b[23;3~".into());
        Key::F12, [logo: true]; Action::Esc("\x1b[24;3~".into());
        Key::NumpadEnter; Action::Esc("\n".into());
    );

    bindings.extend(platform_key_bindings());

    bindings
}

#[cfg(not(any(target_os = "macos", test)))]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    bindings!(
        KeyBinding;
        Key::V, [ctrl: true, shift: true]; Action::Paste;
        Key::C, [ctrl: true, shift: true]; Action::Copy;
        Key::Insert, [shift: true]; Action::PasteSelection;
        Key::Key0, [ctrl: true]; Action::ResetFontSize;
        Key::Equals, [ctrl: true]; Action::IncreaseFontSize;
        Key::Add, [ctrl: true]; Action::IncreaseFontSize;
        Key::Subtract, [ctrl: true]; Action::DecreaseFontSize;
    )
}

#[cfg(all(target_os = "macos", not(test)))]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    bindings!(
        KeyBinding;
        Key::Key0, [logo: true]; Action::ResetFontSize;
        Key::Equals, [logo: true]; Action::IncreaseFontSize;
        Key::Add, [logo: true]; Action::IncreaseFontSize;
        Key::Minus, [logo: true]; Action::DecreaseFontSize;
        Key::K, [logo: true]; Action::ClearHistory;
        Key::K, [logo: true]; Action::Esc("\x0c".into());
        Key::V, [logo: true]; Action::Paste;
        Key::C, [logo: true]; Action::Copy;
        Key::H, [logo: true]; Action::Hide;
        Key::Q, [logo: true]; Action::Quit;
        Key::W, [logo: true]; Action::Quit;
    )
}

// Don't return any bindings for tests since they are commented-out by default
#[cfg(test)]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    vec![]
}
