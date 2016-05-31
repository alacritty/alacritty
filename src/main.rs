//! Alacritty - The GPU Enhanced Terminal
#![feature(question_mark)]
#![feature(range_contains)]
#![feature(inclusive_range_syntax)]
#![feature(io)]
#![feature(unicode)]

extern crate fontconfig;
extern crate freetype;
extern crate libc;
extern crate glutin;
extern crate cgmath;
extern crate euclid;

#[macro_use]
mod macros;

mod list_fonts;
mod text;
mod renderer;
mod grid;
mod meter;
mod tty;
mod ansi;
mod term;

use std::collections::HashMap;
use std::io::{BufReader, Read, BufRead, Write, BufWriter};
use std::sync::Arc;
use std::fs::File;

use std::os::unix::io::{FromRawFd, AsRawFd};

use renderer::{Glyph, QuadRenderer};
use text::FontDesc;
use grid::Grid;
use term::Term;
use meter::Meter;

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
pub struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

mod gl {
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}

static INIT_LIST: &'static str = "abcdefghijklmnopqrstuvwxyz\
                                  ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                  01234567890\
                                  ~`!@#$%^&*()[]{}-_=+\\|\"'/?.,<>;:█└│├─➜";

type GlyphCache = HashMap<char, renderer::Glyph>;

/// Render a string in a predefined location. Used for printing render time for profiling and
/// optimization.
fn render_string(s: &str,
                 renderer: &QuadRenderer,
                 glyph_cache: &GlyphCache,
                 cell_width: u32,
                 color: &Rgb)
{
    let (mut x, mut y) = (200f32, 20f32);

    for c in s.chars() {
        if let Some(glyph) = glyph_cache.get(&c) {
            renderer.render(glyph, x, y, color);
        }

        x += cell_width as f32 + 2f32;
    }
}

fn main() {
    let window = glutin::WindowBuilder::new()
                     .with_title("alacritty".into())
                     .build()
                     .unwrap();

    let (width, height) = window.get_inner_size_pixels().unwrap();
    unsafe {
        window.make_current().unwrap();
    }

    unsafe {
        gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);
        gl::Viewport(0, 0, width as i32, height as i32);
    }

    let (dpi_x, dpi_y) = window.get_dpi().unwrap();
    let dpr = window.hidpi_factor();

    let font_size = 11.;

    let sep_x = 2;
    let sep_y = 5;

    let desc = FontDesc::new("DejaVu Sans Mono", "Book");
    let mut rasterizer = text::Rasterizer::new(dpi_x, dpi_y, dpr);

    let (cell_width, cell_height) = rasterizer.box_size_for_font(&desc, font_size);

    let num_cols = grid::num_cells_axis(cell_width, sep_x, width);
    let num_rows = grid::num_cells_axis(cell_height, sep_y, height);

    let tty = tty::new(num_rows as u8, num_cols as u8);
    tty.resize(num_rows as usize, num_cols as usize, width as usize, height as usize);
    let mut reader = tty.reader();
    let mut writer = tty.writer();

    println!("num_cols, num_rows = {}, {}", num_cols, num_rows);

    let mut grid = Grid::new(num_rows as usize, num_cols as usize);

    let mut glyph_cache = HashMap::new();
    for c in INIT_LIST.chars() {
        let glyph = Glyph::new(&rasterizer.get_glyph(&desc, font_size, c));
        glyph_cache.insert(c, glyph);
    }

    unsafe {
        gl::Enable(gl::BLEND);
        gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
        gl::Enable(gl::MULTISAMPLE);
    }

    let (chars_tx, chars_rx) = ::std::sync::mpsc::channel();
    ::std::thread::spawn(move || {
        for c in reader.chars() {
            let c = c.unwrap();
            chars_tx.send(c);
        }
    });

    let renderer = QuadRenderer::new(width, height);
    let mut terminal = Term::new(tty, grid);
    let mut meter = Meter::new();

    let mut pty_parser = ansi::Parser::new();

    'main_loop: loop {
        // Handle keyboard/mouse input and other window events
        {
            let mut writer = BufWriter::new(&writer);
            for event in window.poll_events() {
                match event {
                    glutin::Event::Closed => break 'main_loop,
                    glutin::Event::ReceivedCharacter(c) => {
                        let encoded = c.encode_utf8();
                        writer.write(encoded.as_slice());
                    },
                    glutin::Event::KeyboardInput(state, _code, key) => {
                        match state {
                            glutin::ElementState::Pressed => {
                                match key {
                                    Some(glutin::VirtualKeyCode::Up) => {
                                        writer.write("\x1b[A".as_bytes());
                                    },
                                    Some(glutin::VirtualKeyCode::Down) => {
                                        writer.write("\x1b[B".as_bytes());
                                    },
                                    Some(glutin::VirtualKeyCode::Left) => {
                                        writer.write("\x1b[D".as_bytes());
                                    },
                                    Some(glutin::VirtualKeyCode::Right) => {
                                        writer.write("\x1b[C".as_bytes());
                                    },
                                    _ => (),
                                }
                            },
                            _ => (),
                        }
                    },
                    _ => ()
                }
            }
        }

        while let Ok(c) = chars_rx.try_recv() {
            pty_parser.advance(&mut terminal, c);
        }

        unsafe {
            gl::ClearColor(0.0, 0.0, 0.00, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        {
            let _sampler = meter.sampler();

            // Draw the grid
            let grid = terminal.grid();
            for i in 0..grid.rows() {
                let row = &grid[i];
                for j in 0..row.cols() {
                    let cell = &row[j];
                    if cell.c != ' ' {
                        if let Some(glyph) = glyph_cache.get(&cell.c) {
                            let y = (cell_height as f32 + sep_y as f32) * (i as f32);
                            let x = (cell_width as f32 + sep_x as f32) * (j as f32);

                            let y_inverted = (height as f32) - y - (cell_height as f32);

                            renderer.render(glyph, x, y_inverted, &cell.fg);
                        }
                    }
                }
            }

            // Also draw the cursor
            if let Some(glyph) = glyph_cache.get(&term::CURSOR_SHAPE) {
                let y = (cell_height as f32 + sep_y as f32) * (terminal.cursor_y() as f32);
                let x = (cell_width as f32 + sep_x as f32) * (terminal.cursor_x() as f32);

                let y_inverted = (height as f32) - y - (cell_height as f32);

                renderer.render(glyph, x, y_inverted, &term::DEFAULT_FG);
            }
        }

        let timing = format!("{:.3} usec", meter.average());
        let color = Rgb { r: 0xd5, g: 0x4e, b: 0x53 };
        render_string(&timing[..], &renderer, &glyph_cache, cell_width, &color);

        window.swap_buffers().unwrap();

        // ::std::thread::sleep(::std::time::Duration::from_millis(17));
    }
}

