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
    context: Arc<Mutex<WaylandClipboard>>,
}

pub struct Primary {
    context: Arc<Mutex<WaylandClipboard>>,
}

pub fn create_clipboards(display: &Display) -> (Primary, Clipboard) {
    let context = Arc::new(Mutex::new(WaylandClipboard::new(display)));

    (Primary { context: context.clone() }, Clipboard { context })
}

/// Create new clipboard from a raw display pointer.
///
/// # Safety
///
/// Since the type of the display is a raw pointer, it's the responsibility of the callee to make
/// sure that the passed pointer is a valid Wayland display.
pub unsafe fn create_clipboards_from_external(display: *mut c_void) -> (Primary, Clipboard) {
    let context =
        Arc::new(Mutex::new(WaylandClipboard::new_from_external(display as *mut wl_display)));

    (Primary { context: context.clone() }, Clipboard { context })
}

impl ClipboardProvider for Clipboard {
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        Ok(self.context.lock().unwrap().load(None))
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        self.context.lock().unwrap().store(None, data);

        Ok(())
    }
}

impl ClipboardProvider for Primary {
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        Ok(self.context.lock().unwrap().load_primary(None))
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        self.context.lock().unwrap().store_primary(None, data);

        Ok(())
    }
}
