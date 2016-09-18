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

extern crate cgmath;
extern crate errno;
extern crate font;
extern crate glutin;
extern crate libc;
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
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::{Condvar, Mutex, MutexGuard};

use config::Config;
use meter::Meter;
use renderer::{QuadRenderer, GlyphCache};
use sync::PriorityMutex;
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

    let reader = terminal.tty().reader();
    let mut writer = terminal.tty().writer();

    let (tx, rx) = mpsc::channel();
    unsafe {
        resize_sender = Some(tx.clone());
    }

    let signal_flag = Flag::new(false);

    let terminal = Arc::new(PriorityMutex::new(terminal));
    let window = Arc::new(window);

    let pty_reader = PtyReader::spawn(
        terminal.clone(),
        reader,
        window.create_window_proxy(),
        signal_flag.clone()
    );

    // Wraps a renderer and gives simple draw() api.
    let mut display = Display::new(
        window.clone(),
        terminal.clone(),
        renderer,
        glyph_cache,
        render_timer,
        rx
    );

    // Event processor
    let mut processor = event::Processor::new(&mut writer, terminal.clone(), tx);

    // Main loop
    loop {
        // Wait for something to happen
        processor.process_events(&window);

        signal_flag.set(false);
 
        // Maybe draw the terminal
        let terminal = terminal.lock_high();
        if terminal.dirty {
            display.draw(terminal);
        }

        if process_should_exit() {
            break;
        }
    }

    // shutdown
    pty_reader.join().ok();
    println!("Goodbye");
}

struct PtyReader;

impl PtyReader {
    pub fn spawn<R>(terminal: Arc<PriorityMutex<Term>>,
                    mut pty: R,
                    proxy: ::glutin::WindowProxy,
                    signal_flag: Flag)
                    -> std::thread::JoinHandle<()>
        where R: std::io::Read + Send + 'static
    {
        thread::spawn_named("pty reader", move || {
            let mut buf = [0u8; 4096];
            let mut pty_parser = ansi::Processor::new();

            loop {
                if let Ok(got) = pty.read(&mut buf[..]) {
                    let mut terminal = terminal.lock_high();

                    for byte in &buf[..got] {
                        pty_parser.advance(&mut *terminal, *byte);
                    }

                    terminal.dirty = true;

                    // Only wake up the event loop if it hasn't already been signaled. This is a
                    // really important optimization because waking up the event loop redundantly
                    // burns *a lot* of cycles.
                    if !signal_flag.get() {
                        proxy.wakeup_event_loop();
                        signal_flag.set(true);
                    }
                } else {
                    break;
                }
            }

            println!("pty reader stopped");
        })
    }
}

struct Display {
    window: Arc<glutin::Window>,
    terminal_mutex: Arc<PriorityMutex<Term>>,
    renderer: QuadRenderer,
    glyph_cache: GlyphCache,
    render_timer: bool,
    rx: mpsc::Receiver<(u32, u32)>,
    meter: Meter,
}

impl Display {
    pub fn new(window: Arc<glutin::Window>,
               terminal_mutex: Arc<PriorityMutex<Term>>,
               renderer: QuadRenderer,
               glyph_cache: GlyphCache,
               render_timer: bool,
               rx: mpsc::Receiver<(u32, u32)>)
               -> Display
    {
        Display {
            window: window,
            terminal_mutex: terminal_mutex,
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
