//! Clipboard access on macOS
//!
//! Implemented according to https://developer.apple.com/library/content/documentation/Cocoa/Conceptual/PasteboardGuide106/Articles/pbReading.html#//apple_ref/doc/uid/TP40008123-SW1
//!
//! FIXME implement this :)

struct Clipboard;

impl Load for Clipboard {
    type Err = ();

    fn new() -> Result<Self, Error> {
        Ok(Clipboard)
    }

    fn load_primary(&self) -> Result<String, Self::Err> {
        Ok(String::new())
    }
}
