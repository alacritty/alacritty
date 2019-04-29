// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::ffi::c_void;

use clipboard::nop_clipboard::NopClipboardContext;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use clipboard::wayland_clipboard::WaylandClipboardContext;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use clipboard::x11_clipboard::{Primary as X11SecondaryClipboard, X11ClipboardContext};
use clipboard::{ClipboardContext, ClipboardProvider};

pub struct Clipboard {
    primary: Box<ClipboardProvider>,
    secondary: Option<Box<ClipboardProvider>>,
}

impl Clipboard {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn new(display: Option<*mut c_void>) -> Self {
        if let Some(display) = display {
            return Self {
                primary: unsafe { Box::new(WaylandClipboardContext::new_from_external(display)) },
                secondary: None,
            };
        }

        Self {
            primary: Box::new(ClipboardContext::new().unwrap()),
            secondary: Some(Box::new(X11ClipboardContext::<X11SecondaryClipboard>::new().unwrap())),
        }
    }

    // Use for tests and ref-tests
    pub fn new_nop() -> Self {
        Self { primary: Box::new(NopClipboardContext::new().unwrap()), secondary: None }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self { primary: Box::new(ClipboardContext::new().unwrap()), secondary: None }
    }
}

#[derive(Debug)]
pub enum ClipboardType {
    Primary,
    Secondary,
}

impl Clipboard {
    pub fn store(&mut self, ty: ClipboardType, text: impl Into<String>) {
        let clipboard = match (ty, &mut self.secondary) {
            (ClipboardType::Secondary, Some(provider)) => provider,
            (ClipboardType::Secondary, None) => return,
            _ => &mut self.primary,
        };

        clipboard.set_contents(text.into()).unwrap_or_else(|err| {
            warn!("Error storing selection to clipboard. {}", err);
        });
    }

    pub fn load(&mut self, ty: ClipboardType) -> Result<String, Box<std::error::Error>> {
        let clipboard = match (ty, &mut self.secondary) {
            (ClipboardType::Secondary, Some(provider)) => provider,
            _ => &mut self.primary,
        };

        clipboard.get_contents()
    }
}
