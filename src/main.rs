//! Alacritty - The GPU Enhanced Terminal
#![feature(question_mark)]
#![feature(range_contains)]
#![feature(inclusive_range_syntax)]
#![feature(io)]
#![feature(unicode)]

extern crate font;
extern crate libc;
extern crate glutin;
extern crate cgmath;
extern crate notify;
extern crate errno;

#[macro_use]
extern crate bitflags;

#[macro_use]
mod macros;

mod renderer;
pub mod grid;
mod meter;
mod input;
mod tty;
pub mod ansi;
mod term;
mod util;

use std::io::{Read, Write, BufWriter};
use std::sync::Arc;
use std::sync::mpsc;

use font::FontDesc;
use grid::Grid;
use meter::Meter;
use renderer::{QuadRenderer, GlyphCache};
use term::Term;
use tty::process_should_exit;
use util::thread;

/// Things that the render/update thread needs to respond to
#[derive(Debug)]
enum Event {
    PtyChar(char),
    Glutin(glutin::Event),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ShouldExit {
    Yes,
    No
}

struct WriteNotifier<'a, W: Write + 'a>(&'a mut W);
impl<'a, W: Write> input::Notify for WriteNotifier<'a, W> {
    fn notify(&mut self, message: &str) {
        println!("writing: {:?} [{} bytes]", message.as_bytes(), message.as_bytes().len());
        self.0.write(message.as_bytes()).unwrap();
    }
}

fn handle_event<W>(event: Event,
                   writer: &mut W,
                   terminal: &mut Term,
                   pty_parser: &mut ansi::Parser,
                   input_processor: &mut input::Processor) -> ShouldExit
    where W: Write
{
    match event {
        // Handle char from pty
        Event::PtyChar(c) => pty_parser.advance(terminal, c),
        // Handle keyboard/mouse input and other window events
        Event::Glutin(gevent) => match gevent {
            glutin::Event::Closed => return ShouldExit::Yes,
            glutin::Event::ReceivedCharacter(c) => {
                let encoded = c.encode_utf8();
                writer.write(encoded.as_slice()).unwrap();
            },
            glutin::Event::KeyboardInput(state, _code, key) => {
                input_processor.process(state, key, &mut WriteNotifier(writer), *terminal.mode())
            },
            _ => ()
        }
    }

    ShouldExit::No
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

#[derive(Debug)]
pub struct TermProps {
    width: f32,
    height: f32,
    cell_width: f32,
    cell_height: f32,
}

#[cfg(target_os = "linux")]
static FONT: &'static str = "DejaVu Sans Mono";
#[cfg(target_os = "linux")]
static FONT_STYLE: &'static str = "Book";

#[cfg(target_os = "macos")]
static FONT: &'static str = "Menlo";
#[cfg(target_os = "macos")]
static FONT_STYLE: &'static str = "Regular";


fn main() {

    let window = glutin::WindowBuilder::new().build().unwrap();
    window.set_title("Alacritty");
    // window.set_window_resize_callback(Some(resize_callback as fn(u32, u32)));

    gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);
    let (width, height) = window.get_inner_size_pixels().unwrap();
    let dpr = window.hidpi_factor();

    println!("device_pixel_ratio: {}", dpr);

    let font_size = 11.;

    let sep_x = 2.0;
    let sep_y = -7.0;

    let desc = FontDesc::new(FONT, FONT_STYLE);
    let mut rasterizer = font::Rasterizer::new(96., 96., dpr);

    let metrics = rasterizer.metrics(&desc, font_size);
    let cell_width = (metrics.average_advance + sep_x) as u32;
    let cell_height = (metrics.line_height + sep_y) as u32;

    println!("Cell Size: ({} x {})", cell_width, cell_height);

    let num_cols = grid::num_cells_axis(cell_width, width);
    let num_rows = grid::num_cells_axis(cell_height, height);

    let tty = tty::new(num_rows as u8, num_cols as u8);
    tty.resize(num_rows as usize, num_cols as usize, width as usize, height as usize);
    let reader = tty.reader();
    let writer = tty.writer();

    println!("num_cols, num_rows = {}, {}", num_cols, num_rows);

    let grid = Grid::new(num_rows as usize, num_cols as usize);

    let props = TermProps {
        cell_width: cell_width as f32,
        cell_height: cell_height as f32,
        height: height as f32,
        width: width as f32,
    };

    let mut glyph_cache = GlyphCache::new(rasterizer, desc, font_size);


    let (tx, rx) = mpsc::channel();
    let reader_tx = tx.clone();
    let reader_thread = thread::spawn_named("TTY Reader", move || {
        for c in reader.chars() {
            let c = c.unwrap();
            reader_tx.send(Event::PtyChar(c)).unwrap();
        }
    });

    let mut terminal = Term::new(tty, grid);
    let mut meter = Meter::new();

    let mut pty_parser = ansi::Parser::new();

    let window = Arc::new(window);
    let window_ref = window.clone();
    let render_thread = thread::spawn_named("Galaxy", move || {
        let _ = unsafe { window.make_current() };
        unsafe {
            gl::Viewport(0, 0, width as i32, height as i32);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
            gl::Enable(gl::MULTISAMPLE);
        }

        let mut renderer = QuadRenderer::new(width, height);
        renderer.with_api(&props, |mut api| {
            glyph_cache.init(&mut api);
        });

        let mut input_processor = input::Processor::new();

        'main_loop: loop {
            {
                let mut writer = BufWriter::new(&writer);

                // Block waiting for next event
                match rx.recv() {
                    Ok(e) => {
                        let res = handle_event(e,
                                               &mut writer,
                                               &mut terminal,
                                               &mut pty_parser,
                                               &mut input_processor);
                        if res == ShouldExit::Yes {
                            break;
                        }
                    },
                    Err(mpsc::RecvError) => break,
                }

                // Handle Any events that have been queued
                loop {
                    match rx.try_recv() {
                        Ok(e) => {
                            let res = handle_event(e,
                                                   &mut writer,
                                                   &mut terminal,
                                                   &mut pty_parser,
                                                   &mut input_processor);

                            if res == ShouldExit::Yes {
                                break;
                            }
                        },
                        Err(mpsc::TryRecvError::Disconnected) => break 'main_loop,
                        Err(mpsc::TryRecvError::Empty) => break,
                    }

                    // TODO make sure this doesn't block renders
                }
            }

            unsafe {
                gl::ClearColor(0.0, 0.0, 0.00, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
            }

            {
                let _sampler = meter.sampler();

                renderer.with_api(&props, |mut api| {
                    // Draw the grid
                    api.render_grid(terminal.grid(), &mut glyph_cache);

                    // Also draw the cursor
                    if terminal.mode().contains(term::mode::SHOW_CURSOR) {
                        api.render_cursor(terminal.cursor(), &mut glyph_cache);
                    }
                })
            }

            // Draw render timer
            let timing = format!("{:.3} usec", meter.average());
            let color = Rgb { r: 0xd5, g: 0x4e, b: 0x53 };
            renderer.with_api(&props, |mut api| {
                api.render_string(&timing[..], &mut glyph_cache, &color);
            });

            window.swap_buffers().unwrap();

            if process_should_exit() {
                break 'main_loop;
            }
        }
    });

    'event_processing: loop {
        for event in window_ref.wait_events() {
            tx.send(Event::Glutin(event)).unwrap();
            if process_should_exit() {
                break 'event_processing;
            }
        }
    }

    reader_thread.join().ok();
    render_thread.join().ok();
    println!("Goodbye");
}

