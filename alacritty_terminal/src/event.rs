use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

use crate::term::color::Rgb;
use crate::term::{ClipboardType, SizeInfo};

/// Terminal event.
///
/// These events instruct the UI over changes that can't be handled by the terminal emulation layer
/// itself.
#[derive(Clone)]
pub enum Event {
    /// Grid has changed possibly requiring a mouse cursor shape change.
    MouseCursorDirty,

    /// Window title change.
    Title(String),

    /// Reset to the default window title.
    ResetTitle,

    /// Request to store a text string in the clipboard.
    ClipboardStore(ClipboardType, String),

    /// Request to write the contents of the clipboard to the PTY.
    ///
    /// The attached function is a formatter which will corectly transform the clipboard content
    /// into the expected escape sequence format.
    ClipboardLoad(ClipboardType, Arc<dyn Fn(&str) -> String + Sync + Send + 'static>),

    /// Request to write the RGB value of a color to the PTY.
    ///
    /// The attached function is a formatter which will corectly transform the RGB color into the
    /// expected escape sequence format.
    ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send + 'static>),

    /// Write some text to the PTY.
    PtyWrite(String),

    /// Cursor blinking state has changed.
    CursorBlinkingChange,

    /// New terminal content available.
    Wakeup,

    /// Terminal bell ring.
    Bell,

    /// Shutdown request.
    Exit,
}

impl Debug for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Event::ClipboardStore(ty, text) => write!(f, "ClipboardStore({:?}, {})", ty, text),
            Event::ClipboardLoad(ty, _) => write!(f, "ClipboardLoad({:?})", ty),
            Event::ColorRequest(index, _) => write!(f, "ColorRequest({})", index),
            Event::PtyWrite(text) => write!(f, "PtyWrite({})", text),
            Event::Title(title) => write!(f, "Title({})", title),
            Event::CursorBlinkingChange => write!(f, "CursorBlinkingChange"),
            Event::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            Event::ResetTitle => write!(f, "ResetTitle"),
            Event::Wakeup => write!(f, "Wakeup"),
            Event::Bell => write!(f, "Bell"),
            Event::Exit => write!(f, "Exit"),
        }
    }
}

/// Byte sequences are sent to a `Notify` in response to some events.
pub trait Notify {
    /// Notify that an escape sequence should be written to the PTY.
    ///
    /// TODO this needs to be able to error somehow.
    fn notify<B: Into<Cow<'static, [u8]>>>(&self, _: B);
}

/// Types that are interested in when the display is resized.
pub trait OnResize {
    fn on_resize(&mut self, size: &SizeInfo);
}

/// Event Loop for notifying the renderer about terminal events.
pub trait EventListener {
    fn send_event(&self, _event: Event) {}
}

/// Placeholder implementation for tests.
#[cfg(test)]
impl EventListener for () {}
