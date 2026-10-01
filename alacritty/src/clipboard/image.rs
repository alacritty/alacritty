//! Clipboard image format detection without reading or decoding image data.

#[cfg(target_os = "macos")]
pub fn has_image() -> bool {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
    use objc2_foundation::NSArray;

    autoreleasepool(|_| {
        // SAFETY: These constants are provided by AppKit and are always valid.
        let types = unsafe { NSArray::from_slice(&[NSPasteboardTypePNG, NSPasteboardTypeTIFF]) };
        NSPasteboard::generalPasteboard().availableTypeFromArray(&types).is_some()
    })
}

#[cfg(windows)]
pub fn has_image() -> bool {
    use windows_sys::Win32::System::DataExchange::{
        IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };
    use windows_sys::Win32::System::Ole::{CF_BITMAP, CF_DIB, CF_DIBV5};
    use windows_sys::core::w;

    // SAFETY: Format queries do not require opening the clipboard. The PNG name is terminated.
    unsafe {
        let png = RegisterClipboardFormatW(w!("PNG"));
        [u32::from(CF_BITMAP), u32::from(CF_DIB), u32::from(CF_DIBV5), png]
            .into_iter()
            .any(|format| format != 0 && IsClipboardFormatAvailable(format) != 0)
    }
}

#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
pub fn has_wayland_image() -> bool {
    use wl_clipboard_rs::paste::{ClipboardType, Error, Seat, get_mime_types};

    match get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
        Ok(types) => types.iter().any(|mime| mime.starts_with("image/")),
        Err(Error::ClipboardEmpty | Error::NoSeats | Error::NoMimeType) => false,
        Err(err) => {
            log::debug!("Unable to query image clipboard formats: {err}");
            false
        },
    }
}

#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
pub fn has_x11_image() -> bool {
    match x11::has_image() {
        Ok(has_image) => has_image,
        Err(err) => {
            log::debug!("Unable to query image clipboard formats: {err}");
            false
        },
    }
}

#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
mod x11 {
    use std::error::Error;
    use std::thread;
    use std::time::{Duration, Instant};

    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, WindowClass};
    use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME, NONE};

    x11rb::atom_manager! {
        Atoms: AtomsCookie {
            CLIPBOARD,
            TARGETS,
            ALACRITTY_CLIPBOARD_FORMATS,
        }
    }

    pub fn has_image() -> Result<bool, Box<dyn Error>> {
        // Connect only on image paste. A fresh requestor also isolates late replies after timeout.
        let (connection, screen) = x11rb::connect(None)?;
        let atoms = Atoms::new(&connection)?.reply()?;
        if connection.get_selection_owner(atoms.CLIPBOARD)?.reply()?.owner == NONE {
            return Ok(false);
        }

        let window = connection.generate_id()?;
        connection.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            connection.setup().roots[screen].root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )?;
        connection.convert_selection(
            window,
            atoms.CLIPBOARD,
            atoms.TARGETS,
            atoms.ALACRITTY_CLIPBOARD_FORMATS,
            CURRENT_TIME,
        )?;
        connection.flush()?;

        // Clipboard owners can fail to answer; never wait indefinitely on the input thread.
        let deadline = Instant::now() + Duration::from_millis(200);
        while Instant::now() < deadline {
            let Some(event) = connection.poll_for_event()? else {
                thread::sleep(Duration::from_millis(1));
                continue;
            };
            let Event::SelectionNotify(event) = event else { continue };
            if event.requestor != window
                || event.selection != atoms.CLIPBOARD
                || event.target != atoms.TARGETS
            {
                continue;
            }
            if event.property == NONE {
                return Ok(false);
            }

            // Bound the metadata read to 4096 format atoms. No image payload is requested.
            let formats = connection
                .get_property(true, window, event.property, AtomEnum::ATOM, 0, 4096)?
                .reply()?;
            let Some(formats) = formats.value32() else { return Ok(false) };
            let names = formats.map(|format| connection.get_atom_name(format));
            let names = names.collect::<Result<Vec<_>, _>>()?;
            for name in names {
                let name = name.reply()?.name;
                if name.starts_with(b"image/") {
                    return Ok(true);
                }
            }
            return Ok(false);
        }

        Err("Clipboard format query timed out".into())
    }
}
