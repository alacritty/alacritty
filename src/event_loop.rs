//! The main event loop which performs I/O on the pseudoterminal
use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Write};
use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;

use mio::{self, Events, PollOpt, Ready};
use mio::unix::EventedFd;

use ansi;
use display;
use term::Term;
use util::thread;
use sync::FairMutex;

/// Messages that may be sent to the `EventLoop`
#[derive(Debug)]
pub enum Msg {
    /// Data that should be written to the pty
    Input(Cow<'static, [u8]>),
}

/// The main event!.. loop.
///
/// Handles all the pty I/O and runs the pty parser which updates terminal
/// state.
pub struct EventLoop<Io> {
    poll: mio::Poll,
    pty: Io,
    rx: mio::channel::Receiver<Msg>,
    tx: mio::channel::Sender<Msg>,
    terminal: Arc<FairMutex<Term>>,
    display: display::Notifier,
    ref_test: bool,
}

/// Helper type which tracks how much of a buffer has been written.
struct Writing {
    source: Cow<'static, [u8]>,
    written: usize,
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
        self.writing = self.write_list
            .pop_front()
            .map(Writing::new);
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
        Writing { source: c, written: 0 }
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

/// `mio::Token` for the pty file descriptor
const PTY: mio::Token = mio::Token(1);

impl<Io> EventLoop<Io>
    where Io: io::Read + io::Write + Send + AsRawFd + 'static
{
    /// Create a new event loop
    pub fn new(
        terminal: Arc<FairMutex<Term>>,
        display: display::Notifier,
        pty: Io,
        ref_test: bool,
    ) -> EventLoop<Io> {
        let (tx, rx) = ::mio::channel::channel();
        EventLoop {
            poll: mio::Poll::new().expect("create mio Poll"),
            pty: pty,
            tx: tx,
            rx: rx,
            terminal: terminal,
            display: display,
            ref_test: ref_test,
        }
    }

    pub fn channel(&self) -> mio::channel::Sender<Msg> {
        self.tx.clone()
    }

    #[inline]
    fn channel_event(&mut self, state: &mut State) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Input(input) => {
                    state.write_list.push_back(input);
                }
            }
        }

        self.poll.reregister(
            &self.rx, CHANNEL,
            Ready::readable(),
            PollOpt::edge() | PollOpt::oneshot()
        ).expect("reregister channel");

        if state.needs_write() {
            self.poll.reregister(
                &EventedFd(&self.pty.as_raw_fd()),
                PTY,
                Ready::readable() | Ready::writable(),
                PollOpt::edge() | PollOpt::oneshot()
            ).expect("reregister fd after channel recv");
        }
    }

    #[inline]
    fn pty_read<W>(
        &mut self,
        state: &mut State,
        buf: &mut [u8],
        mut writer: Option<&mut W>
    )
        where W: Write
    {
        loop {
            match self.pty.read(&mut buf[..]) {
                Ok(0) => break,
                Ok(got) => {
                    writer = writer.map(|w| {
                        w.write_all(&buf[..got]).unwrap(); w
                    });

                    let mut terminal = self.terminal.lock();
                    for byte in &buf[..got] {
                        state.parser.advance(&mut *terminal, *byte, &mut self.pty);
                    }

                    terminal.dirty = true;

                    self.display.notify();
                },
                Err(err) => {
                    match err.kind() {
                        ErrorKind::WouldBlock => break,
                        _ => panic!("unexpected read err: {:?}", err),
                    }
                }
            }
        }
    }

    #[inline]
    fn pty_write(&mut self, state: &mut State) {
        state.ensure_next();

        'write_many: while let Some(mut current) = state.take_current() {
            'write_one: loop {
                match self.pty.write(current.remaining_bytes()) {
                    Ok(0) => {
                        state.set_current(Some(current));
                        break 'write_many;
                    },
                    Ok(n) => {
                        current.advance(n);
                        if current.finished() {
                            state.goto_next();
                            break 'write_one;
                        }
                    },
                    Err(err) => {
                        state.set_current(Some(current));
                        match err.kind() {
                            ErrorKind::WouldBlock => break 'write_many,
                            // TODO
                            _ => panic!("unexpected err: {:?}", err),
                        }
                    }
                }

            }
        }
    }

    pub fn spawn(
        mut self,
        state: Option<State>
    ) -> thread::JoinHandle<(EventLoop<Io>, State)> {
        thread::spawn_named("pty reader", move || {
            let mut state = state.unwrap_or_else(Default::default);
            let mut buf = [0u8; 4096];

            let fd = self.pty.as_raw_fd();
            let fd = EventedFd(&fd);

            let poll_opts = PollOpt::edge() | PollOpt::oneshot();

            self.poll.register(&self.rx, CHANNEL, Ready::readable(), poll_opts).unwrap();
            self.poll.register(&fd, PTY, Ready::readable(), poll_opts).unwrap();

            let mut events = Events::with_capacity(1024);

            let mut pipe = if self.ref_test {
                let file = File::create("./alacritty.recording")
                    .expect("create alacritty recording");
                Some(file)
            } else {
                None
            };

            'event_loop: loop {
                self.poll.poll(&mut events, None).expect("poll ok");

                for event in events.iter() {
                    match event.token() {
                        CHANNEL => self.channel_event(&mut state),
                        PTY => {
                            let kind = event.kind();

                            if kind.is_readable() {
                                self.pty_read(&mut state, &mut buf, pipe.as_mut());
                                if ::tty::process_should_exit() {
                                    break 'event_loop;
                                }
                            }

                            if kind.is_writable() {
                                self.pty_write(&mut state);
                            }

                            if kind.is_hup() {
                                break 'event_loop;
                            }

                            // Figure out pty interest
                            let mut interest = Ready::readable();
                            if state.needs_write() {
                                interest.insert(Ready::writable());
                            }

                            // Reregister pty
                            self.poll
                                .reregister(&fd, PTY, interest, poll_opts)
                                .expect("register fd after read/write");
                        },
                        _ => (),
                    }
                }
            }

            let _ = self.poll.deregister(&self.rx);
            let _ = self.poll.deregister(&fd);

            (self, state)
        })
    }
}
