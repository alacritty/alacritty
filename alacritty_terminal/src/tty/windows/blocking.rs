//! Code for running a reader/writer on another thread while driving it through `polling`.

use std::io::prelude::*;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::{io, thread};

use piper::{Reader, Writer, pipe};
use polling::os::iocp::{CompletionPacket, PollerIocpExt};
use polling::{Event, PollMode, Poller};

use crate::thread::spawn_named;

struct Registration {
    interest: Mutex<Option<Interest>>,
    end: PipeEnd,
}

#[derive(Copy, Clone)]
enum PipeEnd {
    Reader,
    Writer,
}

struct Interest {
    /// The event to send about completion.
    event: Event,

    /// The poller to send the event to.
    poller: Arc<Poller>,

    /// The mode that we are in.
    mode: PollMode,
}

/// Poll a reader in another thread.
pub struct UnblockedReader<R> {
    /// The event to send about completion.
    interest: Arc<Registration>,

    /// The pipe that we are reading from.
    pipe: Reader,

    /// Is this the first time registering?
    first_register: bool,

    /// We logically own the reader, but we don't actually use it.
    _reader: PhantomData<R>,
}

impl<R: Read + Send + 'static> UnblockedReader<R> {
    /// Spawn a new unblocked reader.
    pub fn new(mut source: R, pipe_capacity: usize) -> Self {
        // Create a new pipe.
        let (reader, mut writer) = pipe(pipe_capacity);
        let interest = Arc::new(Registration {
            interest: Mutex::<Option<Interest>>::new(None),
            end: PipeEnd::Reader,
        });

        // Spawn the reader thread.
        spawn_named("alacritty-tty-reader-thread", move || {
            let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
            let mut context = Context::from_waker(&waker);

            loop {
                // Read from the reader into the pipe.
                match writer.poll_fill(&mut context, &mut source) {
                    Poll::Ready(Ok(0)) => {
                        // Either the pipe is closed or the reader is at its EOF.
                        // In any case, we are done.
                        return;
                    },

                    Poll::Ready(Ok(_)) => {
                        // Keep reading.
                        continue;
                    },

                    Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {
                        // We were interrupted; continue.
                        continue;
                    },

                    Poll::Ready(Err(e)) => {
                        log::error!("error writing to pipe: {}", e);
                        return;
                    },

                    Poll::Pending => {
                        // We are now waiting on the other end to advance. Park the
                        // thread until they do.
                        thread::park();
                    },
                }
            }
        });

        Self { interest, pipe: reader, first_register: true, _reader: PhantomData }
    }

    /// Register interest in the reader.
    pub fn register(&mut self, poller: &Arc<Poller>, event: Event, mode: PollMode) {
        let mut interest = self.interest.interest.lock().unwrap();
        *interest = Some(Interest { event, poller: poller.clone(), mode });

        // Send the event to start off with if we have any data.
        if (!self.pipe.is_empty() && event.readable) || self.first_register {
            self.first_register = false;
            poller.post(CompletionPacket::new(event)).ok();
        }
    }

    /// Deregister interest in the reader.
    pub fn deregister(&self) {
        let mut interest = self.interest.interest.lock().unwrap();
        *interest = None;
    }

    /// Try to read from the reader.
    pub fn try_read(&mut self, buf: &mut [u8]) -> usize {
        let waker = Waker::from(self.interest.clone());

        match self.pipe.poll_drain_bytes(&mut Context::from_waker(&waker), buf) {
            Poll::Pending => 0,
            Poll::Ready(n) => {
                // A successful drain removes piper's reader waker. The PTY
                // loop can stop at its read budget before polling an empty
                // pipe, so keep level readiness asserted until it does.
                // That final empty poll also arms notification for new data.
                if n > 0 {
                    self.interest.wake_by_ref();
                }
                n
            },
        }
    }
}

impl<R: Read + Send + 'static> Read for UnblockedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Ok(self.try_read(buf))
    }
}

/// Poll a writer in another thread.
pub struct UnblockedWriter<W> {
    /// The interest to send about completion.
    interest: Arc<Registration>,

    /// The pipe that we are writing to.
    pipe: Writer,

    /// We logically own the writer, but we don't actually use it.
    _reader: PhantomData<W>,
}

impl<W: Write + Send + 'static> UnblockedWriter<W> {
    /// Spawn a new unblocked writer.
    pub fn new(mut sink: W, pipe_capacity: usize) -> Self {
        // Create a new pipe.
        let (mut reader, writer) = pipe(pipe_capacity);
        let interest = Arc::new(Registration {
            interest: Mutex::<Option<Interest>>::new(None),
            end: PipeEnd::Writer,
        });

        // Spawn the writer thread.
        spawn_named("alacritty-tty-writer-thread", move || {
            let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
            let mut context = Context::from_waker(&waker);

            loop {
                // Write from the pipe into the writer.
                match reader.poll_drain(&mut context, &mut sink) {
                    Poll::Ready(Ok(0)) => {
                        // Either the pipe is closed or the writer is full.
                        // In any case, we are done.
                        return;
                    },

                    Poll::Ready(Ok(_)) => {
                        // Keep writing.
                        continue;
                    },

                    Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {
                        // We were interrupted; continue.
                        continue;
                    },

                    Poll::Ready(Err(e)) => {
                        log::error!("error writing to pipe: {}", e);
                        return;
                    },

                    Poll::Pending => {
                        // We are now waiting on the other end to advance. Park the
                        // thread until they do.
                        thread::park();
                    },
                }
            }
        });

        Self { interest, pipe: writer, _reader: PhantomData }
    }

    /// Register interest in the writer.
    pub fn register(&self, poller: &Arc<Poller>, event: Event, mode: PollMode) {
        let mut interest = self.interest.interest.lock().unwrap();
        *interest = Some(Interest { event, poller: poller.clone(), mode });

        // Send the event to start off with if we have room for data.
        if !self.pipe.is_full() && event.writable {
            poller.post(CompletionPacket::new(event)).ok();
        }
    }

    /// Deregister interest in the writer.
    pub fn deregister(&self) {
        let mut interest = self.interest.interest.lock().unwrap();
        *interest = None;
    }

    /// Try to write to the writer.
    pub fn try_write(&mut self, buf: &[u8]) -> usize {
        let waker = Waker::from(self.interest.clone());

        match self.pipe.poll_fill_bytes(&mut Context::from_waker(&waker), buf) {
            Poll::Pending => 0,
            Poll::Ready(n) => n,
        }
    }
}

impl<W: Write + Send + 'static> Write for UnblockedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(self.try_write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        // Nothing to flush.
        Ok(())
    }
}

struct ThreadWaker(thread::Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

impl Wake for Registration {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let mut interest_lock = self.interest.lock().unwrap();
        if let Some(interest) = interest_lock.as_ref() {
            // Send the event to the poller.
            let send_event = match self.end {
                PipeEnd::Reader => interest.event.readable,
                PipeEnd::Writer => interest.event.writable,
            };

            if send_event {
                interest.poller.post(CompletionPacket::new(interest.event)).ok();

                // Clear the event if we're in oneshot mode.
                if matches!(interest.mode, PollMode::Oneshot | PollMode::EdgeOneshot) {
                    *interest_lock = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::mpsc::{self, Receiver};
    use std::time::{Duration, Instant};

    use polling::Events;

    use super::*;

    struct ChannelReader(Receiver<Vec<u8>>);

    impl Read for ChannelReader {
        fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
            if dest.is_empty() {
                return Ok(0);
            }
            let Ok(bytes) = self.0.recv() else { return Ok(0) };
            assert!(bytes.len() <= dest.len());
            dest[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
    }

    fn wait_readable(poller: &Poller) {
        let mut events = Events::with_capacity(NonZeroUsize::new(8).unwrap());
        poller.wait(&mut events, Some(Duration::from_secs(1))).unwrap();
        assert!(
            events.iter().any(|event| event.key == 42 && event.readable),
            "buffered PTY output lost its readable notification"
        );
    }

    #[test]
    fn successful_read_keeps_level_readiness_until_empty() {
        let (sender, receiver) = mpsc::channel();
        let mut reader = UnblockedReader::new(ChannelReader(receiver), 64);
        let poller = Arc::new(Poller::new().unwrap());
        reader.register(&poller, Event::readable(42), PollMode::Level);

        // Consume the synthetic initial event and arm the pipe's empty-reader waker.
        wait_readable(&poller);
        assert_eq!(reader.read(&mut [0; 1]).unwrap(), 0);
        sender.send(vec![b'x'; 16]).unwrap();

        // A successful partial read clears piper's waker. The terminal event loop
        // can stop at its read budget here, with bytes still waiting in the pipe.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut byte = [0; 1];
        loop {
            assert!(Instant::now() < deadline, "reader did not deliver output");
            wait_readable(&poller);
            if reader.read(&mut byte).unwrap() == 1 {
                break;
            }
        }
        assert_eq!(byte, [b'x']);
        assert_eq!(reader.pipe.len(), 15);

        // This must not require keyboard input or another PTY write.
        wait_readable(&poller);
        drop(sender);
    }
}
