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
use alacritty::event_loop::EventLoop;
use alacritty::input;
use alacritty::meter::Meter;
use alacritty::renderer::{QuadRenderer, GlyphCache};
use alacritty::sync::FairMutex;
use alacritty::term::{self, Term};
use alacritty::tty::{self, Pty, process_should_exit};
use alacritty::window::{self, Window, SetInnerSize, Pixels, Size};

mod cli;

fn main() {
    // Load configuration
    let config = match Config::load() {
        // Error loading config
        Err(err) => match err {
            // Use default config when not found
            config::Error::NotFound => {
                err_println!("Config file not found; using defaults");
                Config::default()
            },

            // If there's a problem with the config file, print an error
            // and exit.
            _ => die!("{}", err),
        },

        // Successfully loaded config from file
        Ok(config) => config
    };

    // Load command line options
    let options = cli::Options::load();

    // Extract some properties from config
    let font = config.font();
    let dpi = config.dpi();
    let render_timer = config.render_timer();

    // Create the window where Alacritty will be displayed
    let mut window = match Window::new() {
        Ok(window) => window,
        Err(err) => die!("{}", err)
    };

    // get window properties for initializing the other subsytems
    let size = window.inner_size_pixels().unwrap();
    let dpr = window.hidpi_factor();

    println!("device_pixel_ratio: {}", dpr);

    let rasterizer = font::Rasterizer::new(dpi.x(), dpi.y(), dpr);

    // Create renderer
    let mut renderer = QuadRenderer::new(&config, size);

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

    // Need font metrics to resize the window properly. This suggests to me the
    // font metrics should be computed before creating the window in the first
    // place so that a resize is not needed.
    let metrics = glyph_cache.font_metrics();
    let cell_width = (metrics.average_advance + font.offset().x() as f64) as u32;
    let cell_height = (metrics.line_height + font.offset().y() as f64) as u32;

    // Resize window to specified dimensions
    let width = cell_width * options.columns_u32() + 4;
    let height = cell_height * options.lines_u32() + 4;
    let size = Size { width: Pixels(width), height: Pixels(height) };
    println!("set_inner_size: {}", size);

    window.set_inner_size(size);
    renderer.resize(*size.width as _, *size.height as _);

    println!("Cell Size: ({} x {})", cell_width, cell_height);

    let size = term::SizeInfo {
        width: *size.width as f32,
        height: *size.height as f32,
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

    // Setup the rsize callback for osx
    let terminal_ref = terminal.clone();
    let signal_flag_ref = signal_flag.clone();
    let proxy = window.create_window_proxy();
    let tx2 = tx.clone();
    window.set_resize_callback(move |width, height| {
        let _ = tx2.send((width, height));
        if !signal_flag_ref.0.swap(true, Ordering::AcqRel) {
           // We raised the signal flag
            let mut terminal = terminal_ref.lock();
            terminal.dirty = true;
            proxy.wakeup_event_loop();
        }
    });

    let event_loop = EventLoop::new(
        terminal.clone(),
        window.create_window_proxy(),
        signal_flag.clone(),
        pty_io,
        options.ref_test,
    );

    let loop_tx = event_loop.channel();
    let event_loop_handle = event_loop.spawn(None);

    // Wraps a renderer and gives simple draw() api.
    let mut display = Display::new(
        &window,
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
        options.ref_test,
    );

    let (config_tx, config_rx) = mpsc::channel();

    // create a config watcher when config is loaded from disk
    let _config_reloader = config.path().map(|path| {
        config::Watcher::new(path, ConfigHandler {
            tx: config_tx,
            window: window.create_window_proxy(),
        })
    });

    // Main loop
    let mut force_draw;
    loop {
        force_draw = false;
        // Wait for something to happen
        processor.process_events(&window);

        // Handle config reloads
        if let Ok(config) = config_rx.try_recv() {
            force_draw = true;
            display.update_config(&config);
            processor.update_config(&config);
        }

        // Maybe draw the terminal
        let terminal = terminal.lock();
        signal_flag.set(false);
        if force_draw || terminal.dirty {
            display.draw(terminal, &config);
        }

        // Begin shutdown if the flag was raised.
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
    window: window::Proxy,
}

impl config::OnConfigReload for ConfigHandler {
    fn on_config_reload(&mut self, config: Config) {
        if let Err(..) = self.tx.send(config) {
            err_println!("Failed to notify of new config");
            return;
        }

        self.window.wakeup_event_loop();
    }
}

struct Display<'a> {
    window: &'a Window,
    renderer: QuadRenderer,
    glyph_cache: GlyphCache,
    render_timer: bool,
    rx: mpsc::Receiver<(u32, u32)>,
    meter: Meter,
    pty: Pty,
}

impl<'a> Display<'a> {
    pub fn update_config(&mut self, config: &Config) {
        self.renderer.update_config(config);
        self.render_timer = config.render_timer();
    }

    pub fn new(
        window: &Window,
        renderer: QuadRenderer,
        glyph_cache: GlyphCache,
        render_timer: bool,
        rx: mpsc::Receiver<(u32, u32)>,
        pty: Pty
    ) -> Display {
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
    pub fn draw(&mut self, mut terminal: MutexGuard<Term>, config: &Config) {
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
                self.renderer.with_api(config, &size_info, |mut api| {
                    api.clear();

                    // Draw the grid
                    api.render_cells(terminal.renderable_cells(), glyph_cache);
                });
            }

            // Draw render timer
            if self.render_timer {
                let timing = format!("{:.3} usec", self.meter.average());
                let color = alacritty::ansi::Color::Spec(Rgb { r: 0xd5, g: 0x4e, b: 0x53 });
                self.renderer.with_api(config, terminal.size_info(), |mut api| {
                    api.render_string(&timing[..], glyph_cache, &color);
                });
            }
        }

        // Unlock the terminal mutex
        drop(terminal);
        self.window.swap_buffers().unwrap();
    }
}
