//! Thin wrapper around `arboard` that distinguishes the two clipboards we
//! actually care about: the regular system clipboard (Ctrl+V) and Linux's
//! PRIMARY selection (middle-click paste, alacritty's default auto-copy
//! target).  arboard's Linux backend supports both via the `SetExtLinux` /
//! `GetExtLinux` extensions when built with `wayland-data-control`.

#[cfg(target_os = "linux")]
use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};

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
