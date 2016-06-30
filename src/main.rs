//! Alacritty - The GPU Enhanced Terminal
#![feature(question_mark)]
#![feature(range_contains)]
#![feature(inclusive_range_syntax)]
#![feature(io)]
#![feature(drop_types_in_const)]
#![feature(unicode)]

extern crate font;
extern crate libc;
extern crate glutin;
extern crate cgmath;
extern crate notify;
extern crate errno;
extern crate parking_lot;

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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use parking_lot::Mutex;

use font::FontDesc;
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
        self.0.write(message.as_bytes()).unwrap();
    }
}

/// Channel used by resize handling on mac
static mut resize_sender: Option<mpsc::Sender<Event>> = None;

/// Resize handling for Mac
fn window_resize_handler(width: u32, height: u32) {
    unsafe {
        if let Some(ref tx) = resize_sender {
            let _ = tx.send(Event::Glutin(glutin::Event::Resized(width, height)));
        }
    }
}

fn handle_event<W>(event: Event,
                   writer: &mut W,
                   terminal: &mut Term,
                   pty_parser: &mut ansi::Parser,
                   render_tx: &mpsc::Sender<(u32, u32)>,
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
            glutin::Event::Resized(w, h) => {
                terminal.resize(w as f32, h as f32);
                render_tx.send((w, h)).expect("render thread active");
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

#[cfg(target_os = "linux")]
static FONT: &'static str = "DejaVu Sans Mono";
#[cfg(target_os = "linux")]
static FONT_STYLE: &'static str = "Book";

#[cfg(target_os = "macos")]
static FONT: &'static str = "Menlo";
#[cfg(target_os = "macos")]
static FONT_STYLE: &'static str = "Regular";


fn main() {

    let mut window = glutin::WindowBuilder::new()
                                           .with_vsync()
                                           .with_title("Alacritty")
                                           .build().unwrap();
    window.set_window_resize_callback(Some(window_resize_handler as fn(u32, u32)));

    gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);
    let (width, height) = window.get_inner_size_pixels().unwrap();
    let dpr = window.hidpi_factor();

    println!("device_pixel_ratio: {}", dpr);

    let font_size = 11.;

    let sep_x = 0.0;
    let sep_y = 0.0;

    let desc = FontDesc::new(FONT, FONT_STYLE);
    let mut rasterizer = font::Rasterizer::new(96., 96., dpr);

    let metrics = rasterizer.metrics(&desc, font_size);
    let cell_width = (metrics.average_advance + sep_x) as u32;
    let cell_height = (metrics.line_height + sep_y) as u32;

    println!("Cell Size: ({} x {})", cell_width, cell_height);

    let terminal = Term::new(width as f32, height as f32, cell_width as f32, cell_height as f32);

    let reader = terminal.tty().reader();
    let writer = terminal.tty().writer();

    let mut glyph_cache = GlyphCache::new(rasterizer, desc, font_size);
    let needs_render = Arc::new(AtomicBool::new(true));
    let needs_render2 = needs_render.clone();

    let (tx, rx) = mpsc::channel();
    let reader_tx = tx.clone();
    unsafe {
        resize_sender = Some(tx.clone());
    }
    let reader_thread = thread::spawn_named("TTY Reader", move || {
        for c in reader.chars() {
            let c = c.unwrap();
            reader_tx.send(Event::PtyChar(c)).unwrap();
        }
    });

    let terminal = Arc::new(Mutex::new(terminal));
    let term_ref = terminal.clone();
    let mut meter = Meter::new();

    let mut pty_parser = ansi::Parser::new();

    let window = Arc::new(window);
    let window_ref = window.clone();

    let (render_tx, render_rx) = mpsc::channel::<(u32, u32)>();

    let update_thread = thread::spawn_named("Update", move || {
        'main_loop: loop {
            let mut writer = BufWriter::new(&writer);
            let mut input_processor = input::Processor::new();

            // Handle case where renderer didn't acquire lock yet
            if needs_render.load(Ordering::Acquire) {
                ::std::thread::yield_now();
                continue;
            }

            if process_should_exit() {
                break;
            }

            // Block waiting for next event and handle it
            let event = match rx.recv() {
                Ok(e) => e,
                Err(mpsc::RecvError) => break,
            };

            // Need mutable terminal for updates; lock it.
            let mut terminal = terminal.lock();
            let res = handle_event(event,
                                   &mut writer,
                                   &mut *terminal,
                                   &mut pty_parser,
                                   &render_tx,
                                   &mut input_processor);
            if res == ShouldExit::Yes {
                break;
            }

            // Handle Any events that are in the queue
            loop {
                match rx.try_recv() {
                    Ok(e) => {
                        let res = handle_event(e,
                                               &mut writer,
                                               &mut *terminal,
                                               &mut pty_parser,
                                               &render_tx,
                                               &mut input_processor);

                        if res == ShouldExit::Yes {
                            break;
                        }
                    },
                    Err(mpsc::TryRecvError::Disconnected) => break 'main_loop,
                    Err(mpsc::TryRecvError::Empty) => break,
                }

                // Release the lock if a render is needed
                if needs_render.load(Ordering::Acquire) {
                    break;
                }
            }
        }
    });

    let render_thread = thread::spawn_named("Render", move || {
        let _ = unsafe { window.make_current() };
        unsafe {
            gl::Viewport(0, 0, width as i32, height as i32);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
            gl::Enable(gl::MULTISAMPLE);
        }

        // Create renderer
        let mut renderer = QuadRenderer::new(width, height);

        // Initialize glyph cache
        {
            let terminal = term_ref.lock();
            renderer.with_api(terminal.size_info(), |mut api| {
                glyph_cache.init(&mut api);
            });
        }

        loop {
            unsafe {
                gl::ClearColor(0.0, 0.0, 0.00, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
            }

            // Receive any resize events; only call gl::Viewport on last available
            let mut new_size = None;
            while let Ok(val) = render_rx.try_recv() {
                new_size = Some(val);
            }
            if let Some((w, h)) = new_size.take() {
                renderer.resize(w as i32, h as i32);
            }

            // Need scope so lock is released when swap_buffers is called
            {
                // Flag that it's time for render
                needs_render2.store(true, Ordering::Release);
                // Acquire term lock
                let terminal = term_ref.lock();
                // Have the lock, ok to lower flag
                needs_render2.store(false, Ordering::Relaxed);

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
                let timing = format!("{:.3} usec", meter.average());
                let color = Rgb { r: 0xd5, g: 0x4e, b: 0x53 };
                renderer.with_api(terminal.size_info(), |mut api| {
                    api.render_string(&timing[..], &mut glyph_cache, &color);
                });
            }

            window.swap_buffers().unwrap();

            if process_should_exit() {
                break;
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
    update_thread.join().ok();
    println!("Goodbye");
}

