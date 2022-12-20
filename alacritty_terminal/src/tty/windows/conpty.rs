use std::io::{Error, Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle};
use std::{mem, ptr};

use mio_anonymous_pipes::{EventedAnonRead, EventedAnonWrite};

use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{
    DuplicateHandle, SetHandleInformation, DUPLICATE_SAME_ACCESS, HANDLE, HANDLE_FLAG_INHERIT,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, GetCurrentProcess, InitializeProcThreadAttributeList,
    UpdateProcThreadAttribute, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW,
};

use crate::config::PtyConfig;
use crate::event::{OnResize, WindowSize};
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::windows::{cmdline, win32_string, Pty};

/// RAII Pseudoconsole.
pub struct Conpty {
    conin: miow::pipe::AnonWrite,
}

impl Drop for Conpty {
    fn drop(&mut self) {}
}

// The ConPTY handle can be sent between threads.
unsafe impl Send for Conpty {}

pub fn new(config: &PtyConfig, window_size: WindowSize) -> Option<Pty> {
    // Passing 0 as the size parameter allows the "system default" buffer
    // size to be used. There may be small performance and memory advantages
    // to be gained by tuning this in the future, but it's likely a reasonable
    // start point.
    let (conout, conout_pty_handle) = miow::pipe::anonymous(0).unwrap();
    let (conin_pty_handle, mut conin) = miow::pipe::anonymous(0).unwrap();

    let failure = unsafe {
        SetHandleInformation(conout_pty_handle.as_raw_handle() as _, HANDLE_FLAG_INHERIT, 1)
    };

    if failure == 0 {
        panic_shell_spawn();
    }

    let failure = unsafe {
        SetHandleInformation(conin_pty_handle.as_raw_handle() as _, HANDLE_FLAG_INHERIT, 1)
    };

    if failure == 0 {
        panic_shell_spawn();
    }

    let mut success;

    // Prepare child process startup info.

    let mut size: usize = 0;

    let mut startup_info_ex: STARTUPINFOEXW = unsafe { mem::zeroed() };

    startup_info_ex.StartupInfo.lpTitle = std::ptr::null_mut() as PWSTR;

    startup_info_ex.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;

    startup_info_ex.StartupInfo.hStdError = conout_pty_handle.into_raw_handle() as _;
    startup_info_ex.StartupInfo.hStdOutput = startup_info_ex.StartupInfo.hStdError;
    startup_info_ex.StartupInfo.hStdInput = conin_pty_handle.into_raw_handle() as _;

    startup_info_ex.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;

    // Create the appropriately sized thread attribute list.
    unsafe {
        let failure =
            InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size as *mut usize) > 0;

        // This call was expected to return false.
        if failure {
            panic_shell_spawn();
        }
    }

    let mut attr_list: Box<[u8]> = vec![0; size].into_boxed_slice();

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
            &mut size as *mut usize,
        ) > 0;

        if !success {
            panic_shell_spawn();
        }
    }

    let mut inherit_handles =
        vec![startup_info_ex.StartupInfo.hStdOutput, startup_info_ex.StartupInfo.hStdInput];

    // Set thread attribute list's handle list to the new stdin and stdout.
    unsafe {
        success = UpdateProcThreadAttribute(
            startup_info_ex.lpAttributeList,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inherit_handles.as_mut_ptr() as *mut std::ffi::c_void,
            mem::size_of::<HANDLE>() * inherit_handles.len(),
            ptr::null_mut(),
            ptr::null_mut(),
        ) > 0;

        if !success {
            panic_shell_spawn();
        }
    }

    let cmdline = win32_string(&cmdline(config));
    let cwd = config.working_directory.as_ref().map(win32_string);

    let mut proc_info: PROCESS_INFORMATION = unsafe { mem::zeroed() };
    unsafe {
        success = CreateProcessW(
            ptr::null(),
            cmdline.as_ptr() as PWSTR,
            ptr::null_mut(),
            ptr::null_mut(),
            true as i32,
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

    let mut conpty = Conpty { conin: dup_pipe(&conin) };
    conpty.on_resize(window_size);

    let (mut alacout, conin_internal) = miow::pipe::anonymous(0).unwrap();
    std::thread::spawn(move || {
        let mut buffer = [0; 512];
        while let Ok(read) = alacout.read(&mut buffer) {
            let mut start = 0;
            while let Some(next_tick) = buffer[start..read].iter().position(|&x| x == b'`') {
                conin.write_all(&buffer[start..next_tick + 1]).unwrap();
                conin.write_all(b"`").unwrap();
                start += next_tick + 1;
            }
            conin.write_all(&buffer[start..read]).unwrap();
        }
    });

    let conin = EventedAnonWrite::new(conin_internal);
    let conout = EventedAnonRead::new(conout);

    let child_watcher = ChildExitWatcher::new(proc_info.hProcess).unwrap();

    Some(Pty::new(conpty, conout, conin, child_watcher))
}

fn dup_pipe(pipe: &miow::pipe::AnonWrite) -> miow::pipe::AnonWrite {
    unsafe {
        let mut new_handle: HANDLE = mem::zeroed();
        let success = DuplicateHandle(
            GetCurrentProcess(),
            pipe.as_raw_handle() as HANDLE,
            GetCurrentProcess(),
            &mut new_handle as *mut HANDLE,
            0,
            false as i32,
            DUPLICATE_SAME_ACCESS,
        ) != 0;

        if !success {
            panic_shell_spawn();
        }

        miow::pipe::AnonWrite::from_raw_handle(new_handle as _)
    }
}

// Panic with the last os error as message.
fn panic_shell_spawn() {
    panic!("Unable to spawn shell: {}", Error::last_os_error());
}

impl OnResize for Conpty {
    fn on_resize(&mut self, window_size: WindowSize) {
        let pkt = format!("`r{}:{};", window_size.num_cols, window_size.num_lines);
        self.conin.write_all(pkt.as_bytes()).unwrap();
    }
}
