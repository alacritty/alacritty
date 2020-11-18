use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

use crate::term::{ClipboardType, SizeInfo};

#[derive(Clone)]
pub enum Event {
    MouseCursorDirty,
    Title(String),
    ResetTitle,
    ClipboardStore(ClipboardType, String),
    ClipboardLoad(ClipboardType, Arc<dyn Fn(&str) -> String + Sync + Send + 'static>),
    CursorBlinkingChange(bool),
    Wakeup,
    Bell,
    Exit,
}

impl Debug for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Event::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            Event::Title(title) => write!(f, "Title({})", title),
            Event::ResetTitle => write!(f, "ResetTitle"),
            Event::ClipboardStore(ty, text) => write!(f, "ClipboardStore({:?}, {})", ty, text),
            Event::ClipboardLoad(ty, _) => write!(f, "ClipboardLoad({:?})", ty),
            Event::Wakeup => write!(f, "Wakeup"),
            Event::Bell => write!(f, "Bell"),
            Event::Exit => write!(f, "Exit"),
            Event::CursorBlinkingChange(blinking) => write!(f, "CursorBlinking({})", blinking),
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
