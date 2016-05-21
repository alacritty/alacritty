extern crate fontconfig;
extern crate freetype;
extern crate libc;
extern crate glutin;
extern crate cgmath;
extern crate euclid;

use std::collections::HashMap;

mod list_fonts;
mod text;
mod renderer;
mod grid;

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

        grid[row][col] = grid::Cell::new(Some(c.escape_default().collect()));
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

    for event in window.wait_events() {
        unsafe {
            gl::ClearColor(0.0, 0.0, 0.00, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        for i in 0..grid.rows() {
            let row = &grid[i];
            for j in 0..row.cols() {
                let cell = &row[j];
                if let Some(ref c) = cell.character {
                    if let Some(glyph) = glyph_cache.get(&c[..]) {
                        let y = (cell_height as f32 + sep_y as f32) * (i as f32);
                        let x = (cell_width as f32 + sep_x as f32) * (j as f32);

                        let y_inverted = (height as f32) - y - (cell_height as f32);

                        renderer.render(glyph, x, y_inverted);
                    }
                }
            }
        }

        window.swap_buffers().unwrap();

        match event {
            glutin::Event::Closed => break,
            _ => ()
        }
    }
}

