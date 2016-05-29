//! Alacritty - The GPU Enhanced Terminal
#![feature(question_mark)]
#![feature(range_contains)]
#![feature(inclusive_range_syntax)]
#![feature(io)]

extern crate fontconfig;
extern crate freetype;
extern crate libc;
extern crate glutin;
extern crate cgmath;
extern crate euclid;

use std::collections::HashMap;

use std::io::{BufReader, Read, BufRead};

#[macro_use]
mod macros;

mod list_fonts;
mod text;
mod renderer;
mod grid;
mod meter;
mod tty;
mod ansi;

use renderer::{Glyph, QuadRenderer};
use text::FontDesc;
use grid::Grid;

mod gl {
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}

static INIT_LIST: &'static str = "abcdefghijklmnopqrstuvwxyz\
                                  ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                  01234567890\
                                  ~`!@#$%^&*()[]{}-_=+\\|\"/?.,<>;:";

type GlyphCache = HashMap<String, renderer::Glyph>;

/// Render a string in a predefined location. Used for printing render time for profiling and
/// optimization.
fn render_string(s: &str,
                 renderer: &QuadRenderer,
                 glyph_cache: &GlyphCache,
                 cell_width: u32,
                 color: &renderer::Rgb)
{
    let (mut x, mut y) = (200f32, 20f32);

    for c in s.chars() {
        let s: String = c.escape_default().collect();
        if let Some(glyph) = glyph_cache.get(&s[..]) {
            renderer.render(glyph, x, y, color);
        }

        x += cell_width as f32 + 2f32;
    }
}

fn main() {
    let window = glutin::Window::new().unwrap();
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

    let mut cmd = tty::new(num_rows as u8, num_cols as u8);

    ::std::thread::spawn(move || {
        for byte in cmd.bytes() {
            let b = byte.unwrap();
            println!("{:02x}, {:?}", b, ::std::char::from_u32(b as u32));
        }
    });

    println!("num_cols, num_rows = {}, {}", num_cols, num_rows);

    let mut grid = Grid::new(num_rows as usize, num_cols as usize);

    // let contents = [
    //     "for (row, line) in contents.iter().enumerate() {",
    //     "    for (i, c) in line.chars().enumerate() {",
    //     "        grid[row][i] = grid::Cell::new(Some(c.escape_default().collect()));",
    //     "    }",
    //     "}"];

    let contents = include_str!("grid.rs");
    let mut row = 0usize;
    let mut col = 0;

    for (i, c) in contents.chars().enumerate() {
        if c == '\n' {
            row += 1;
            col = 0;
            continue;
        }

        if row >= (num_rows as usize) {
            break;
        }

        if col >= grid.cols() {
            continue;
        }

        grid[row][col] = grid::Cell::new(c.escape_default().collect::<String>());
        col += 1;
    }

    let mut glyph_cache = HashMap::new();
    for c in INIT_LIST.chars() {
        let glyph = Glyph::new(&rasterizer.get_glyph(&desc, font_size, c));
        let string: String = c.escape_default().collect();
        glyph_cache.insert(string, glyph);
    }

    unsafe {
        gl::Enable(gl::BLEND);
        gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
        gl::Enable(gl::MULTISAMPLE);
    }

    let renderer = QuadRenderer::new(width, height);

    let mut meter = meter::Meter::new();
    'main_loop: loop {
        for event in window.poll_events() {
            match event {
                glutin::Event::Closed => break 'main_loop,
                _ => ()
            }
        }

        unsafe {
            gl::ClearColor(0.0, 0.0, 0.00, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        {
            let color = renderer::Rgb { r: 0.917, g: 0.917, b: 0.917 };
            let _sampler = meter.sampler();

            for i in 0..grid.rows() {
                let row = &grid[i];
                for j in 0..row.cols() {
                    let cell = &row[j];
                    if !cell.character.is_empty() {
                        if let Some(glyph) = glyph_cache.get(&cell.character[..]) {
                            let y = (cell_height as f32 + sep_y as f32) * (i as f32);
                            let x = (cell_width as f32 + sep_x as f32) * (j as f32);

                            let y_inverted = (height as f32) - y - (cell_height as f32);

                            renderer.render(glyph, x, y_inverted, &color);
                        }
                    }
                }
            }
        }

        let timing = format!("{:.3} usec", meter.average());
        let color = renderer::Rgb { r: 0.835, g: 0.306, b: 0.325 };
        render_string(&timing[..], &renderer, &glyph_cache, cell_width, &color);

        window.swap_buffers().unwrap();

        // ::std::thread::sleep(::std::time::Duration::from_millis(17));
    }
}

