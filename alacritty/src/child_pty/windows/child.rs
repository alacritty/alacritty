
use std::io::Error;


use winapi::shared::ntdef::{BOOLEAN, HANDLE, PVOID};
use winapi::um::winbase::{RegisterWaitForSingleObject, INFINITE};
use winapi::um::winnt::{WT_EXECUTEINWAITTHREAD, WT_EXECUTEONLYONCE};

use glutin::event_loop::EventLoopProxy;
use crate::event::Event;
use once_cell::sync::Lazy;
use arc_swap::ArcSwap;
use std::sync::Arc;
use std::boxed::Box;
use std::mem::MaybeUninit;
use std::sync::Mutex;

use log::error;

extern "system" fn child_exit_callback(_ctx: PVOID, timed_out: BOOLEAN) {
    if timed_out == 0 {
        let event_loop_box = EVENT_LOOP_PROXY.load();
        let ea1 = &*event_loop_box;
        let ea2 = &*ea1;
        let event_loop_box_mutex = &*ea2;
        unsafe { 
            let ebox = &*event_loop_box_mutex;
            let ref_box = ebox.as_ptr(); 
            if let Ok(event_loop_guard) = (*ref_box).lock() {
                let event_loop = &*event_loop_guard;
                let event_sent_result = event_loop.send_event(crate::event::Event::TerminalEvent(alacritty_terminal::event::Event::Close));
                match event_sent_result {
                    Ok(_res) => {

                    },
                    Err(e) => {
                        error!("Error occurred sending event to close tab {}", e);
                    }
                };
                drop(event_loop_guard);
            }
        };
    }
}


#[allow(clippy::type_complexity)]
pub static EVENT_LOOP_PROXY: Lazy<ArcSwap<Box<MaybeUninit<Mutex<EventLoopProxy<Event>>>>>> = Lazy::new(|| ArcSwap::from(Arc::new(Box::<Mutex<EventLoopProxy<Event>>>::new_uninit())));

pub struct ChildExitWatcher {
}

impl ChildExitWatcher {
    pub fn new(child_handle: HANDLE) -> Result<ChildExitWatcher, Error> {
        let mut wait_handle: HANDLE = 0 as HANDLE;

        let success = unsafe {
            RegisterWaitForSingleObject(
                &mut wait_handle,
                child_handle,
                Some(child_exit_callback),
                0 as PVOID,
                INFINITE,
                WT_EXECUTEINWAITTHREAD | WT_EXECUTEONLYONCE,
            )
        };

        if success == 0 {
            Err(Error::last_os_error())
        } else {
            Ok(ChildExitWatcher { })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;

    use super::*;

    #[test]
    pub fn event_is_emitted_when_child_exits() {
        let mut child = Command::new("cmd.exe").spawn().unwrap();
        let _child_exit_watcher = ChildExitWatcher::new(child.as_raw_handle()).unwrap();

        child.kill().unwrap();
    }
}
