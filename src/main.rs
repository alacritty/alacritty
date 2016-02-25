extern crate fontconfig;
extern crate freetype;
extern crate libc;
extern crate glutin;
extern crate gl;
extern crate cgmath;
extern crate euclid;

mod list_fonts;
mod text;

use std::ffi::CString;
use std::mem::size_of;
use std::ptr;

use euclid::{Rect, Size2D, Point2D};

use libc::c_void;

use gl::types::*;

use cgmath::Matrix;

use text::RasterizedGlyph;

static TEXT_SHADER_V: &'static str = include_str!("../res/text.v.glsl");
static TEXT_SHADER_F: &'static str = include_str!("../res/text.f.glsl");

fn main() {
    let window = glutin::Window::new().unwrap();
    let (width, height) = window.get_inner_size_pixels().unwrap();
    unsafe {
        window.make_current()
    };

    unsafe {
        gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);
        gl::Viewport(0, 0, width as i32, height as i32);
    }

    let rasterizer = text::Rasterizer::new();
    let glyph_j = rasterizer.get_glyph(180., 'J');

    let tex = AlphaTexture::new(
        glyph_j.width as i32,
        glyph_j.height as i32,
        glyph_j.buf.as_ptr() as *const _
    );

    unsafe {
        gl::Enable(gl::BLEND);
        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
    }

    let mut vao: GLuint = 0;
    let mut vbo: GLuint = 0;
    let mut ebo: GLuint = 0;
    unsafe {
        gl::GenVertexArrays(1, &mut vao);
        gl::GenBuffers(1, &mut vbo);
        gl::GenBuffers(1, &mut ebo);
        gl::BindVertexArray(vao);

        gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
        gl::BufferData(
            gl::ARRAY_BUFFER,
            (size_of::<f32>() * 4 * 4) as GLsizeiptr,
            ptr::null(),
            gl::DYNAMIC_DRAW
        );

        let indices: [u32; 6] = [0, 1, 3,
                                 1, 2, 3];

        gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
        gl::BufferData(gl::ELEMENT_ARRAY_BUFFER,
                       6 * size_of::<u32>() as isize,
                       indices.as_ptr() as *const _,
                       gl::STATIC_DRAW);

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, 4 * size_of::<f32>() as i32,
                                ptr::null());

        gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
        gl::BindVertexArray(0);
    }

    let program = ShaderProgram::new(width, height);

    for event in window.wait_events() {
        unsafe {
            gl::ClearColor(0.08, 0.08, 0.08, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        render(&program, &glyph_j, &tex, vbo, vao, ebo);

        window.swap_buffers();

        match event {
            glutin::Event::Closed => break,
            _ => ()
        }
    }
}

fn get_rect(glyph: &RasterizedGlyph, x: f32, y: f32) -> Rect<f32> {
    Rect::new(
        Point2D::new(x, y),
        Size2D::new(glyph.width as f32, glyph.height as f32)
    )
}

/// Render a character
fn render(program: &ShaderProgram, glyph: &RasterizedGlyph, tex: &AlphaTexture, vbo: GLuint,
          vao: GLuint, ebo: GLuint)
{
    program.activate();
    unsafe {
        // set color
        gl::Uniform3f(program.color, 1., 1., 0.5);
    }

    let rect = get_rect(glyph, 10.0, 10.0);

    // top right of character
    let vertices: [[f32; 4]; 4] = [
        [rect.max_x(), rect.max_y(), 1., 0.],
        [rect.max_x(), rect.min_y(), 1., 1.],
        [rect.min_x(), rect.min_y(), 0., 1.],
        [rect.min_x(), rect.max_y(), 0., 0.],
    ];

    unsafe {
        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindVertexArray(vao);

        gl::BindTexture(gl::TEXTURE_2D, tex.id);
        gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
        gl::BufferSubData(
            gl::ARRAY_BUFFER,
            0,
            (4 * 4 * size_of::<f32>()) as isize,
            vertices.as_ptr() as *const _
        );
        gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);

        gl::DrawElements(gl::TRIANGLES, 6, gl::UNSIGNED_INT, ptr::null());
        gl::BindVertexArray(0);
        gl::BindTexture(gl::TEXTURE_2D, 0);
    }

    program.deactivate();
}

pub struct ShaderProgram {
    id: GLuint,
    /// uniform location for projection matrix
    projection: GLint,
    /// uniform location foyr textColor
    color: GLint,
}

impl ShaderProgram {
    pub fn activate(&self) {
        unsafe {
            gl::UseProgram(self.id);
        }
    }

    pub fn deactivate(&self) {
        unsafe {
            gl::UseProgram(0);
        }
    }

    pub fn new(width: u32, height: u32) -> ShaderProgram {
        let vertex_shader = ShaderProgram::create_vertex_shader();
        let fragment_shader = ShaderProgram::create_fragment_shader();
        let program = ShaderProgram::create_program(vertex_shader, fragment_shader);

        unsafe {
            gl::DeleteShader(vertex_shader);
            gl::DeleteShader(fragment_shader);
        }

        // get uniform locations
        let projection_str = CString::new("projection").unwrap();
        let color_str = CString::new("textColor").unwrap();

        let (projection, color) = unsafe {
            (
                gl::GetUniformLocation(program, projection_str.as_ptr()),
                gl::GetUniformLocation(program, color_str.as_ptr())
            )
        };

        assert!(projection != gl::INVALID_VALUE as i32);
        assert!(projection != gl::INVALID_OPERATION as i32);
        assert!(color != gl::INVALID_VALUE as i32);
        assert!(color != gl::INVALID_OPERATION as i32);

        let shader = ShaderProgram {
            id: program,
            projection: projection,
            color: color,
        };

        // set projection uniform
        let ortho = cgmath::ortho(0., width as f32, 0., height as f32, -1., 1.);
        let projection: [[f32; 4]; 4] = ortho.into();

        println!("width: {}, height: {}", width, height);

        shader.activate();
        unsafe {
            gl::UniformMatrix4fv(shader.projection, 1, gl::FALSE, projection.as_ptr() as *const _);
        }
        shader.deactivate();

        shader
    }

    fn create_program(vertex: GLuint, fragment: GLuint) -> GLuint {

        unsafe {
            let program = gl::CreateProgram();
            gl::AttachShader(program, vertex);
            gl::AttachShader(program, fragment);
            gl::LinkProgram(program);

            let mut success: GLint = 0;
            gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);

            if success != (gl::TRUE as GLint) {
                panic!("failed to link shader program");
            }
            program
        }
    }

    fn create_fragment_shader() -> GLuint {
        unsafe {
            let fragment_shader = gl::CreateShader(gl::FRAGMENT_SHADER);
            let fragment_source = CString::new(TEXT_SHADER_F).unwrap();
            gl::ShaderSource(fragment_shader, 1, &fragment_source.as_ptr(), ptr::null());
            gl::CompileShader(fragment_shader);

            let mut success: GLint = 0;
            gl::GetShaderiv(fragment_shader, gl::COMPILE_STATUS, &mut success);

            if success != (gl::TRUE as GLint) {
                panic!("failed to compiler fragment shader");
            }
            fragment_shader
        }
    }

    fn create_vertex_shader() -> GLuint {
        unsafe {
            let vertex_shader = gl::CreateShader(gl::VERTEX_SHADER);
            let vertex_source = CString::new(TEXT_SHADER_V).unwrap();
            gl::ShaderSource(vertex_shader, 1, &vertex_source.as_ptr(), ptr::null());
            gl::CompileShader(vertex_shader);

            let mut success: GLint = 0;
            gl::GetShaderiv(vertex_shader, gl::COMPILE_STATUS, &mut success);

            if success != (gl::TRUE as GLint) {
                panic!("failed to compiler vertex shader");
            }
            vertex_shader
        }
    }
}

struct AlphaTexture {
    pub id: GLuint,
    pub width: i32,
    pub height: i32,
}

impl AlphaTexture {
    pub fn new(width: i32, height: i32, bytes: *const ::std::os::raw::c_void) -> AlphaTexture {
        let mut id: GLuint = 0;
        unsafe {
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::GenTextures(1, &mut id);
            gl::BindTexture(gl::TEXTURE_2D, id);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RED as i32,
                width as i32,
                height as i32,
                0,
                gl::RED,
                gl::UNSIGNED_BYTE,
                bytes
            );

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);

            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        AlphaTexture {
            id: id,
            width: width,
            height: height,
        }
    }
}
