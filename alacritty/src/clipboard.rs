use log::{debug, warn};
use winit::raw_window_handle::RawDisplayHandle;

use alacritty_terminal::term::ClipboardType;

#[cfg(any(feature = "x11", target_os = "macos", windows))]
use copypasta::ClipboardContext;
use copypasta::ClipboardProvider;
use copypasta::nop_clipboard::NopClipboardContext;
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use copypasta::wayland_clipboard;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use copypasta::x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext};

mod image;

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    selection: Option<Box<dyn ClipboardProvider>>,
    image: fn() -> bool,
}

impl Clipboard {
    pub unsafe fn new(display: RawDisplayHandle) -> Self {
        match display {
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            RawDisplayHandle::Wayland(display) => {
                let (selection, clipboard) = unsafe {
                    wayland_clipboard::create_clipboards_from_external(display.display.as_ptr())
                };
                Self {
                    clipboard: Box::new(clipboard),
                    selection: Some(Box::new(selection)),
                    image: image::has_wayland_image,
                }
            },
            _ => Self::default(),
        }
    }

    /// Used for tests, to handle missing clipboard provider when built without the `x11`
    /// feature, and as default clipboard value.
    pub fn new_nop() -> Self {
        Self {
            clipboard: Box::new(NopClipboardContext::new().unwrap()),
            selection: None,
            image: || false,
        }
    }

    #[cfg(test)]
    pub fn new_test(text: &str, image: fn() -> bool) -> Self {
        Self { clipboard: Box::new(TestClipboard(text.into())), selection: None, image }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        #[cfg(any(target_os = "macos", windows))]
        return Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: None,
            image: image::has_image,
        };

        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        return Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: Some(Box::new(X11ClipboardContext::<X11SelectionClipboard>::new().unwrap())),
            image: image::has_x11_image,
        };

        #[cfg(not(any(feature = "x11", target_os = "macos", windows)))]
        return Self::new_nop();
    }
}

impl Clipboard {
    pub fn store(&mut self, ty: ClipboardType, text: impl Into<String>) {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            (ClipboardType::Selection, None) => return,
            _ => &mut self.clipboard,
        };

        clipboard.set_contents(text.into()).unwrap_or_else(|err| {
            warn!("Unable to store text in clipboard: {err}");
        });
    }

    pub fn load(&mut self, ty: ClipboardType) -> String {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            _ => &mut self.clipboard,
        };

        match clipboard.get_contents() {
            Err(err) => {
                debug!("Unable to load text from clipboard: {err}");
                String::new()
            },
            Ok(text) => text,
        }
    }

    /// Check for an image format without transferring the clipboard contents.
    pub fn has_image(&self) -> bool {
        (self.image)()
    }
}

#[cfg(test)]
struct TestClipboard(String);

#[cfg(test)]
impl ClipboardProvider for TestClipboard {
    fn get_contents(&mut self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.0.clone())
    }

    fn set_contents(
        &mut self,
        text: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.0 = text;
        Ok(())
    }
}
