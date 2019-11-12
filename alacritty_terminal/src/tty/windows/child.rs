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

use mio_extras::channel::{channel, Receiver, Sender};

use winapi::shared::ntdef::{BOOLEAN, HANDLE, PVOID};
use winapi::um::winbase::{RegisterWaitForSingleObject, UnregisterWait, INFINITE};
use winapi::um::winnt::{WT_EXECUTEINWAITTHREAD, WT_EXECUTEONLYONCE};

use crate::tty::ChildEvent;

/// WinAPI callback to run when child process exits.
extern "system" fn child_exit_callback(ctx: PVOID, timed_out: BOOLEAN) {
    if timed_out != 0 {
        return;
    }

    let event_tx: Box<_> = unsafe { Box::from_raw(ctx as *mut Sender<ChildEvent>) };
    let _ = event_tx.send(ChildEvent::Exited);
}

pub struct ChildExitWatcher {
    wait_handle: AtomicPtr<c_void>,
    event_rx: Receiver<ChildEvent>,
}

impl ChildExitWatcher {
    pub fn new(child_handle: HANDLE) -> Result<ChildExitWatcher, Error> {
        let (event_tx, event_rx) = channel::<ChildEvent>();

        let mut wait_handle: HANDLE = 0 as HANDLE;
        let sender_ref = Box::new(event_tx);

        let success = unsafe {
            RegisterWaitForSingleObject(
                &mut wait_handle,
                child_handle,
                Some(child_exit_callback),
                Box::into_raw(sender_ref) as PVOID,
                INFINITE,
                WT_EXECUTEINWAITTHREAD | WT_EXECUTEONLYONCE,
            )
        };

        if success == 0 {
            Err(Error::last_os_error())
        } else {
            Ok(ChildExitWatcher { wait_handle: AtomicPtr::from(wait_handle), event_rx })
        }
    }

    pub fn event_rx(&self) -> &Receiver<ChildEvent> {
        &self.event_rx
    }
}

impl Drop for ChildExitWatcher {
    fn drop(&mut self) {
        unsafe {
            UnregisterWait(self.wait_handle.load(Ordering::Relaxed));
        }
    }
}

#[cfg(test)]
mod test {
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;
    use std::time::Duration;

    use mio::{Events, Poll, PollOpt, Ready, Token};

    use super::*;

    #[test]
    pub fn event_is_emitted_when_child_exits() {
        const WAIT_TIMEOUT: Duration = Duration::from_millis(200);

        let mut child = Command::new("cmd.exe").spawn().unwrap();
        let child_exit_watcher = ChildExitWatcher::new(child.as_raw_handle()).unwrap();

        let mut events = Events::with_capacity(1);
        let poll = Poll::new().unwrap();
        let child_events_token = Token::from(0usize);

        poll.register(
            child_exit_watcher.event_rx(),
            child_events_token,
            Ready::readable(),
            PollOpt::oneshot(),
        )
        .unwrap();

        child.kill().unwrap();

        // Poll for the event or fail with timeout if nothing has been sent
        poll.poll(&mut events, Some(WAIT_TIMEOUT)).unwrap();
        for event in events.iter() {
            assert_eq!(event.token(), child_events_token);

            // Verify that at least one `ChildEvent::Exited` was received
            loop {
                match child_exit_watcher.event_rx().try_recv() {
                    Ok(ChildEvent::Exited) => return, // Success
                    Err(_) => unreachable!("No event {:?} was received", ChildEvent::Exited),
                }
            }
        }
    }
}
