// Copyright 2020 Christian Duerr, The Alacritty Project Contributors
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

use std::borrow::Cow;
use std::path::PathBuf;

use crate::clipboard::ClipboardType;
use crate::message_bar::Message;
use crate::term::SizeInfo;

#[derive(Clone)]
pub enum Event {
    DPRChanged(f64, (u32, u32)),
    ConfigReload(PathBuf),
    MouseCursorDirty,
    Message(Message),
    Title(String),
    ClipboardStore(ClipboardType, String),
    ClipboardLoad(ClipboardType, std::sync::Arc<dyn Fn(&str) -> String + Sync + Send + 'static>),
    Wakeup,
    Urgent,
    Exit,
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::DPRChanged(scale, size) => write!(f, "DPRChanged({}, {:?})", scale, size),
            Event::ConfigReload(path) => write!(f, "ConfigReload({:?})", path),
            Event::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            Event::Message(msg) => write!(f, "Message({:?})", msg),
            Event::Title(title) => write!(f, "Title({})", title),
            Event::ClipboardStore(ty, text) => write!(f, "ClipboardStore({:?},{})", ty, text),
            Event::ClipboardLoad(ty, _) => write!(f, "ClipboardLoad({:?})", ty),
            Event::Wakeup => write!(f, "Wakeup"),
            Event::Urgent => write!(f, "Urgent"),
            Event::Exit => write!(f, "Exit"),
        }
    }
}

/// Byte sequences are sent to a `Notify` in response to some events.
pub trait Notify {
    /// Notify that an escape sequence should be written to the PTY.
    ///
    /// TODO this needs to be able to error somehow.
    fn notify<B: Into<Cow<'static, [u8]>>>(&mut self, _: B);
}

/// Types that are interested in when the display is resized.
pub trait OnResize {
    fn on_resize(&mut self, size: &SizeInfo);
}

/// Event Loop for notifying the renderer about terminal events.
pub trait EventListener {
    fn send_event(&self, event: Event);
}
