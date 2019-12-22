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
//
//! ANSI Terminal Stream Parsing
#[cfg(windows)]
use crate::util::win32_string;

// Use the default behavior of the other platforms.
#[cfg(not(windows))]
pub fn attach_handler() {}

// Install a panic handler that renders the panic in a classical Windows error
// dialog box as well as writes the panic to STDERR.
#[cfg(windows)]
pub fn attach_handler() {
    use std::{io, io::Write, panic, ptr};
    use winapi::um::winuser;

    panic::set_hook(Box::new(|panic_info| {
        let _ = writeln!(io::stderr(), "{}", panic_info);
        let msg = format!("{}\n\nPress Ctrl-C to Copy", panic_info);
        unsafe {
            winuser::MessageBoxW(
                ptr::null_mut(),
                win32_string(&msg).as_ptr(),
                win32_string("Alacritty: Runtime Error").as_ptr(),
                winuser::MB_ICONERROR
                    | winuser::MB_OK
                    | winuser::MB_SETFOREGROUND
                    | winuser::MB_TASKMODAL,
            );
        }
    }));
}
