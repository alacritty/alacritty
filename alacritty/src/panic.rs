use std::io::Write;
use std::{io, panic, ptr};

use windows_sys::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TASKMODAL, MessageBoxW,
};

use alacritty_terminal::tty::windows::win32_string;

// Install a panic handler that renders the panic in a classical Windows error
// dialog box as well as writes the panic to STDERR.
pub fn attach_handler() {
    panic::set_hook(Box::new(|panic_info| {
        let _ = writeln!(io::stderr(), "{}", panic_info);
        let msg = format!("{}\n\nPress Ctrl-C to Copy", panic_info);
        unsafe {
            MessageBoxW(
                ptr::null_mut(),
                win32_string(&msg).as_ptr(),
                win32_string("Alacritty: Runtime Error").as_ptr(),
                MB_ICONERROR | MB_OK | MB_SETFOREGROUND | MB_TASKMODAL,
            );
        }
    }));
}
