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
#![feature(inclusive_range_syntax)]
#![feature(drop_types_in_const)]
#![allow(stable_features)] // lying about question_mark because 1.14.0 isn't released!

#![feature(proc_macro)]

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate alacritty;
extern crate cgmath;
extern crate copypasta;
extern crate errno;
extern crate font;
extern crate glutin;
extern crate libc;
extern crate mio;
extern crate notify;
extern crate parking_lot;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate vte;

#[macro_use]
extern crate bitflags;

use std::sync::{mpsc, Arc};
use std::sync::atomic::Ordering;

use parking_lot::{MutexGuard};

use alacritty::Flag;
use alacritty::Rgb;
use alacritty::config::{self, Config};
use alacritty::event;
use alacritty::gl;
use alacritty::input;
use alacritty::meter::Meter;
use alacritty::renderer::{QuadRenderer, GlyphCache};
use alacritty::sync::FairMutex;
use alacritty::term::{self, Term};
use alacritty::tty::{self, Pty, process_should_exit};
use alacritty::event_loop::EventLoop;

/// Channel used by resize handling on mac
static mut RESIZE_CALLBACK: Option<Box<Fn(u32, u32)>> = None;

/// Resize handling for Mac
fn window_resize_handler(width: u32, height: u32) {
    unsafe {
        RESIZE_CALLBACK.as_ref().map(|func| func(width, height));
    }
}

fn main() {
    // Load configuration
    let (config, config_path) = match Config::load() {
        Err(err) => match err {
            // Use default config when not found
            config::Error::NotFound => (Config::default(), None),
            // Exit when there's a problem with it
            _ => die!("{}", err),
        },
        Ok((config, path)) => (config, Some(path)),
    };

    let mut ref_test = false;
    let mut columns = 80;
    let mut lines = 24;

    let mut args_iter = ::std::env::args();
    while let Some(arg) = args_iter.next() {
        match &arg[..] {
            // Generate ref test
            "--ref-test" => ref_test = true,
            // Set dimensions
            "-d" | "--dimensions" => {
                args_iter.next()
                    .map(|w| w.parse().map(|w| columns = w));
                args_iter.next()
                    .map(|h| h.parse().map(|h| lines = h));
            },
            // ignore unexpected
            _ => (),
        }
    }

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
        // gl::Viewport(0, 0, width as i32, height as i32);
        gl::Enable(gl::BLEND);
        gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
        gl::Enable(gl::MULTISAMPLE);
    }

    let rasterizer = font::Rasterizer::new(dpi.x(), dpi.y(), dpr);

    // Create renderer
    let mut renderer = QuadRenderer::new(&config, width, height);

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

    // Resize window to be 80 col x 24 lines
    let width = cell_width * columns + 4;
    let height = cell_height * lines + 4;
    println!("set_inner_size: {} x {}", width, height);
    // Is this in points?
    let width_pts = (width as f32 / dpr) as u32;
    let height_pts = (height as f32 / dpr) as u32;
    println!("set_inner_size: {} x {}; pts: {} x {}", width, height, width_pts, height_pts);
    window.set_inner_size(width_pts, height_pts);
    renderer.resize(width as _, height as _);

    println!("Cell Size: ({} x {})", cell_width, cell_height);

    let size = term::SizeInfo {
        width: width as f32,
        height: height as f32,
        cell_width: cell_width as f32,
        cell_height: cell_height as f32
    };

    let terminal = Term::new(size);
    let pty = tty::new(size.lines(), size.cols());
    pty.resize(size.lines(), size.cols(), size.width as usize, size.height as usize);
    let pty_io = pty.reader();

    let (tx, rx) = mpsc::channel();

    let signal_flag = Flag::new(false);

    let terminal = Arc::new(FairMutex::new(terminal));
    let window = Arc::new(window);

    // Setup the rsize callback for osx
    let terminal_ref = terminal.clone();
    let signal_flag_ref = signal_flag.clone();
    let proxy = window.create_window_proxy();
    let tx2 = tx.clone();
    unsafe {
        RESIZE_CALLBACK = Some(Box::new(move |width: u32, height: u32| {
            let _ = tx2.send((width, height));
            if !signal_flag_ref.0.swap(true, Ordering::AcqRel) {
               // We raised the signal flag
                let mut terminal = terminal_ref.lock();
                terminal.dirty = true;
                proxy.wakeup_event_loop();
            }
        }));
    }

    let event_loop = EventLoop::new(
        terminal.clone(),
        window.create_window_proxy(),
        signal_flag.clone(),
        pty_io,
        ref_test,
    );

    let loop_tx = event_loop.channel();
    let event_loop_handle = event_loop.spawn(None);

    // Wraps a renderer and gives simple draw() api.
    let mut display = Display::new(
        window.clone(),
        renderer,
        glyph_cache,
        render_timer,
        rx,
        pty
    );

    // Event processor
    let mut processor = event::Processor::new(
        input::LoopNotifier(loop_tx),
        terminal.clone(),
        tx,
        &config,
        ref_test,
    );

    let (config_tx, config_rx) = mpsc::channel();

    // create a config watcher when config is loaded from disk
    let _config_reloader = config_path.map(|config_path| {
        config::Watcher::new(config_path, ConfigHandler {
            tx: config_tx,
            window: window.create_window_proxy(),
        })
    });

    // Main loop
    loop {
        // Wait for something to happen
        processor.process_events(&window);

        if let Ok(config) = config_rx.try_recv() {
            display.update_config(&config);
            processor.update_config(&config);
        }

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

    // FIXME need file watcher to work with custom delegates before
    //       joining config reloader is possible
    // config_reloader.join().ok();

    // shutdown
    event_loop_handle.join().ok();
    println!("Goodbye");
}

struct ConfigHandler {
    tx: mpsc::Sender<config::Config>,
    window: ::glutin::WindowProxy,
}

// TODO FIXME
impl config::OnConfigReload for ConfigHandler {
    fn on_config_reload(&mut self, config: Config) {
        if let Err(..) = self.tx.send(config) {
            err_println!("Failed to notify of new config");
            return;
        }

        self.window.wakeup_event_loop();
    }
}

struct Display {
    window: Arc<glutin::Window>,
    renderer: QuadRenderer,
    glyph_cache: GlyphCache,
    render_timer: bool,
    rx: mpsc::Receiver<(u32, u32)>,
    meter: Meter,
    pty: Pty,
}

impl Display {
    pub fn update_config(&mut self, config: &Config) {
        self.renderer.update_config(config);
        self.render_timer = config.render_timer();
    }

    pub fn new(window: Arc<glutin::Window>,
               renderer: QuadRenderer,
               glyph_cache: GlyphCache,
               render_timer: bool,
               rx: mpsc::Receiver<(u32, u32)>,
               pty: Pty)
               -> Display
    {
        Display {
            window: window,
            renderer: renderer,
            glyph_cache: glyph_cache,
            render_timer: render_timer,
            rx: rx,
            meter: Meter::new(),
            pty: pty,
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


        // Check for any out-of-band resize events (mac only)
        while let Ok(sz) = self.rx.try_recv() {
            new_size = Some(sz);
        }

        // Receive any resize events; only call gl::Viewport on last
        // available
        if let Some((w, h)) = new_size.take() {
            terminal.resize(w as f32, h as f32);
            let size = terminal.size_info();
            self.pty.resize(size.lines(), size.cols(), w as _, h as _);
            self.renderer.resize(w as i32, h as i32);
        }

        {
            let glyph_cache = &mut self.glyph_cache;
            // Draw grid
            {
                let _sampler = self.meter.sampler();

                let size_info = terminal.size_info().clone();
                self.renderer.with_api(&size_info, |mut api| {
                    api.clear();

                    // Draw the grid
                    api.render_cells(terminal.renderable_cells(), glyph_cache);
                });
            }

            // Draw render timer
            if self.render_timer {
                let timing = format!("{:.3} usec", self.meter.average());
                let color = alacritty::ansi::Color::Spec(Rgb { r: 0xd5, g: 0x4e, b: 0x53 });
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
