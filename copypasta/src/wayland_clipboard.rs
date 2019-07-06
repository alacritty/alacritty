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
use std::marker::PhantomData;

use smithay_clipboard::WaylandClipboard;
use wayland_client::sys::client::wl_display;
use wayland_client::Display;

use common::ClipboardProvider;

pub trait ClipboardType: Send {}

pub struct Clipboard;
impl ClipboardType for Clipboard {}

pub struct Primary;
impl ClipboardType for Primary {}

pub struct WaylandClipboardContext<T: ClipboardType>(WaylandClipboard, PhantomData<T>);

impl<T: ClipboardType> WaylandClipboardContext<T> {
    /// Create a new clipboard context.
    pub fn new(display: &Display) -> Self {
        WaylandClipboardContext(WaylandClipboard::new(display), PhantomData)
    }

    /// Create a new clipboard context from an external pointer.
    pub unsafe fn new_from_external(display: *mut c_void) -> Self {
        WaylandClipboardContext(
            WaylandClipboard::new_from_external(display as *mut wl_display),
            PhantomData,
        )
    }
}

impl ClipboardProvider for WaylandClipboardContext<Clipboard> {
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        Ok(self.0.load(None))
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        self.0.store(None, data);
        Ok(())
    }
}

impl ClipboardProvider for WaylandClipboardContext<Primary> {
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        Ok(self.0.load_primary(None))
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        self.0.store_primary(None, data);
        Ok(())
    }
}
