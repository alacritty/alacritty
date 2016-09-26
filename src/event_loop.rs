//! The main event loop which performs I/O on the pseudoterminal
use std::borrow::Cow;
use std::io::{self, ErrorKind};
use std::os::unix::io::AsRawFd;
use std::sync::Arc;

use mio::{self, Events, PollOpt, Ready};
use mio::unix::EventedFd;

use ansi;
use term::Term;
use util::thread;
use sync::FairMutex;

use super::Flag;

pub struct EventLoop<Io> {
    poll: mio::Poll,
    pty: Io,
    rx: mio::channel::Receiver<Msg>,
    tx: mio::channel::Sender<Msg>,
    terminal: Arc<FairMutex<Term>>,
    proxy: ::glutin::WindowProxy,
    signal_flag: Flag,
}


#[derive(Debug)]
pub enum Msg {
    Input(Cow<'static, [u8]>),
}

const CHANNEL: mio::Token = mio::Token(0);
const PTY: mio::Token = mio::Token(1);

impl<Io> EventLoop<Io>
    where Io: io::Read + io::Write + Send + AsRawFd + 'static
{
    pub fn new(
        terminal: Arc<FairMutex<Term>>,
        proxy: ::glutin::WindowProxy,
        signal_flag: Flag,
        pty: Io,
    ) -> EventLoop<Io> {
        let (tx, rx) = ::mio::channel::channel();
        EventLoop {
            poll: mio::Poll::new().expect("create mio Poll"),
            pty: pty,
            tx: tx,
            rx: rx,
            terminal: terminal,
            proxy: proxy,
            signal_flag: signal_flag
        }
    }

    pub fn channel(&self) -> mio::channel::Sender<Msg> {
        self.tx.clone()
    }

    pub fn spawn(self) -> thread::JoinHandle<()> {

        struct Writing {
            source: Cow<'static, [u8]>,
            written: usize,
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

        thread::spawn_named("pty reader", move || {

            let EventLoop { poll, mut pty, rx, terminal, proxy, signal_flag, .. } = self;


            let mut buf = [0u8; 4096];
            let mut pty_parser = ansi::Processor::new();
            let fd = pty.as_raw_fd();
            let fd = EventedFd(&fd);

            poll.register(&rx, CHANNEL, Ready::readable(), PollOpt::edge() | PollOpt::oneshot())
                .unwrap();
            poll.register(&fd, PTY, Ready::readable(), PollOpt::edge() | PollOpt::oneshot())
                .unwrap();

            let mut events = Events::with_capacity(1024);
            let mut write_list = ::std::collections::VecDeque::new();
            let mut writing = None;

            'event_loop: loop {
                poll.poll(&mut events, None).expect("poll ok");

                for event in events.iter() {
                    match event.token() {
                        CHANNEL => {
                            while let Ok(msg) = rx.try_recv() {
                                match msg {
                                    Msg::Input(input) => {
                                        write_list.push_back(input);
                                    }
                                }
                            }

                            poll.reregister(
                                &rx, CHANNEL,
                                Ready::readable(),
                                PollOpt::edge() | PollOpt::oneshot()
                            ).expect("reregister channel");

                            if writing.is_some() || !write_list.is_empty() {
                                poll.reregister(
                                    &fd,
                                    PTY,
                                    Ready::readable() | Ready::writable(),
                                    PollOpt::edge() | PollOpt::oneshot()
                                ).expect("reregister fd after channel recv");
                            }
                        },
                        PTY => {
                            let kind = event.kind();

                            if kind.is_readable() {
                                loop {
                                    match pty.read(&mut buf[..]) {
                                        Ok(0) => break,
                                        Ok(got) => {
                                            let mut terminal = terminal.lock();
                                            for byte in &buf[..got] {
                                                pty_parser.advance(&mut *terminal, *byte);
                                            }

                                            terminal.dirty = true;

                                            // Only wake up the event loop if it hasn't already been
                                            // signaled. This is a really important optimization
                                            // because waking up the event loop redundantly burns *a
                                            // lot* of cycles.
                                            if !signal_flag.get() {
                                                proxy.wakeup_event_loop();
                                                signal_flag.set(true);
                                            }
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

                            if kind.is_writable() {
                                if writing.is_none() {
                                    writing = write_list
                                        .pop_front()
                                        .map(|c| Writing::new(c));
                                }

                                'write_list_loop: while let Some(mut write_now) = writing.take() {
                                    loop {
                                        match pty.write(write_now.remaining_bytes()) {
                                            Ok(0) => {
                                                writing = Some(write_now);
                                                break 'write_list_loop;
                                            },
                                            Ok(n) => {
                                                write_now.advance(n);
                                                if write_now.finished() {
                                                    writing = write_list
                                                        .pop_front()
                                                        .map(|next| Writing::new(next));

                                                    break;
                                                } else {
                                                }
                                            },
                                            Err(err) => {
                                                writing = Some(write_now);
                                                match err.kind() {
                                                    ErrorKind::WouldBlock => break 'write_list_loop,
                                                    // TODO
                                                    _ => panic!("unexpected err: {:?}", err),
                                                }
                                            }
                                        }

                                    }
                                }
                            }

                            if kind.is_hup() {
                                break 'event_loop;
                            }

                            let mut interest = Ready::readable();
                            if writing.is_some() || !write_list.is_empty() {
                                interest.insert(Ready::writable());
                            }

                            poll.reregister(&fd, PTY, interest, PollOpt::edge() | PollOpt::oneshot())
                                .expect("register fd after read/write");
                        },
                        _ => (),
                    }
                }
            }

            println!("pty reader stopped");
        })
    }
}
