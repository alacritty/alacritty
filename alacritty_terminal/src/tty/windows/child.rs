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

use std::io::Error;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use log::warn;

use mio_extras::channel::{channel, Receiver};

use winapi::shared::ntdef::{BOOLEAN, HANDLE, PVOID};
use winapi::um::winbase::{RegisterWaitForSingleObject, UnregisterWait, INFINITE};
use winapi::um::winnt::{WT_EXECUTEINWAITTHREAD, WT_EXECUTEONLYONCE};

use crate::tty::ChildEvent;

/// WinAPI callback for `HandleWaitSignal`, unpacks Rust closure reference and calls it
extern "system" fn child_exit_callback<F>(ctx: PVOID, timed_out: BOOLEAN)
where
    F: FnOnce() + Send,
{
    if timed_out != 0 {
        return;
    }

    let callback: Box<F> = unsafe { Box::from_raw(ctx as *mut F) };
    callback();
}

/// Represents closure attached to Win32 handle wait signal
///
/// This allows to fire a Rust callback when subprocess exits
pub(crate) struct HandleWaitSignal {
    wait_handle: AtomicPtr<c_void>,
}

impl HandleWaitSignal {
    /// Registers an asynchronous closure to call when process under `child_handle` exits.
    ///
    /// The `on_exit` is called on Win32 threadpool thread so it should avoid
    /// blocking calls. See [`WT_EXECUTEINWAITTHREAD` flag docs] for details.
    ///
    /// [`WT_EXECUTEINWAITTHREAD` flag docs]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-registerwaitforsingleobject
    fn new<F>(child_handle: HANDLE, on_exit: F) -> Result<HandleWaitSignal, Error>
    where
        F: FnOnce() + Send,
    {
        let mut wait_handle: HANDLE = 0 as HANDLE;
        let wait_notify = Box::new(on_exit);

        let success = unsafe {
            RegisterWaitForSingleObject(
                &mut wait_handle,
                child_handle,
                Some(child_exit_callback::<F>),
                Box::into_raw(wait_notify) as PVOID,
                INFINITE,
                WT_EXECUTEINWAITTHREAD | WT_EXECUTEONLYONCE,
            )
        };

        if 0 == success {
            Err(Error::last_os_error())
        } else {
            Ok(HandleWaitSignal { wait_handle: AtomicPtr::from(wait_handle) })
        }
    }
}

impl Drop for HandleWaitSignal {
    fn drop(&mut self) {
        unsafe {
            // Cancel WinAPI wait for child process exiting
            UnregisterWait(self.wait_handle.load(Ordering::Relaxed));
        }
    }
}

pub(crate) struct ChildProcessState {
    _on_exit: HandleWaitSignal,
    events: Receiver<ChildEvent>,
}

impl ChildProcessState {
    pub fn new(subprocess_handle: HANDLE) -> Result<ChildProcessState, Error> {
        let (sender, receiver) = channel();
        let on_exit = HandleWaitSignal::new(subprocess_handle, move || {
            if let Err(e) = sender.send(ChildEvent::Exited) {
                warn!(
                    "An error occurred while attempting to notify about child process z
                    termination: {}",
                    e
                );
            }
        })?;

        Ok(ChildProcessState { _on_exit: on_exit, events: receiver })
    }

    pub fn events(&self) -> &Receiver<ChildEvent> {
        &self.events
    }
}

#[cfg(test)]
mod test {
    use std::io::Error;
    use std::ptr;
    use std::time::Duration;

    use std::sync::mpsc::channel;

    use widestring::U16CString;

    use winapi::shared::ntdef::LPWSTR;
    use winapi::um::processthreadsapi::{
        CreateProcessW, TerminateProcess, PROCESS_INFORMATION, STARTUPINFOW,
    };

    use super::*;

    fn make_cmd_process() -> Result<HANDLE, Error> {
        let mut pi: PROCESS_INFORMATION = Default::default();
        let mut si: STARTUPINFOW = Default::default();
        let cmdline = U16CString::from_str("cmd.exe").unwrap();

        unsafe {
            if 0 == CreateProcessW(
                ptr::null(),
                cmdline.as_ptr() as LPWSTR,
                ptr::null_mut(),
                ptr::null_mut(),
                false as i32,
                Default::default(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut si as *mut STARTUPINFOW,
                &mut pi as *mut PROCESS_INFORMATION,
            ) {
                Err(Error::last_os_error())
            } else {
                Ok(pi.hProcess)
            }
        }
    }

    // TODO: This is more of an integration test since it spawns new 'cmd.exe'
    //       process and heavily uses Win32 API. Should this be ignored
    //       by default? Worst-case, this will timeout after `WAIT_TIMEOUT`.
    #[test]
    pub fn on_handle_wait_completed_signalls() {
        const WAIT_TIMEOUT: Duration = Duration::from_millis(200);

        let (sender, receiver) = channel::<()>();
        let subprocess_handle = make_cmd_process().unwrap();

        // Setup callback to signal Condvar when process exits
        let _hnd_wait = HandleWaitSignal::new(subprocess_handle, move || {
            sender.send(()).unwrap();
        })
        .unwrap();

        // Kill the subprocess
        let kill_succeeded = unsafe { TerminateProcess(subprocess_handle, 0) };
        assert!(kill_succeeded > 0, "Couldn't kill the process");

        // Wait for condvar to be signalled by OS-thread or fail with time out
        receiver.recv_timeout(WAIT_TIMEOUT).unwrap();
    }
}
