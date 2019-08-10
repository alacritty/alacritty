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

use log::{debug, warn};

use copypasta::nop_clipboard::NopClipboardContext;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use copypasta::wayland_clipboard;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use copypasta::x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext};
use copypasta::{ClipboardContext, ClipboardProvider};

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    selection: Option<Box<dyn ClipboardProvider>>,
}

impl Clipboard {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn new(display: Option<*mut c_void>) -> Self {
        if let Some(display) = display {
            let (selection, clipboard) =
                unsafe { wayland_clipboard::create_clipboards_from_external(display) };
            return Self { clipboard: Box::new(clipboard), selection: Some(Box::new(selection)) };
        }

        Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: Some(Box::new(X11ClipboardContext::<X11SelectionClipboard>::new().unwrap())),
        }
    }

    // Use for tests and ref-tests
    pub fn new_nop() -> Self {
        Self { clipboard: Box::new(NopClipboardContext::new().unwrap()), selection: None }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self { clipboard: Box::new(ClipboardContext::new().unwrap()), selection: None }
    }
}

#[derive(Debug)]
pub enum ClipboardType {
    Clipboard,
    Selection,
}

impl Clipboard {
    pub fn store(&mut self, ty: ClipboardType, text: impl Into<String>) {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            (ClipboardType::Selection, None) => return,
            _ => &mut self.clipboard,
        };

        clipboard.set_contents(text.into()).unwrap_or_else(|err| {
            warn!("Unable to store text in clipboard: {}", err);
        });
    }

    pub fn load(&mut self, ty: ClipboardType) -> String {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            _ => &mut self.clipboard,
        };

        match clipboard.get_contents() {
            Err(err) => {
                debug!("Unable to load text from clipboard: {}", err);
                String::new()
            },
            Ok(text) => text,
        }
    }
}
