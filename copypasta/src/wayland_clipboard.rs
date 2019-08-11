// Copyright 2017 Avraham Weinstock
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::error::Error;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use smithay_clipboard::WaylandClipboard;

use wayland_client::sys::client::wl_display;
use wayland_client::Display;

use common::ClipboardProvider;

pub struct Clipboard {
    clipboard_context: Arc<Mutex<WaylandClipboard>>,
}
pub struct Primary {
    clipboard_context: Arc<Mutex<WaylandClipboard>>,
}

impl Primary {
    fn new(clipboard_context: Arc<Mutex<WaylandClipboard>>) -> Self {
        Self { clipboard_context }
    }
}

impl Clipboard {
    fn new(clipboard_context: Arc<Mutex<WaylandClipboard>>) -> Self {
        Self { clipboard_context }
    }
}

pub fn create_clipboards(display: &Display) -> (Primary, Clipboard) {
    let context = Arc::new(Mutex::new(WaylandClipboard::new(display)));
    let context_clone = context.clone();
    (Primary::new(context), Clipboard::new(context_clone))
}

pub unsafe fn create_clipboards_from_external(display: *mut c_void) -> (Primary, Clipboard) {
    let context =
        Arc::new(Mutex::new(WaylandClipboard::new_from_external(display as *mut wl_display)));
    let context_clone = context.clone();
    (Primary::new(context), Clipboard::new(context_clone))
}

impl ClipboardProvider for Clipboard {
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        let mut clipboard = self.clipboard_context.lock().unwrap();
        Ok(clipboard.load(None))
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        let mut clipboard = self.clipboard_context.lock().unwrap();
        clipboard.store(None, data);
        Ok(())
    }
}

impl ClipboardProvider for Primary {
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        let mut clipboard = self.clipboard_context.lock().unwrap();
        Ok(clipboard.load_primary(None))
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        let mut clipboard = self.clipboard_context.lock().unwrap();
        clipboard.store_primary(None, data);
        Ok(())
    }
}
