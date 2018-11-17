//! Linux clipboard implementation
//!
//! This implementation uses xclip by default to access the clipboard however
//! waylands clipboard can be used by calling `set_wayland()` on `Clipboard`
//!
//! Note that the x11 implementation is really crap right now - we just depend
//! on xclip being on the user's path. If x11 pasting doesn't work, it's
//! probably because xclip is unavailable. There's currently no non-GPL x11
//! clipboard library for Rust. Until then, we have this hack.
//!
//! FIXME: Implement actual X11 clipboard API using the ICCCM reference
//!        https://tronche.com/gui/x/icccm/

use std::ffi::OsStr;
use std::io;
use std::io::{Read, Write};
use std::process::{Command, Output};
use std::string::FromUtf8Error;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use super::{Load, Store};

use sctk::data_device::DataSource;
use sctk::data_device::DataSourceEvent;
use sctk::data_device::ReadPipe;
use sctk::data_device::{DataDevice, DndEvent};
use sctk::keyboard::{map_keyboard_auto, Event as KbEvent};
use sctk::reexports::client::Display;
use sctk::wayland_client::sys::client::wl_display;
use sctk::Environment;

/// The linux clipboard
pub struct Clipboard {
    request_send: Option<mpsc::Sender<WaylandRequest>>,
    load_recv: Option<mpsc::Receiver<String>>,
}

enum WaylandRequest {
    Store(String),
    Load,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Xclip(String),
    Wayland(String),
    Utf8(FromUtf8Error),
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::Io(ref err) => Some(err),
            Error::Utf8(ref err) => Some(err),
            _ => None,
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Io(..) => "error calling xclip",
            Error::Xclip(..) => "error reported by xclip",
            Error::Wayland(..) => "error reported by wayland",
            Error::Utf8(..) => "clipboard contents not utf8",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::Io(ref err) => match err.kind() {
                io::ErrorKind::NotFound => {
                    write!(f, "Please install `xclip` to enable clipboard support")
                }
                _ => write!(f, "error calling xclip: {}", err),
            },
            Error::Xclip(ref s) => write!(f, "error from xclip: {}", s),
            Error::Wayland(ref s) => write!(f, "error from wayland: {}", s),
            Error::Utf8(ref err) => write!(f, "error parsing xclip output: {}", err),
        }
    }
}

impl From<io::Error> for Error {
    fn from(val: io::Error) -> Error {
        Error::Io(val)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(val: FromUtf8Error) -> Error {
        Error::Utf8(val)
    }
}

impl Load for Clipboard {
    type Err = Error;

    fn new() -> Result<Self, Error> {
        Ok(Clipboard {
            request_send: None,
            load_recv: None, 
        })
    }

    fn load_primary(&self) -> Result<String, Self::Err> {
        if self.request_send.is_some() {
            // Same as `load_selection()` since wayland doesn't have a primary clipboard
            self.load_selection()
        } else {
            let output = Command::new("xclip")
                .args(&["-o", "-selection", "clipboard"])
                .output()?;

            Clipboard::process_xclip_output(output)
        }
    }

    fn load_selection(&self) -> Result<String, Self::Err> {
            if let Some(ref request_send) = self.request_send {
                request_send.send(WaylandRequest::Load).unwrap();
            }
            if let Some(ref load_recv) = self.load_recv {
                Ok(load_recv.recv().unwrap())
            } else {
                let output = Command::new("xclip").args(&["-o"]).output()?;
                Clipboard::process_xclip_output(output)
            }
    }
}

impl Store for Clipboard {
    /// Sets the primary clipboard contents
    #[inline]
    fn store_primary<S>(&mut self, contents: S) -> Result<(), Self::Err>
    where
        S: Into<String>,
    {
        if self.request_send.is_some() {
            // Same as `store_selection()` since wayland doesn't have a primary clipboard
            self.store_selection(contents)
        } else {
            self.xclip_store(contents, &["-i", "-selection", "clipboard"])
        }
    }

    /// Sets the secondary clipboard contents
    #[inline]
    fn store_selection<S>(&mut self, contents: S) -> Result<(), Self::Err>
    where
        S: Into<String>,
    {
        if let Some(ref request_send) = self.request_send {
            Ok(request_send.send(WaylandRequest::Store(contents.into())).unwrap())
        } else {
            self.xclip_store(contents, &["-i"])
        }
    }
}

impl Clipboard {
    /// Sets Clipboard to use wayland to access the system clipboard
    pub fn set_wayland(&mut self, wayland_display: *mut ::std::os::raw::c_void) {
        let wayland_display = unsafe { (wayland_display as *mut wl_display).as_mut().unwrap() };
        let (request_send, request_recv) = mpsc::channel::<WaylandRequest>();
        let (load_send, load_recv) = mpsc::channel();
        self.load_recv = Some(load_recv);
        self.request_send = Some(request_send);
        ::std::thread::spawn(move || {
            let (display, mut event_queue) =
                unsafe { Display::from_external_display(wayland_display as *mut wl_display) };
            let env = Environment::from_display(&*display, &mut event_queue)
                .unwrap();

            let seat = env
                .manager
                .instantiate_auto(|seat| {
                    seat.implement(|_, _| {}, ()) 
                })
                .unwrap();

            let device = DataDevice::init_for_seat(
                &env.data_device_manager,
                &seat,
                |event| {
                    match event {
                        // we don't accept drag'n'drop
                        DndEvent::Enter {
                            offer: Some(offer), ..
                        } => offer.accept(None),
                        _ => (),
                    }
                },
            );

            let enter_serial = Arc::new(Mutex::new(None));
            let my_enter_serial = enter_serial.clone();
            let _keyboard = map_keyboard_auto(
                &seat,
                move |event: KbEvent, _| match event {
                    KbEvent::Enter { serial, .. } => {
                        *(my_enter_serial.lock().unwrap()) = Some(serial);
                    }
                    _ => (),
                },
            );

            loop {
                if let Ok(request) = request_recv.try_recv() {
                    match request {
                        WaylandRequest::Load => {
                            // Load
                            event_queue.dispatch_pending().unwrap();
                            let mut reader = None::<ReadPipe>;
                            device.with_selection(|offer| {
                                if let Some(offer) = offer {
                                    offer.with_mime_types(|types| {
                                        for t in types {
                                            if t == "text/plain;charset=utf-8" {
                                                reader = Some(
                                                    offer
                                                        .receive("text/plain;charset=utf-8".into())
                                                        .unwrap(),
                                                );
                                            }
                                        }
                                    });
                                }
                            });
                            display.flush().unwrap();
                            if let Some(mut reader) = reader {
                                let mut contents = String::new();
                                reader.read_to_string(&mut contents).unwrap();
                                load_send.send(contents).unwrap();
                            } else {
                                load_send.send("".to_string()).unwrap();
                            }
                        }
                        WaylandRequest::Store(contents) => {
                            // let display = display.clone();
                            event_queue.dispatch_pending().unwrap();
                            let data_source = DataSource::new(
                                &env.data_device_manager,
                                &["text/plain;charset=utf-8"],
                                move |source_event| match source_event {
                                    DataSourceEvent::Send { mut pipe, .. } => {
                                        // let _ = display.lock().unwrap().flush();
                                        let _ = write!(pipe, "{}", contents);
                                    }
                                    _ => {}
                                },
                            );
                            event_queue.dispatch_pending().unwrap();
                            if let Some(enter_serial) = *enter_serial.lock().unwrap() {
                                device.set_selection(&Some(data_source), enter_serial);
                            }
                        }
                    }
                }
                event_queue.dispatch_pending().unwrap();
                ::std::thread::sleep(::std::time::Duration::from_millis(50));
            }
        });
    }

    fn process_xclip_output(output: Output) -> Result<String, Error> {
        if output.status.success() {
            String::from_utf8(output.stdout).map_err(::std::convert::From::from)
        } else {
            String::from_utf8(output.stderr).map_err(::std::convert::From::from)
        }
    }

    fn xclip_store<C, S>(&self, contents: C, args: &[S]) -> Result<(), Error>
    where
        C: Into<String>,
        S: AsRef<OsStr>,
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let contents = contents.into();
        let mut child = Command::new("xclip")
            .args(args)
            .stdin(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(contents.as_bytes())?;
        }

        // Return error if didn't exit cleanly
        let exit_status = child.wait()?;
        if exit_status.success() {
            Ok(())
        } else {
            Err(Error::Xclip("xclip returned non-zero exit code".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Clipboard;
    use {Load, Store};

    #[test]
    fn clipboard_works() {
        let mut clipboard = Clipboard::new().expect("create clipboard");
        let arst = "arst";
        let oien = "oien";
        clipboard.store_primary(arst).expect("store selection");
        clipboard.store_selection(oien).expect("store selection");

        let selection = clipboard.load_selection().expect("load selection");
        let primary = clipboard.load_primary().expect("load selection");

        assert_eq!(arst, primary);
        assert_eq!(oien, selection);
    }
}
