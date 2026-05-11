//! Thin wrapper around `arboard` that distinguishes the two clipboards we
//! actually care about: the regular system clipboard (Ctrl+V) and Linux's
//! PRIMARY selection (middle-click paste, alacritty's default auto-copy
//! target).  arboard's Linux backend supports both via the `SetExtLinux` /
//! `GetExtLinux` extensions when built with `wayland-data-control`.

#[cfg(target_os = "linux")]
use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};

/// Sentinel text that smuggles a Ctrl+V keystroke past egui_winit's
/// text-only paste handler when the real clipboard holds an image.
pub const IMAGE_PASTE_MARKER: &str = "\u{1}alacritree-image-paste\u{1}";

#[derive(Copy, Clone, Debug)]
pub enum Target {
    /// `Ctrl+V` clipboard.
    Clipboard,
    /// Linux PRIMARY selection (X11 / Wayland primary).  Falls back to the
    /// regular clipboard on platforms that don't have a separate PRIMARY.
    Primary,
}

pub fn write(target: Target, text: &str) {
    if text.is_empty() {
        return;
    }
    let mut clip = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("clipboard unavailable: {e}");
            return;
        },
    };
    let res = match target {
        Target::Clipboard => clip.set_text(text.to_owned()),
        #[cfg(target_os = "linux")]
        Target::Primary => clip.set().clipboard(LinuxClipboardKind::Primary).text(text.to_owned()),
        #[cfg(not(target_os = "linux"))]
        Target::Primary => clip.set_text(text.to_owned()),
    };
    if let Err(e) = res {
        log::warn!("clipboard write ({:?}) failed: {e}", target);
    }
}

pub fn read(target: Target) -> Option<String> {
    let mut clip = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("clipboard unavailable: {e}");
            return None;
        },
    };
    let res = match target {
        Target::Clipboard => clip.get_text(),
        #[cfg(target_os = "linux")]
        Target::Primary => clip.get().clipboard(LinuxClipboardKind::Primary).text(),
        #[cfg(not(target_os = "linux"))]
        Target::Primary => clip.get_text(),
    };
    match res {
        Ok(s) => Some(s),
        Err(arboard::Error::ContentNotAvailable) => None,
        Err(e) => {
            log::warn!("clipboard read ({:?}) failed: {e}", target);
            None
        },
    }
}

#[derive(Clone)]
pub struct PendingImage {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

pub fn stash_image_and_mark() -> Option<PendingImage> {
    let mut clip = arboard::Clipboard::new().ok()?;
    if let Ok(text) = clip.get_text() {
        if !text.is_empty() {
            return None;
        }
    }
    let image = match clip.get_image() {
        Ok(img) => img,
        Err(arboard::Error::ContentNotAvailable) => return None,
        Err(e) => {
            log::warn!("clipboard image read failed: {e}");
            return None;
        },
    };
    let stash =
        PendingImage { width: image.width, height: image.height, bytes: image.bytes.into_owned() };
    if let Err(e) = clip.set_text(IMAGE_PASTE_MARKER.to_owned()) {
        log::warn!("clipboard marker write failed: {e}");
        return None;
    }
    Some(stash)
}

pub fn restore_image(image: &PendingImage) {
    let mut clip = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("clipboard unavailable for image restore: {e}");
            return;
        },
    };
    let res = clip.set_image(arboard::ImageData {
        width: image.width,
        height: image.height,
        bytes: std::borrow::Cow::Borrowed(&image.bytes),
    });
    if let Err(e) = res {
        log::warn!("clipboard image restore failed: {e}");
    }
}
