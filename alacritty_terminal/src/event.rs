use std::borrow::Cow;
use std::path::PathBuf;

use crate::message_bar::Message;
use crate::term::SizeInfo;

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    DPRChanged(f64, (u32, u32)),
    ConfigReload(PathBuf),
    MouseCursorDirty,
    Message(Message),
    Title(String),
    Wakeup,
    Urgent,
    Exit,
}

/// Byte sequences are sent to a `Notify` in response to some events
pub trait Notify {
    /// Notify that an escape sequence should be written to the pty
    ///
    /// TODO this needs to be able to error somehow
    fn notify<B: Into<Cow<'static, [u8]>>>(&mut self, _: B);
}

/// Types that are interested in when the display is resized
pub trait OnResize {
    fn on_resize(&mut self, size: &SizeInfo);
}

/// Event Loop for notifying the renderer about terminal events
pub trait EventListener {
    fn send_event(&self, event: Event);
}
