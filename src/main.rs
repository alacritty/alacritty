// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
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
//
//! Alacritty - The GPU Enhanced Terminal
#![feature(question_mark)]
#![feature(range_contains)]
#![feature(inclusive_range_syntax)]
#![feature(drop_types_in_const)]
#![feature(unicode)]
#![feature(step_trait)]
#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]
#![feature(core_intrinsics)]

extern crate cgmath;
extern crate errno;
extern crate font;
extern crate glutin;
extern crate libc;
extern crate mio;
extern crate notify;
extern crate parking_lot;
extern crate serde;
extern crate serde_yaml;
extern crate vte;

#[macro_use]
extern crate bitflags;

#[macro_use]
mod macros;

mod renderer;
pub mod grid;
mod meter;
pub mod config;
mod input;
mod index;
mod event;
mod tty;
pub mod ansi;
mod term;
mod util;
mod sync;

use std::sync::{mpsc, Arc};
use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::{MutexGuard};

use config::Config;
use meter::Meter;
use renderer::{QuadRenderer, GlyphCache};
use sync::FairMutex;
use term::Term;
use tty::process_should_exit;
use util::thread;

/// Channel used by resize handling on mac
static mut resize_sender: Option<mpsc::Sender<(u32, u32)>> = None;

#[derive(Clone)]
struct Flag(Arc<AtomicBool>);
impl Flag {
    pub fn new(initial_value: bool) -> Flag {
        Flag(Arc::new(AtomicBool::new(initial_value)))
    }

    #[inline]
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    #[inline]
    pub fn set(&self, value: bool) {
        self.0.store(value, Ordering::Release)
    }
}

/// Resize handling for Mac
fn window_resize_handler(width: u32, height: u32) {
    unsafe {
        if let Some(ref tx) = resize_sender {
            let _ = tx.send((width, height));
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
pub struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

mod gl {
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}

fn main() {
    // Load configuration
    let config = match Config::load() {
        Err(err) => match err {
            // Use default config when not found
            config::Error::NotFound => Config::default(),
            // Exit when there's a problem with it
            _ => die!("{}", err),
        },
        Ok(config) => config,
    };

    let font = config.font();
    let dpi = config.dpi();
    let render_timer = config.render_timer();

    let mut window = glutin::WindowBuilder::new()
                                           .with_vsync()
                                           .with_title("Alacritty")
                                           .build().unwrap();

    window.set_window_resize_callback(Some(window_resize_handler as fn(u32, u32)));

    gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);
    let (width, height) = window.get_inner_size_pixels().unwrap();
    let dpr = window.hidpi_factor();

    println!("device_pixel_ratio: {}", dpr);

    let _ = unsafe { window.make_current() };
    unsafe {
        gl::Viewport(0, 0, width as i32, height as i32);
        gl::Enable(gl::BLEND);
        gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
        gl::Enable(gl::MULTISAMPLE);
    }

    let rasterizer = font::Rasterizer::new(dpi.x(), dpi.y(), dpr);

    // Create renderer
    let mut renderer = QuadRenderer::new(width, height);

    // Initialize glyph cache
    let glyph_cache = {
        println!("Initializing glyph cache");
        let init_start = ::std::time::Instant::now();

        let cache = renderer.with_loader(|mut api| {
            GlyphCache::new(rasterizer, &config, &mut api)
        });

        let stop = init_start.elapsed();
        let stop_f = stop.as_secs() as f64 + stop.subsec_nanos() as f64 / 1_000_000_000f64;
        println!("Finished initializing glyph cache in {}", stop_f);

        cache
    };

    let metrics = glyph_cache.font_metrics();
    let cell_width = (metrics.average_advance + font.offset().x() as f64) as u32;
    let cell_height = (metrics.line_height + font.offset().y() as f64) as u32;

    println!("Cell Size: ({} x {})", cell_width, cell_height);

    let terminal = Term::new(width as f32, height as f32, cell_width as f32, cell_height as f32);
    let pty_io = terminal.tty().reader();

    let (tx, rx) = mpsc::channel();
    unsafe {
        resize_sender = Some(tx.clone());
    }

    let signal_flag = Flag::new(false);


    let terminal = Arc::new(FairMutex::new(terminal));
    let window = Arc::new(window);

    let event_loop = EventLoop::new(
        terminal.clone(),
        window.create_window_proxy(),
        signal_flag.clone(),
        pty_io,
    );

    let loop_tx = event_loop.channel();
    let event_loop_handle = event_loop.spawn();

    // Wraps a renderer and gives simple draw() api.
    let mut display = Display::new(
        window.clone(),
        renderer,
        glyph_cache,
        render_timer,
        rx
    );

    // Event processor
    let mut processor = event::Processor::new(
        input::LoopNotifier(loop_tx),
        terminal.clone(),
        tx
    );

    // Main loop
    loop {
        // Wait for something to happen
        processor.process_events(&window);

        // Maybe draw the terminal
        let terminal = terminal.lock();
        signal_flag.set(false);
        if terminal.dirty {
            display.draw(terminal);
        }

        if process_should_exit() {
            break;
        }
    }

    // shutdown
    event_loop_handle.join().ok();
    println!("Goodbye");
}

struct EventLoop {
    poll: mio::Poll,
    pty: File,
    rx: mio::channel::Receiver<EventLoopMessage>,
    tx: mio::channel::Sender<EventLoopMessage>,
    terminal: Arc<FairMutex<Term>>,
    proxy: ::glutin::WindowProxy,
    signal_flag: Flag,
}

const CHANNEL: mio::Token = mio::Token(0);
const PTY: mio::Token = mio::Token(1);

#[derive(Debug)]
pub enum EventLoopMessage {
    Input(::std::borrow::Cow<'static, [u8]>),
}

impl EventLoop {
    pub fn new(
        terminal: Arc<FairMutex<Term>>,
        proxy: ::glutin::WindowProxy,
        signal_flag: Flag,
        pty: File,
    ) -> EventLoop {
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

    pub fn channel(&self) -> mio::channel::Sender<EventLoopMessage> {
        self.tx.clone()
    }

    pub fn spawn(mut self) -> std::thread::JoinHandle<()> {
        use mio::{Events, PollOpt, Ready};
        use mio::unix::EventedFd;
        use std::borrow::Cow;
        use std::os::unix::io::AsRawFd;
        use std::io::{Read, Write, ErrorKind};

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

            let EventLoop { mut poll, mut pty, rx, terminal, proxy, signal_flag, .. } = self;


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
                                    EventLoopMessage::Input(input) => {
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
                                        let start = write_now.written;
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

struct Display {
    window: Arc<glutin::Window>,
    renderer: QuadRenderer,
    glyph_cache: GlyphCache,
    render_timer: bool,
    rx: mpsc::Receiver<(u32, u32)>,
    meter: Meter,
}

impl Display {
    pub fn new(window: Arc<glutin::Window>,
               renderer: QuadRenderer,
               glyph_cache: GlyphCache,
               render_timer: bool,
               rx: mpsc::Receiver<(u32, u32)>)
               -> Display
    {
        Display {
            window: window,
            renderer: renderer,
            glyph_cache: glyph_cache,
            render_timer: render_timer,
            rx: rx,
            meter: Meter::new(),
        }
    }

    /// Draw the screen
    ///
    /// A reference to Term whose state is being drawn must be provided.
    ///
    /// This call may block if vsync is enabled
    pub fn draw(&mut self, mut terminal: MutexGuard<Term>) {
        terminal.dirty = false;

        // Resize events new_size and are handled outside the poll_events
        // iterator. This has the effect of coalescing multiple resize
        // events into one.
        let mut new_size = None;

        // TODO should be built into renderer
        unsafe {
            gl::ClearColor(0.0, 0.0, 0.00, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        // Check for any out-of-band resize events (mac only)
        while let Ok(sz) = self.rx.try_recv() {
            new_size = Some(sz);
        }

        // Receive any resize events; only call gl::Viewport on last
        // available
        if let Some((w, h)) = new_size.take() {
            terminal.resize(w as f32, h as f32);
            self.renderer.resize(w as i32, h as i32);
        }

        {
            let glyph_cache = &mut self.glyph_cache;
            // Draw grid
            {
                let _sampler = self.meter.sampler();

                let size_info = terminal.size_info().clone();
                self.renderer.with_api(&size_info, |mut api| {
                    // Draw the grid
                    api.render_grid(&terminal.render_grid(), glyph_cache);
                });
            }

            // Draw render timer
            if self.render_timer {
                let timing = format!("{:.3} usec", self.meter.average());
                let color = Rgb { r: 0xd5, g: 0x4e, b: 0x53 };
                self.renderer.with_api(terminal.size_info(), |mut api| {
                    api.render_string(&timing[..], glyph_cache, &color);
                });
            }
        }

        // Unlock the terminal mutex
        drop(terminal);
        self.window.swap_buffers().unwrap();
    }
}
