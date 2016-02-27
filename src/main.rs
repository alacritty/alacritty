extern crate fontconfig;
extern crate freetype;
extern crate libc;
extern crate glutin;
extern crate gl;
extern crate cgmath;
extern crate euclid;

mod list_fonts;
mod text;
mod renderer;

use renderer::{Glyph, QuadRenderer};
use text::FontDesc;


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

    let desc = FontDesc::new("Ubuntu Mono", "Regular");
    let mut rasterizer = text::Rasterizer::new();

    let glyph_r = Glyph::new(&rasterizer.get_glyph(&desc, 180., 'R'));
    let glyph_u = Glyph::new(&rasterizer.get_glyph(&desc, 180., 'u'));
    let glyph_s = Glyph::new(&rasterizer.get_glyph(&desc, 180., 's'));
    let glyph_t = Glyph::new(&rasterizer.get_glyph(&desc, 180., 't'));

    unsafe {
        gl::Enable(gl::BLEND);
        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        gl::Enable(gl::MULTISAMPLE);
    }

    let renderer = QuadRenderer::new(width, height);

    for event in window.wait_events() {
        unsafe {
            gl::ClearColor(0.08, 0.08, 0.08, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        renderer.render(&glyph_r, 10.0, 10.0);
        renderer.render(&glyph_u, 130.0, 10.0);
        renderer.render(&glyph_s, 250.0, 10.0);
        renderer.render(&glyph_t, 370.0, 10.0);

        window.swap_buffers().unwrap();

        match event {
            glutin::Event::Closed => break,
            _ => ()
        }
    }
}

