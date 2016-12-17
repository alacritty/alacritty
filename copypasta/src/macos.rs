//! Clipboard access on macOS
//!
//! Implemented according to
//! https://developer.apple.com/library/content/documentation/Cocoa/Conceptual/PasteboardGuide106/Articles/pbReading.html#//apple_ref/doc/uid/TP40008123-SW1

mod ns {
    extern crate objc_id;
    extern crate objc_foundation;

    #[link(name = "AppKit", kind = "framework")]
    extern {}

    use std::mem;

    use objc::runtime::{Class, Object};
    use self::objc_id::{Id, Owned};
    use self::objc_foundation::{NSArray, NSObject, NSDictionary, NSString};
    use self::objc_foundation::{INSString, INSArray, INSObject};

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

    /// A trait for reading contents from the pasteboard
    ///
    /// This is intended to reflect the underlying objective C API
    /// `readObjectsForClasses:options:`.
    pub trait PasteboardReadObject<T> {
        type Err;
        fn read_object(&self) -> Result<T, Self::Err>;
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
    }
}

#[derive(Debug)]
pub enum Error {
    CreatePasteboard(ns::NewPasteboardError),
    ReadString(ns::ReadStringError),
}


impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::CreatePasteboard(ref err) => Some(err),
            Error::ReadString(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::CreatePasteboard(ref _err) => "Failed to create pasteboard",
            Error::ReadString(ref _err) => "Failed to read string from pasteboard",
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

pub struct Clipboard(ns::Pasteboard);

impl super::Load for Clipboard {
    type Err = Error;

    fn new() -> Result<Self, Error> {
        Ok(Clipboard(ns::Pasteboard::new()?))
    }

    fn load_primary(&self) -> Result<String, Self::Err> {
        self::ns::PasteboardReadObject::<String>::read_object(&self.0)
            .map_err(::std::convert::From::from)
    }
}

#[cfg(test)]
mod tests {
    use super::Clipboard;
    use ::Load;

    #[test]
    fn create_clipboard_and_load_contents() {
        let clipboard = Clipboard::new().unwrap();
        println!("{:?}", clipboard.load_primary());
    }
}
