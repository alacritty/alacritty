//! X11 Clipboard implementation
use std::{ fmt, error };
use xcb::{ Connection, Window, Atom };
use xcb_util::icccm;

use super::{ Load };


pub struct Clipboard {
    conn: Connection,
    window: Window,
}

#[derive(Debug)]
pub enum Error {
    X11Conn(::xcb::ConnError),
    Generic(::xcb::GenericError),
    CaptureEventFail
}

impl From<::xcb::ConnError> for Error {
    fn from(err: ::xcb::ConnError) -> Error {
        Error::X11Conn(err)
    }
}

impl From<::xcb::GenericError> for Error {
    fn from(err: ::xcb::GenericError) -> Error {
        Error::Generic(err)
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::X11Conn(..) => "X11 Connection Error",
            Error::Generic(..) => "X11 Generic Error",
            Error::CaptureEventFail => "X11 Event Capture Fail"
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::X11Conn(ref err) =>
                write!(f, "{}: {:?}", error::Error::description(self), err),
            Error::Generic(ref err) =>
                write!(f, "{}: {}", error::Error::description(self), err.error_code()),
            Error::CaptureEventFail =>
                write!(f, "{}", error::Error::description(self))
        }
    }
}

impl Clipboard {
    fn load(&self, selection: Atom, target: Atom, property: Atom) -> Result<String, Error> {
        ::xcb::convert_selection(
            &self.conn, self.window,
            selection, target, property,
            ::xcb::CURRENT_TIME
        );
        self.conn.flush();

        while let Some(event) = self.conn.wait_for_event() {
            let event = ::xcb::cast_event::<::xcb::PropertyNotifyEvent>(&event);
            if event.atom() == property {
                if let Ok(reply) = icccm::get_text_property(&self.conn, self.window, property).get_reply() {
                    return Ok(reply.name().to_string());
                }
            }
        }

        Err(Error::CaptureEventFail)
    }
}

impl Load for Clipboard {
    type Err = Error;

    fn new() -> Result<Self, Self::Err> {
        let (conn, id) = Connection::connect(None)?;
        let window = conn.generate_id();

        {
            let screen = conn.get_setup().roots().nth(id as usize)
                .ok_or(::xcb::ConnError::ClosedInvalidScreen)?;
            ::xcb::create_window(
                &conn,
                ::xcb::COPY_FROM_PARENT as u8,
                window,
                screen.root(),
                0, 0, 1, 1,
                0,
                ::xcb::WINDOW_CLASS_INPUT_OUTPUT as u16,
                screen.root_visual(),
                &[(::xcb::CW_EVENT_MASK, ::xcb::EVENT_MASK_PROPERTY_CHANGE)]
            );
            conn.flush();
        }

        Ok(Clipboard {
            conn: conn,
            window: window,
        })
    }

    fn load_primary(&self) -> Result<String, Self::Err> {
        self.load(
            ::xcb::intern_atom(&self.conn, false, "CLIPBOARD").get_reply()?.atom(),
            ::xcb::intern_atom(&self.conn, false, "UTF8_STRING").get_reply()?.atom(),
            ::xcb::intern_atom(&self.conn, false, "XSEL_DATA").get_reply()?.atom()
        )
    }

    fn load_selection(&self) -> Result<String, Self::Err> {
        self.load(
            ::xcb::ATOM_PRIMARY,
            ::xcb::intern_atom(&self.conn, false, "UTF8_STRING").get_reply()?.atom(),
            ::xcb::intern_atom(&self.conn, false, "XSEL_DATA").get_reply()?.atom()
        )
    }
}


#[cfg(test)]
mod tests {
    use super::Clipboard;
    use ::Load;

    #[test]
    fn clipboard_works() {
        let clipboard = Clipboard::new().expect("create clipboard");
        assert_eq!(
            clipboard.load_primary().expect("load selection"),
            clipboard.load_primary().expect("load selection")
        );
        assert_eq!(
            clipboard.load_selection().expect("load selection"),
            clipboard.load_selection().expect("load selection")
        );
    }
}
