use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::path::PathBuf;
use std::sync::Arc;

use crate::message_bar::Message;
use crate::term::{ClipboardType, SizeInfo};

#[derive(Clone)]
pub enum Event {
    DPRChanged(f64, (u32, u32)),
    ConfigReload(PathBuf),
    MouseCursorDirty,
    Message(Message),
    Title(String),
    ClipboardStore(ClipboardType, String),
    ClipboardLoad(ClipboardType, Arc<dyn Fn(&str) -> String + Sync + Send + 'static>),
    Wakeup,
    Urgent,
    Exit,
}

impl Debug for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Event::DPRChanged(scale, size) => write!(f, "DPRChanged({}, {:?})", scale, size),
            Event::ConfigReload(path) => write!(f, "ConfigReload({:?})", path),
            Event::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            Event::Message(msg) => write!(f, "Message({:?})", msg),
            Event::Title(title) => write!(f, "Title({})", title),
            Event::ClipboardStore(ty, text) => write!(f, "ClipboardStore({:?}, {})", ty, text),
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
