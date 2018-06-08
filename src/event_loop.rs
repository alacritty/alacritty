//! The main event loop which performs I/O on the pseudoterminal
use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Write};
use std::fs::File;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::marker::{PhantomData, Send};

use mio::{self, Events, PollOpt, Ready};
use mio_more::channel::{self, Receiver, Sender};
#[cfg(unix)]
use mio::unix::UnixReady;
#[cfg(unix)]
use mio::unix::EventedFd;

use ansi;
use display;
use event;
use tty;
use term::Term;
use util::thread;
use sync::FairMutex;

/// Messages that may be sent to the `EventLoop`
#[derive(Debug)]
pub enum Msg {
    /// Data that should be written to the pty
    Input(Cow<'static, [u8]>),

    /// Indicates that the `EventLoop` should shut down, as Alacritty is shutting down
    Shutdown,
}

/// The main event!.. loop.
///
/// Handles all the pty I/O and runs the pty parser which updates terminal
/// state.
#[cfg(windows)]
pub struct EventLoop<R: io::Read + Send, W: io::Write + Send, T: tty::EventedRW<R, W>> {
    poll: mio::Poll,
    pty: T,
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    terminal: Arc<FairMutex<Term>>,
    display: display::Notifier,
    ref_test: bool,
    // FIXME: A possibly better solution is to change the trait (PTY?) to use an associated type
    _r: PhantomData<R>,
    _w: PhantomData<W>,
}
#[cfg(not(windows))]
pub struct EventLoop<R: io::Read, W: io::Write, T: tty::PTY<R, W>> {
    poll: mio::Poll,
    pty: T,
    rx: Reciever<Msg>,
    tx: Sender<Msg>,
    terminal: Arc<FairMutex<Term>>,
    display: display::Notifier,
    ref_test: bool,
}

/// Helper type which tracks how much of a buffer has been written.
struct Writing {
    source: Cow<'static, [u8]>,
    written: usize,
}

/// Indicates the result of draining the mio channel
#[derive(Debug)]
enum DrainResult {
    /// At least one new item was received
    ReceivedItem,
    /// Nothing was available to receive
    Empty,
    /// A shutdown message was received
    Shutdown,
}

impl DrainResult {
    pub fn is_shutdown(&self) -> bool {
        match *self {
            DrainResult::Shutdown => true,
            _ => false,
        }
    }
}

/// All of the mutable state needed to run the event loop
///
/// Contains list of items to write, current write state, etc. Anything that
/// would otherwise be mutated on the `EventLoop` goes here.
pub struct State {
    write_list: VecDeque<Cow<'static, [u8]>>,
    writing: Option<Writing>,
    parser: ansi::Processor,
}

pub struct Notifier(pub Sender<Msg>);

impl event::Notify for Notifier {
    fn notify<B>(&mut self, bytes: B)
    where
        B: Into<Cow<'static, [u8]>>,
    {
        let bytes = bytes.into();
        // terminal hangs if we send 0 bytes through.
        if bytes.len() == 0 {
            return;
        }
        if self.0.send(Msg::Input(bytes)).is_err() {
            panic!("expected send event loop msg");
        }
    }
}

impl Default for State {
    fn default() -> State {
        State {
            write_list: VecDeque::new(),
            parser: ansi::Processor::new(),
            writing: None,
        }
    }
}

impl State {
    #[inline]
    fn ensure_next(&mut self) {
        if self.writing.is_none() {
            self.goto_next();
        }
    }

    #[inline]
    fn goto_next(&mut self) {
        self.writing = self.write_list.pop_front().map(Writing::new);
    }

    #[inline]
    fn take_current(&mut self) -> Option<Writing> {
        self.writing.take()
    }

    #[inline]
    fn needs_write(&self) -> bool {
        self.writing.is_some() || !self.write_list.is_empty()
    }

    #[inline]
    fn set_current(&mut self, new: Option<Writing>) {
        self.writing = new;
    }
}

impl Writing {
    #[inline]
    fn new(c: Cow<'static, [u8]>) -> Writing {
        Writing {
            source: c,
            written: 0,
        }
    }

    #[inline]
    fn advance(&mut self, n: usize) {
        self.written += n;
    }

    #[inline]
    fn remaining_bytes(&self) -> &[u8] {
        &self.source[self.written..]
    }

    #[inline]
    fn finished(&self) -> bool {
        self.written >= self.source.len()
    }
}

/// `mio::Token` for the event loop channel
const CHANNEL: mio::Token = mio::Token(0);

impl<R, W, T> EventLoop<R, W, T>
where
    R: io::Read + Send + 'static,
    W: io::Write + Send + 'static,
    T: tty::EventedRW<R, W> + Send + 'static,
{
    /// Create a new event loop
    pub fn new(
        terminal: Arc<FairMutex<Term>>,
        display: display::Notifier,
        pty: T,
        ref_test: bool,
    ) -> EventLoop<R, W, T> {
        let (tx, rx) = channel::channel();
        EventLoop {
            poll: mio::Poll::new().expect("create mio Poll"),
            pty,
            tx,
            rx,
            terminal,
            display,
            ref_test,
        }
    }

    pub fn channel(&self) -> Sender<Msg> {
        self.tx.clone()
    }

    // Drain the channel
    //
    // Returns a `DrainResult` indicating the result of receiving from the channel
    //
    fn drain_recv_channel(&self, state: &mut State) -> DrainResult {
        let mut received_item = false;
        while let Ok(msg) = self.rx.try_recv() {
            received_item = true;
            match msg {
                Msg::Input(input) => {
                    state.write_list.push_back(input);
                }
                Msg::Shutdown => {
                    return DrainResult::Shutdown;
                }
            }
        }

        if received_item {
            DrainResult::ReceivedItem
        } else {
            DrainResult::Empty
        }
    }

    // Returns a `bool` indicating whether or not the event loop should continue running
    #[inline]
    fn channel_event(&mut self, state: &mut State) -> bool {
        if self.drain_recv_channel(state).is_shutdown() {
            return false;
        }

        let mut interest = Ready::readable();
        if state.needs_write() {
            interest.insert(Ready::writable());
        }
        self.pty
            .reregister(&self.poll, interest, PollOpt::edge() | PollOpt::oneshot());
        // .expect("reregister channel");

        true
    }

    #[inline]
    fn pty_read<X>(
        &mut self,
        state: &mut State,
        buf: &mut [u8],
        mut writer: Option<&mut X>,
    ) -> io::Result<()>
    where
        X: Write,
    {
        const MAX_READ: usize = 0x1_0000;
        let mut processed = 0;
        let mut terminal = None;

        loop {
            match self.pty.reader().read(&mut buf[..]) {
                Ok(0) => break,
                Ok(got) => {
                    // Record bytes read; used to limit time spent in pty_read.
                    processed += got;

                    // Send a copy of bytes read to a subscriber. Used for
                    // example with ref test recording.
                    writer = writer.map(|w| {
                        w.write_all(&buf[..got]).unwrap();
                        w
                    });

                    // Get reference to terminal. Lock is acquired on initial
                    // iteration and held until there's no bytes left to parse
                    // or we've reached MAX_READ.
                    if terminal.is_none() {
                        terminal = Some(self.terminal.lock());
                    }
                    let terminal = terminal.as_mut().unwrap();

                    // Run the parser
                    for byte in &buf[..got] {
                        state
                            .parser
                            .advance(&mut **terminal, *byte, &mut self.pty.writer());
                    }

                    // Exit if we've processed enough bytes
                    if processed > MAX_READ {
                        break;
                    }
                }
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                        break;
                    }
                    _ => return Err(err),
                },
            }
        }

        // Only request a draw if one hasn't already been requested.
        if let Some(mut terminal) = terminal {
            if !terminal.dirty {
                self.display.notify();
                terminal.dirty = true;
            }
        }

        Ok(())
    }

    #[inline]
    fn pty_write(&mut self, state: &mut State) -> io::Result<()> {
        state.ensure_next();

        'write_many: while let Some(mut current) = state.take_current() {
            'write_one: loop {
                match self.pty.writer().write(current.remaining_bytes()) {
                    Ok(0) => {
                        state.set_current(Some(current));
                        break 'write_many;
                    }
                    Ok(n) => {
                        current.advance(n);
                        if current.finished() {
                            state.goto_next();
                            break 'write_one;
                        }
                    }
                    Err(err) => {
                        state.set_current(Some(current));
                        match err.kind() {
                            ErrorKind::Interrupted | ErrorKind::WouldBlock => break 'write_many,
                            _ => return Err(err),
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn spawn(mut self, state: Option<State>) -> thread::JoinHandle<(Self, State)> {
        thread::spawn_named("pty reader", move || {
            let mut state = state.unwrap_or_else(Default::default);
            let mut buf = [0u8; 0x1000];

            let poll_opts = PollOpt::level() | PollOpt::oneshot();
            let tokens: [usize; 2] = [1, 2];

            self.poll
                .register(&self.rx, CHANNEL, Ready::readable(), poll_opts)
                .unwrap();

            // Unix uses a single fd for r/w, Windows uses two named pipes.
            #[cfg(not(windows))]
            let fd = {
                let fd = EventedFd(&self.pty.as_raw_fd());
                self.poll
                    .register(fd, PTY, Ready::readable(), poll_opts)
                    .unwrap();
                fd
            };

            self.pty
                .register(&self.poll, &mut tokens.iter(), Ready::readable(), poll_opts);

            let mut events = Events::with_capacity(1024);

            let mut pipe = if self.ref_test {
                let file =
                    File::create("./alacritty.recording").expect("create alacritty recording");
                Some(file)
            } else {
                None
            };

            'event_loop: loop {
                if let Err(err) = self.poll.poll(&mut events, None) {
                    match err.kind() {
                        ErrorKind::Interrupted => continue,
                        _ => panic!("EventLoop polling error: {:?}", err),
                    }
                }

                for event in events.iter() {
                    match event.token() {
                        CHANNEL => if !self.channel_event(&mut state) {
                            break 'event_loop;
                        },
                        read_token if read_token == self.pty.read_token() => {
                            #[cfg(unix)]
                            {
                                if UnixReady::from(event.readiness()).is_hup() {
                                    break 'event_loop;
                                }
                            }

                            if ready.is_readable() {
                                if let Err(err) = self.pty_read(&mut state, &mut buf, pipe.as_mut()) {
                                    error!("Event loop exiting due to error: {} [{}:{}]",
                                           err, file!(), line!());
                                    break 'event_loop;
                                }

                                if ::tty::process_should_exit() {
                                    break 'event_loop;
                                }
                            }
                        }
                        write_token if write_token == self.pty.write_token() => {
                            #[cfg(unix)]
                            {
                                if UnixReady::from(event.readiness()).is_hup() {
                                    break 'event_loop;
                                }
                            }
                            if event.readiness().is_writable() {
                                if let Err(err) = self.pty_write(&mut state) {
                                    error!("Event loop exiting due to error: {} [{}:{}]",
                                           err, file!(), line!());
                                    break 'event_loop;
                                }
                            }
                        }
                        _ => (),
                    }

                    // Register write interest if necessary
                    let mut interest = Ready::readable();
                    if state.needs_write() {
                        interest.insert(Ready::writable());
                    }
                    // Reregister with new interest
                    self.pty.reregister(&self.poll, interest, poll_opts);
                }
            }

            // The evented instances are not dropped here so deregister them explicitly
            let _ = self.poll.deregister(&self.rx);
            self.pty.deregister(&self.poll);

            (self, state)
        })
    }
}
