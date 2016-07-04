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
#![feature(zero_one)]
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
mod tty;
pub mod ansi;
mod term;
mod util;
mod io;
mod sync;

use std::io::{Write, BufWriter, Read};
use std::sync::{mpsc, Arc};

use sync::PriorityMutex;

use config::Config;
use font::FontDesc;
use meter::Meter;
use renderer::{QuadRenderer, GlyphCache};
use term::Term;
use tty::process_should_exit;
use util::thread;

use io::{Utf8Chars, Utf8CharsError};

/// Channel used by resize handling on mac
static mut resize_sender: Option<mpsc::Sender<(u32, u32)>> = None;

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

    let desc = FontDesc::new(font.family(), font.style());
    let mut rasterizer = font::Rasterizer::new(dpi.x(), dpi.y(), dpr);

    let metrics = rasterizer.metrics(&desc, font.size());
    let cell_width = (metrics.average_advance + font.offset().x() as f64) as u32;
    let cell_height = (metrics.line_height + font.offset().y() as f64) as u32;

    println!("Cell Size: ({} x {})", cell_width, cell_height);

    let terminal = Term::new(width as f32, height as f32, cell_width as f32, cell_height as f32);

    let mut reader = terminal.tty().reader();
    let writer = terminal.tty().writer();

    let mut glyph_cache = GlyphCache::new(rasterizer, desc, font.size());

    let (tx, rx) = mpsc::channel();
    unsafe {
        resize_sender = Some(tx.clone());
    }

    let terminal = Arc::new(PriorityMutex::new(terminal));
    let term_ref = terminal.clone();
    let term_updater = terminal.clone();

    // Rendering thread
    //
    // Holds lock only when updating terminal
    let reader_thread = thread::spawn_named("TTY Reader", move || {
        let mut buf = [0u8; 4096];
        let mut start = 0;
        let mut pty_parser = ansi::Parser::new();

        'reader: loop {
            let got = reader.read(&mut buf[start..]).expect("pty fd active");
            let mut remain = 0;

            // if `start` is nonzero, then actual bytes in buffer is > `got` by `start` bytes.
            let end = start + got;
            let mut terminal = term_updater.lock_low();
            for c in Utf8Chars::new(&buf[..end]) {
                match c {
                    Ok(c) => pty_parser.advance(&mut *terminal, c),
                    Err(err) => match err {
                        Utf8CharsError::IncompleteUtf8(unused) => {
                            remain = unused;
                            break;
                        },
                        _ => panic!("{}", err),
                    }
                }
            }

            // Move any leftover bytes to front of buffer
            for i in 0..remain {
                buf[i] = buf[end - (remain - i)];
            }
            start = remain;
        }
    });

    let mut meter = Meter::new();

    let window = Arc::new(window);
    let window_ref = window.clone();

    // Create renderer
    let mut renderer = QuadRenderer::new(width, height);

    // Initialize glyph cache
    {
        let init_start = ::std::time::Instant::now();
        println!("Initializing glyph cache");
        let terminal = term_ref.lock_high();
        renderer.with_api(terminal.size_info(), |mut api| {
            glyph_cache.init(&mut api);
        });

        let stop = init_start.elapsed();
        let stop_f = stop.as_secs() as f64 + stop.subsec_nanos() as f64 / 1_000_000_000f64;
        println!("Finished initializing glyph cache in {}", stop_f);
    }

    let mut input_processor = input::Processor::new();

    'main_loop: loop {

        // Scope ensures terminal lock isn't held when calling swap_buffers
        {
            // Acquire term lock
            let mut terminal = term_ref.lock_high();

            // Resize events new_size and are handled outside the poll_events
            // iterator. This has the effect of coalescing multiple resize
            // events into one.
            let mut new_size = None;

            // Process input events
            {
                let mut writer = BufWriter::new(&writer);
                for event in window_ref.poll_events() {
                    match event {
                        glutin::Event::Closed => break 'main_loop,
                        glutin::Event::ReceivedCharacter(c) => {
                            match c {
                                // Ignore BACKSPACE and DEL. These are handled specially.
                                '\u{8}' | '\u{7f}' => (),
                                _ => {
                                    let encoded = c.encode_utf8();
                                    writer.write(encoded.as_slice()).unwrap();
                                }
                            }
                        },
                        glutin::Event::Resized(w, h) => {
                            terminal.resize(w as f32, h as f32);
                            new_size = Some((w, h));
                        },
                        glutin::Event::KeyboardInput(state, _code, key) => {
                            input_processor.process(state,
                                                    key,
                                                    &mut input::WriteNotifier(&mut writer),
                                                    *terminal.mode())
                        },
                        _ => (),
                    }
                }
            }

            unsafe {
                gl::ClearColor(0.0, 0.0, 0.00, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
            }

            // Check for any out-of-band resize events (mac only)
            while let Ok(sz) = rx.try_recv() {
                new_size = Some(sz);
            }

            // Receive any resize events; only call gl::Viewport on last
            // available
            if let Some((w, h)) = new_size.take() {
                renderer.resize(w as i32, h as i32);
            }

            {
                // Draw grid + cursor
                {
                    let _sampler = meter.sampler();

                    renderer.with_api(terminal.size_info(), |mut api| {
                        // Draw the grid
                        api.render_grid(terminal.grid(), &mut glyph_cache);

                        // Also draw the cursor
                        if terminal.mode().contains(term::mode::SHOW_CURSOR) {
                            api.render_cursor(terminal.cursor(), &mut glyph_cache);
                        }
                    })
                }

                // Draw render timer
                if render_timer {
                    let timing = format!("{:.3} usec", meter.average());
                    let color = Rgb { r: 0xd5, g: 0x4e, b: 0x53 };
                    renderer.with_api(terminal.size_info(), |mut api| {
                        api.render_string(&timing[..], &mut glyph_cache, &color);
                    });
                }
            }
        }

        window.swap_buffers().unwrap();

        if process_should_exit() {
            break;
        }
    }

    reader_thread.join().ok();
    println!("Goodbye");
}

