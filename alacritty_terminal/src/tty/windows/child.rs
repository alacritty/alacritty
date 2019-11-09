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

use std::ffi::c_void;
use std::io::Error;
use std::sync::atomic::{AtomicPtr, Ordering};

use mio_extras::channel::{channel, Receiver};

use winapi::shared::ntdef::{BOOLEAN, HANDLE, PVOID};
use winapi::um::winbase::{RegisterWaitForSingleObject, UnregisterWait, INFINITE};
use winapi::um::winnt::{WT_EXECUTEINWAITTHREAD, WT_EXECUTEONLYONCE};

use crate::tty::ChildEvent;

/// WinAPI callback for `HandleWaitSignal`, unpacks Rust closure reference and calls it.
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

/// Represents closure attached to Win32 wait signal.
///
/// This allows firing a Rust callback when the subprocess exits.
pub(crate) struct WaitSignalHandler {
    wait_handle: AtomicPtr<c_void>,
}

impl WaitSignalHandler {
    /// Registers an asynchronous closure to call when process under `child_handle` exits.
    ///
    /// The `on_exit` is called on Win32 threadpool thread so it should avoid
    /// blocking calls. See [`WT_EXECUTEINWAITTHREAD` flag docs] for details.
    ///
    /// [`WT_EXECUTEINWAITTHREAD` flag docs]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-registerwaitforsingleobject
    fn new<F>(child_handle: HANDLE, on_exit: F) -> Result<WaitSignalHandler, Error>
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

        if success == 0 {
            Err(Error::last_os_error())
        } else {
            Ok(WaitSignalHandler { wait_handle: AtomicPtr::from(wait_handle) })
        }
    }
}

impl Drop for WaitSignalHandler {
    fn drop(&mut self) {
        unsafe {
            UnregisterWait(self.wait_handle.load(Ordering::Relaxed));
        }
    }
}

pub struct ChildProcessWatcher {
    _on_exit: WaitSignalHandler,
    event_rx: Receiver<ChildEvent>,
}

impl ChildProcessWatcher {
    pub fn new(subprocess_handle: HANDLE) -> Result<ChildProcessWatcher, Error> {
        let (sender, receiver) = channel();
        let on_exit = WaitSignalHandler::new(subprocess_handle, move || {
            let _ = sender.send(ChildEvent::Exited);
        })?;

        Ok(ChildProcessWatcher { _on_exit: on_exit, event_rx: receiver })
    }

    pub fn events_rx(&self) -> &Receiver<ChildEvent> {
        &self.event_rx
    }
}

#[cfg(test)]
mod test {
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    use super::*;

    #[test]
    pub fn on_handle_wait_runs_callback_when_process_exits() {
        const WAIT_TIMEOUT: Duration = Duration::from_millis(200);

        let (sender, receiver) = channel::<()>();
        let mut command = Command::new("cmd.exe");
        let mut child = command.spawn().unwrap();
        let subprocess_handle = child.as_raw_handle();

        // Setup exit handler to send an empty message through the channel.
        let _exit_handler = WaitSignalHandler::new(subprocess_handle, move || {
            sender.send(()).unwrap();
        })
        .unwrap();

        child.kill().unwrap();

        // Wait for the message on the channel or time-out if the message has not been sent.
        receiver.recv_timeout(WAIT_TIMEOUT).unwrap();
    }
}
