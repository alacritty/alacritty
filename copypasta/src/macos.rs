//! Clipboard access on macOS
//!
//! Implemented according to
//! https://developer.apple.com/library/content/documentation/Cocoa/Conceptual/PasteboardGuide106/Articles/pbReading.html#//apple_ref/doc/uid/TP40008123-SW1

mod ns {
    #[link(name = "AppKit", kind = "framework")]
    extern {}

    use std::mem;

    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};
    use objc_id::{Id, Owned};
    use objc_foundation::{NSArray, NSObject, NSDictionary, NSString};
    use objc_foundation::{INSString, INSArray, INSObject};

    /// Rust API for NSPasteboard
    pub struct Pasteboard(Id<Object>);

    /// Errors occurring when creating a Pasteboard
    #[derive(Debug)]
    pub enum NewPasteboardError {
        GetPasteboardClass,
        LoadGeneralPasteboard,
    }

    /// Errors occurring when reading a string from the pasteboard
    #[derive(Debug)]
    pub enum ReadStringError {
        GetStringClass,
        ReadObjectsForClasses,
    }

    /// Errors from writing strings to the pasteboard
    #[derive(Debug)]
    pub struct WriteStringError;

    /// A trait for reading contents from the pasteboard
    ///
    /// This is intended to reflect the underlying objective C API
    /// `readObjectsForClasses:options:`.
    pub trait PasteboardReadObject<T> {
        type Err;
        fn read_object(&self) -> Result<T, Self::Err>;
    }

    /// A trait for writing contents to the pasteboard
    pub trait PasteboardWriteObject<T> {
        type Err;
        fn write_object(&mut self, object: T) -> Result<(), Self::Err>;
    }

    impl PasteboardReadObject<String> for Pasteboard {
        type Err = ReadStringError;
        fn read_object(&self) -> Result<String, ReadStringError> {
            // Get string class; need this for passing to readObjectsForClasses
            let ns_string_class = match Class::get("NSString") {
                Some(class) => class,
                None => return Err(ReadStringError::GetStringClass),
            };

            let ns_string: Id<Object> = unsafe {
                let ptr: *mut Object = msg_send![ns_string_class, class];

                if ptr.is_null() {
                    return Err(ReadStringError::GetStringClass);
                } else {
                    Id::from_ptr(ptr)
                }
            };

            let classes: Id<NSArray<NSObject, Owned>> = unsafe {
                // I think this transmute is valid. It's going from an
                // Id<Object> to an Id<NSObject>. From transmute's perspective,
                // the only thing that matters is that they both have the same
                // size (they do for now since the generic is phantom data).  In
                // both cases, the underlying pointer is an id (from `[NSString
                // class]`), so again, this should be valid. There's just
                // nothing implemented in objc_id or objc_foundation to do this
                // "safely". By the way, the only reason this is necessary is
                // because INSObject isn't implemented for Id<Object>.
                //
                // And if that argument isn't convincing, my final reasoning is
                // that "it seems to work".
                NSArray::from_vec(vec![mem::transmute(ns_string)])
            };

            // No options
            //
            // Apparently this doesn't compile without a type hint, so it maps
            // objects to objects!
            let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();

            // call [pasteboard readObjectsForClasses:options:]
            let copied_items = unsafe {
                let copied_items: *mut NSArray<NSString> = msg_send![
                    self.0,
                    readObjectsForClasses:&*classes
                    options:&*options
                ];

                if copied_items.is_null() {
                    return Err(ReadStringError::ReadObjectsForClasses);
                } else {
                    Id::from_ptr(copied_items) as Id<NSArray<NSString>>
                }
            };

            // Ok, this is great. We have an NSArray<NSString>, and these have
            // decent bindings. Use the first item returned (if an item was
            // returned) or just return an empty string
            //
            // XXX Should this return an error if no items were returned?
            let contents = copied_items
                .first_object()
                .map(|ns_string| ns_string.as_str().to_owned())
                .unwrap_or_else(String::new);

            Ok(contents)
        }
    }

    impl PasteboardWriteObject<String> for Pasteboard {
        type Err = WriteStringError;

        fn write_object(&mut self, object: String) -> Result<(), Self::Err> {
            let objects = NSArray::from_vec(vec![NSString::from_str(&object)]);

            self.clear_contents();

            // The writeObjects method returns true in case of success, and
            // false otherwise.
            let ok: bool = unsafe {
                msg_send![self.0, writeObjects:objects]
            };

            if ok {
                Ok(())
            } else {
                Err(WriteStringError)
            }
        }
    }

    impl ::std::error::Error for WriteStringError {
        fn description(&self) -> &str {
            "Failed writing string to the NSPasteboard (writeContents returned false)"
        }
    }

    impl ::std::fmt::Display for WriteStringError {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            f.write_str(::std::error::Error::description(self))
        }
    }

    impl ::std::error::Error for ReadStringError {
        fn description(&self) -> &str {
            match *self {
                ReadStringError::GetStringClass => "NSString class not found",
                ReadStringError::ReadObjectsForClasses => "readObjectsForClasses:options: failed",
            }
        }
    }

    impl ::std::fmt::Display for ReadStringError {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            f.write_str(::std::error::Error::description(self))
        }
    }

    impl ::std::error::Error for NewPasteboardError {
        fn description(&self) -> &str {
            match *self {
                NewPasteboardError::GetPasteboardClass => {
                    "NSPasteboard class not found"
                },
                NewPasteboardError::LoadGeneralPasteboard => {
                    "[NSPasteboard generalPasteboard] failed"
                },
            }
        }
    }

    impl ::std::fmt::Display for NewPasteboardError {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            f.write_str(::std::error::Error::description(self))
        }
    }

    impl Pasteboard {
        pub fn new() -> Result<Pasteboard, NewPasteboardError> {
            // NSPasteboard *pasteboard = [NSPasteboard generalPasteboard];
            let ns_pasteboard_class = match Class::get("NSPasteboard") {
                Some(class) => class,
                None => return Err(NewPasteboardError::GetPasteboardClass),
            };

            let ptr = unsafe {
                let ptr: *mut Object = msg_send![ns_pasteboard_class, generalPasteboard];

                if ptr.is_null() {
                    return Err(NewPasteboardError::LoadGeneralPasteboard);
                } else {
                    ptr
                }
            };

            let id = unsafe {
                Id::from_ptr(ptr)
            };

            Ok(Pasteboard(id))
        }

        /// Clears the existing contents of the pasteboard, preparing it for new
        /// contents.
        ///
        /// This is the first step in providing data on the pasteboard. The
        /// return value is the change count of the pasteboard
        pub fn clear_contents(&mut self) -> usize {
            unsafe {
                msg_send![self.0, clearContents]
            }
        }
    }
}

#[derive(Debug)]
pub enum Error {
    CreatePasteboard(ns::NewPasteboardError),
    ReadString(ns::ReadStringError),
    WriteString(ns::WriteStringError),
}


impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::CreatePasteboard(ref err) => Some(err),
            Error::ReadString(ref err) => Some(err),
            Error::WriteString(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::CreatePasteboard(ref _err) => "Failed to create pasteboard",
            Error::ReadString(ref _err) => "Failed to read string from pasteboard",
            Error::WriteString(ref _err) => "Failed to write string to pasteboard",
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::CreatePasteboard(ref err) => {
                write!(f, "Failed to create pasteboard: {}", err)
            },
            Error::ReadString(ref err) => {
                write!(f, "Failed to read string from pasteboard: {}", err)
            },
            Error::WriteString(ref err) => {
                write!(f, "Failed to write string to pasteboard: {}", err)
            },
        }
    }
}

impl From<ns::NewPasteboardError> for Error {
    fn from(val: ns::NewPasteboardError) -> Error {
        Error::CreatePasteboard(val)
    }
}

impl From<ns::ReadStringError> for Error {
    fn from(val: ns::ReadStringError) -> Error {
        Error::ReadString(val)
    }
}

impl From<ns::WriteStringError> for Error {
    fn from(val: ns::WriteStringError) -> Error {
        Error::WriteString(val)
    }
}

pub struct Clipboard(ns::Pasteboard);

impl super::Load for Clipboard {
    type Err = Error;

    fn new() -> Result<Self, Error> {
        Ok(Clipboard(ns::Pasteboard::new()?))
    }

    fn load_primary(&self) -> Result<String, Self::Err> {
        use self::ns::PasteboardReadObject;

        self.0.read_object()
            .map_err(::std::convert::From::from)
    }
}

impl super::Store for Clipboard {
    fn store_primary<S>(&mut self, contents: S) -> Result<(), Self::Err>
        where S: Into<String>
    {
        use self::ns::PasteboardWriteObject;

        self.0.write_object(contents.into())
            .map_err(::std::convert::From::from)
    }

    fn store_selection<S>(&mut self, _contents: S) -> Result<(), Self::Err>
        where S: Into<String>
    {
        // No such thing on macOS
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Clipboard;
    use ::{Load, Store};

    #[test]
    fn create_clipboard_save_load_contents() {
        let mut clipboard = Clipboard::new().unwrap();
        clipboard.store_primary("arst").unwrap();
        let loaded = clipboard.load_primary().unwrap();
        assert_eq!("arst", loaded);
    }
}
