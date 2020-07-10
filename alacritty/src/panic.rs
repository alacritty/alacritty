use alacritty_terminal::tty::windows::win32_string;

// Install a panic handler that renders the panic in a classical Windows error
// dialog box as well as writes the panic to STDERR.
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
