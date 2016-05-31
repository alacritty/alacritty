use std::ffi::CString;
use std::mem::size_of;
use std::ptr;

use gl;
use cgmath;

use euclid::{Rect, Size2D, Point2D};

use gl::types::*;

use cgmath::Matrix;

use text::RasterizedGlyph;

static TEXT_SHADER_V: &'static str = include_str!("../../res/text.v.glsl");
static TEXT_SHADER_F: &'static str = include_str!("../../res/text.f.glsl");

pub struct QuadRenderer {
    program: ShaderProgram,
    vao: GLuint,
    vbo: GLuint,
    ebo: GLuint,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct PackedVertex {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
}

use super::Rgb;

impl QuadRenderer {
    // TODO should probably hand this a transform instead of width/height
    pub fn new(width: u32, height: u32) -> QuadRenderer {
        let program = ShaderProgram::new(width, height);

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
                (size_of::<PackedVertex>() * 4) as GLsizeiptr,
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
            gl::EnableVertexAttribArray(1);

            // positions
            gl::VertexAttribPointer(0, 2,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<PackedVertex>() as i32,
                                    ptr::null());

            // uv mapping
            gl::VertexAttribPointer(1, 2,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<PackedVertex>() as i32,
                                    (2 * size_of::<f32>()) as *const _);

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
        }

        QuadRenderer {
            program: program,
            vao: vao,
            vbo: vbo,
            ebo: ebo,
        }
    }

    pub fn render(&self, glyph: &Glyph, x: f32, y: f32, color: &Rgb) {
        self.program.activate();
        unsafe {
            // set color
            gl::Uniform3i(self.program.u_color, color.r as i32, color.g as i32, color.b as i32);
        }

        let rect = get_rect(glyph, x, y);

        // Top right, Bottom right, Bottom left, Top left
        let packed = [
            PackedVertex { x: rect.max_x(), y: rect.max_y(), u: 1.0, v: 0.0, },
            PackedVertex { x: rect.max_x(), y: rect.min_y(), u: 1.0, v: 1.0, },
            PackedVertex { x: rect.min_x(), y: rect.min_y(), u: 0.0, v: 1.0, },
            PackedVertex { x: rect.min_x(), y: rect.max_y(), u: 0.0, v: 0.0, },
        ];

        unsafe {
            bind_mask_texture(glyph.tex_id);
            gl::BindVertexArray(self.vao);

            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                (packed.len() * size_of::<PackedVertex>()) as isize,
                packed.as_ptr() as *const _
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.ebo);

            gl::DrawElements(gl::TRIANGLES, 6, gl::UNSIGNED_INT, ptr::null());
            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        self.program.deactivate();
    }
}

fn get_rect(glyph: &Glyph, x: f32, y: f32) -> Rect<f32> {
    Rect::new(
        Point2D::new(x + glyph.left as f32, y - (glyph.height - glyph.top) as f32),
        Size2D::new(glyph.width as f32, glyph.height as f32)
    )
}

fn bind_mask_texture(id: u32) {
    unsafe {
        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, id);
    }
}

pub struct ShaderProgram {
    id: GLuint,
    /// projection matrix uniform
    u_projection: GLint,
    /// color uniform
    u_color: GLint,
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
                gl::GetUniformLocation(program, color_str.as_ptr()),
            )
        };

        assert!(projection != gl::INVALID_VALUE as i32);
        assert!(projection != gl::INVALID_OPERATION as i32);
        assert!(color != gl::INVALID_VALUE as i32);
        assert!(color != gl::INVALID_OPERATION as i32);

        let shader = ShaderProgram {
            id: program,
            u_projection: projection,
            u_color: color,
        };

        // set projection uniform
        let ortho = cgmath::ortho(0., width as f32, 0., height as f32, -1., 1.);
        let projection: [[f32; 4]; 4] = ortho.into();

        println!("width: {}, height: {}", width, height);

        shader.activate();
        unsafe {
            gl::UniformMatrix4fv(shader.u_projection,
                                 1, gl::FALSE, projection.as_ptr() as *const _);
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

#[allow(dead_code)]
pub struct Glyph {
    tex_id: GLuint,
    top: i32,
    left: i32,
    width: i32,
    height: i32,
}

impl Glyph {
    pub fn new(rasterized: &RasterizedGlyph) -> Glyph {
        let mut id: GLuint = 0;
        unsafe {
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::GenTextures(1, &mut id);
            gl::BindTexture(gl::TEXTURE_2D, id);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB as i32,
                rasterized.width as i32,
                rasterized.height as i32,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                rasterized.buf.as_ptr() as *const _
            );

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);

            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        Glyph {
            tex_id: id,
            top: rasterized.top as i32,
            width: rasterized.width as i32,
            height: rasterized.height as i32,
            left: rasterized.left as i32,
        }
    }
}
