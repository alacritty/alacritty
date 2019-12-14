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

use std::i16;
use std::io::Error;
use std::mem;
use std::os::windows::io::IntoRawHandle;
use std::ptr;
use std::sync::Arc;

use dunce::canonicalize;
use mio_anonymous_pipes::{EventedAnonRead, EventedAnonWrite};
use miow;
use widestring::U16CString;
use winapi::shared::basetsd::{PSIZE_T, SIZE_T};
use winapi::shared::minwindef::{BYTE, DWORD};
use winapi::shared::ntdef::{HANDLE, HRESULT, LPWSTR};
use winapi::shared::winerror::S_OK;
use winapi::um::libloaderapi::{GetModuleHandleA, GetProcAddress};
use winapi::um::processthreadsapi::{
    CreateProcessW, InitializeProcThreadAttributeList, UpdateProcThreadAttribute,
    PROCESS_INFORMATION, STARTUPINFOW,
};
use winapi::um::winbase::{EXTENDED_STARTUPINFO_PRESENT, STARTF_USESTDHANDLES, STARTUPINFOEXW};
use winapi::um::wincontypes::{COORD, HPCON};

use crate::config::{Config, Shell};
use crate::event::OnResize;
use crate::term::SizeInfo;
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::windows::Pty;

// TODO: Replace with winapi's implementation. This cannot be
//  done until a safety net is in place for versions of Windows
//  that do not support the ConPTY api, as such versions will
//  pass unit testing - but fail to actually function.
/// Dynamically-loaded Pseudoconsole API from kernel32.dll
///
/// The field names are deliberately PascalCase as this matches
/// the defined symbols in kernel32 and also is the convention
/// that the `winapi` crate follows.
#[allow(non_snake_case)]
struct ConptyApi {
    CreatePseudoConsole:
        unsafe extern "system" fn(COORD, HANDLE, HANDLE, DWORD, *mut HPCON) -> HRESULT,
    ResizePseudoConsole: unsafe extern "system" fn(HPCON, COORD) -> HRESULT,
    ClosePseudoConsole: unsafe extern "system" fn(HPCON),
}

impl ConptyApi {
    /// Load the API or None if it cannot be found.
    pub fn new() -> Option<Self> {
        // Unsafe because windows API calls
        unsafe {
            let hmodule = GetModuleHandleA("kernel32\0".as_ptr() as _);
            assert!(!hmodule.is_null());

            let cpc = GetProcAddress(hmodule, "CreatePseudoConsole\0".as_ptr() as _);
            let rpc = GetProcAddress(hmodule, "ResizePseudoConsole\0".as_ptr() as _);
            let clpc = GetProcAddress(hmodule, "ClosePseudoConsole\0".as_ptr() as _);

            if cpc.is_null() || rpc.is_null() || clpc.is_null() {
                None
            } else {
                Some(Self {
                    CreatePseudoConsole: mem::transmute(cpc),
                    ResizePseudoConsole: mem::transmute(rpc),
                    ClosePseudoConsole: mem::transmute(clpc),
                })
            }
        }
    }
}

/// RAII Pseudoconsole
pub struct Conpty {
    pub handle: HPCON,
    api: ConptyApi,
}

/// Handle can be cloned freely and moved between threads.
pub type ConptyHandle = Arc<Conpty>;

impl Drop for Conpty {
    fn drop(&mut self) {
        // XXX: This will block until the conout pipe is drained. Will cause a deadlock if the
        // conout pipe has already been dropped by this point.
        //
        // See PR #3084 and https://docs.microsoft.com/en-us/windows/console/closepseudoconsole
        unsafe { (self.api.ClosePseudoConsole)(self.handle) }
    }
}

// The Conpty API can be accessed from multiple threads.
unsafe impl Send for Conpty {}
unsafe impl Sync for Conpty {}

pub fn new<C>(config: &Config<C>, size: &SizeInfo, _window_id: Option<usize>) -> Option<Pty> {
    if !config.enable_experimental_conpty_backend {
        return None;
    }

    let api = ConptyApi::new()?;

    let mut pty_handle = 0 as HPCON;

    // Passing 0 as the size parameter allows the "system default" buffer
    // size to be used. There may be small performance and memory advantages
    // to be gained by tuning this in the future, but it's likely a reasonable
    // start point.
    let (conout, conout_pty_handle) = miow::pipe::anonymous(0).unwrap();
    let (conin_pty_handle, conin) = miow::pipe::anonymous(0).unwrap();

    let coord =
        coord_from_sizeinfo(size).expect("Overflow when creating initial size on pseudoconsole");

    // Create the Pseudo Console, using the pipes
    let result = unsafe {
        (api.CreatePseudoConsole)(
            coord,
            conin_pty_handle.into_raw_handle(),
            conout_pty_handle.into_raw_handle(),
            0,
            &mut pty_handle as *mut HPCON,
        )
    };

    assert_eq!(result, S_OK);

    let mut success;

    // Prepare child process startup info

    let mut size: SIZE_T = 0;

    let mut startup_info_ex: STARTUPINFOEXW = Default::default();

    let title = config.window.title.clone();
    let title = U16CString::from_str(title).unwrap();
    startup_info_ex.StartupInfo.lpTitle = title.as_ptr() as LPWSTR;

    startup_info_ex.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;

    // Setting this flag but leaving all the handles as default (null) ensures the
    // pty process does not inherit any handles from this Alacritty process.
    startup_info_ex.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;

    // Create the appropriately sized thread attribute list.
    unsafe {
        let failure =
            InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size as PSIZE_T) > 0;

        // This call was expected to return false.
        if failure {
            panic_shell_spawn();
        }
    }

    let mut attr_list: Box<[BYTE]> = vec![0; size].into_boxed_slice();

    // Set startup info's attribute list & initialize it
    //
    // Lint failure is spurious; it's because winapi's definition of PROC_THREAD_ATTRIBUTE_LIST
    // implies it is one pointer in size (32 or 64 bits) but really this is just a dummy value.
    // Casting a *mut u8 (pointer to 8 bit type) might therefore not be aligned correctly in
    // the compiler's eyes.
    #[allow(clippy::cast_ptr_alignment)]
    {
        startup_info_ex.lpAttributeList = attr_list.as_mut_ptr() as _;
    }

    unsafe {
        success = InitializeProcThreadAttributeList(
            startup_info_ex.lpAttributeList,
            1,
            0,
            &mut size as PSIZE_T,
        ) > 0;

        if !success {
            panic_shell_spawn();
        }
    }

    // Set thread attribute list's Pseudo Console to the specified ConPTY
    unsafe {
        success = UpdateProcThreadAttribute(
            startup_info_ex.lpAttributeList,
            0,
            22 | 0x0002_0000, // PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE
            pty_handle,
            mem::size_of::<HPCON>(),
            ptr::null_mut(),
            ptr::null_mut(),
        ) > 0;

        if !success {
            panic_shell_spawn();
        }
    }

    // Get process commandline
    let default_shell = &Shell::new("powershell");
    let shell = config.shell.as_ref().unwrap_or(default_shell);
    let mut cmdline = shell.args.clone();
    cmdline.insert(0, shell.program.to_string());

    // Warning, here be borrow hell
    let cwd = config.working_directory().as_ref().map(|dir| canonicalize(dir).unwrap());
    let cwd = cwd.as_ref().map(|dir| dir.to_str().unwrap());

    // Create the client application, using startup info containing ConPTY info
    let cmdline = U16CString::from_str(&cmdline.join(" ")).unwrap();
    let cwd = cwd.map(|s| U16CString::from_str(&s).unwrap());

    let mut proc_info: PROCESS_INFORMATION = Default::default();
    unsafe {
        success = CreateProcessW(
            ptr::null(),
            cmdline.as_ptr() as LPWSTR,
            ptr::null_mut(),
            ptr::null_mut(),
            false as i32,
            EXTENDED_STARTUPINFO_PRESENT,
            ptr::null_mut(),
            cwd.as_ref().map_or_else(ptr::null, |s| s.as_ptr()),
            &mut startup_info_ex.StartupInfo as *mut STARTUPINFOW,
            &mut proc_info as *mut PROCESS_INFORMATION,
        ) > 0;

        if !success {
            panic_shell_spawn();
        }
    }

    let conin = EventedAnonWrite::new(conin);
    let conout = EventedAnonRead::new(conout);

    let child_watcher = ChildExitWatcher::new(proc_info.hProcess).unwrap();
    let agent = Conpty { handle: pty_handle, api };

    Some(Pty {
        handle: super::PtyHandle::Conpty(ConptyHandle::new(agent)),
        conout: super::EventedReadablePipe::Anonymous(conout),
        conin: super::EventedWritablePipe::Anonymous(conin),
        read_token: 0.into(),
        write_token: 0.into(),
        child_event_token: 0.into(),
        child_watcher,
    })
}

// Panic with the last os error as message
fn panic_shell_spawn() {
    panic!("Unable to spawn shell: {}", Error::last_os_error());
}

impl OnResize for ConptyHandle {
    fn on_resize(&mut self, sizeinfo: &SizeInfo) {
        if let Some(coord) = coord_from_sizeinfo(sizeinfo) {
            let result = unsafe { (self.api.ResizePseudoConsole)(self.handle, coord) };
            assert_eq!(result, S_OK);
        }
    }
}

/// Helper to build a COORD from a SizeInfo, returning None in overflow cases.
fn coord_from_sizeinfo(sizeinfo: &SizeInfo) -> Option<COORD> {
    let cols = sizeinfo.cols().0;
    let lines = sizeinfo.lines().0;

    if cols <= i16::MAX as usize && lines <= i16::MAX as usize {
        Some(COORD { X: sizeinfo.cols().0 as i16, Y: sizeinfo.lines().0 as i16 })
    } else {
        None
    }
}
