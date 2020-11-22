use alacritty_terminal::term::SizeInfo;

use crate::gl;
use crate::gl::types::*;
use crate::renderer::rects::RenderRect;
use crate::renderer::{create_program, create_shader, Error, ShaderCreationError};

use std::mem::size_of;
use std::ptr;

const MAX_U16_INDICES: usize = 65536;

static RECT_SHADER_F_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/rect.f.glsl");
static RECT_SHADER_V_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/rect.v.glsl");
static RECT_SHADER_F: &str = include_str!("../../res/rect.f.glsl");
static RECT_SHADER_V: &str = include_str!("../../res/rect.v.glsl");

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Vertex {
    // TODO these can certainly be i16.
    x: f32,
    y: f32,
    color: Rgba,
}

#[derive(Debug)]
pub struct SolidRectRenderer {
    vao: GLuint,
    vbo: GLuint,
    ebo: GLuint,

    program: RectShaderProgram,

    uploaded_indices: usize,
    indices: Vec<u16>,
    vertices: Vec<Vertex>,
}

impl SolidRectRenderer {
    pub fn set_program(&mut self, program: RectShaderProgram) {
        self.program = program;
    }

    pub fn new() -> Result<Self, Error> {
        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;
        let mut ebo: GLuint = 0;
        let program = RectShaderProgram::new()?;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::GenBuffers(1, &mut ebo);

            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);

            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            // Position.
            gl::VertexAttribPointer(
                0,
                2,
                gl::FLOAT,
                gl::FALSE,
                (size_of::<Vertex>()) as _,
                ptr::null(),
            );
            gl::EnableVertexAttribArray(0);

            // Color.
            gl::VertexAttribPointer(
                1,
                4,
                gl::UNSIGNED_BYTE,
                gl::TRUE,
                (size_of::<Vertex>()) as _,
                offset_of!(Vertex, color) as *const _,
            );
            gl::EnableVertexAttribArray(1);

            // Reset buffer bindings.
            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        Ok(Self {
            vao,
            vbo,
            ebo,
            program,
            indices: Vec::new(),
            vertices: Vec::new(),
            uploaded_indices: 0,
        })
    }

    pub fn draw(&mut self, size_info: &SizeInfo, rects: Vec<RenderRect>) {
        // Setup bindings. VAO will set up attribs and EBO, but not VBO.
        unsafe {
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);

            gl::UseProgram(self.program.id);
        }

        let center_x = size_info.width() / 2.;
        let center_y = size_info.height() / 2.;

        for rect in &rects {
            self.append_rect(center_x, center_y, rect);
        }

        self.draw_accumulated();

        unsafe {
            // Disable program.
            gl::UseProgram(0);

            // Reset buffer bindings to nothing.
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
        }
    }

    fn append_rect(&mut self, center_x: f32, center_y: f32, rect: &RenderRect) {
        assert!(self.vertices.len() <= MAX_U16_INDICES - 4);

        // Calculate rectangle position.
        let x = (rect.x - center_x) / center_x;
        let y = -(rect.y - center_y) / center_y;
        let width = rect.width / center_x;
        let height = rect.height / center_y;
        let color = Rgba {
            r: rect.color.r,
            g: rect.color.g,
            b: rect.color.b,
            a: (rect.alpha * 255.) as u8,
        };

        self.vertices.extend_from_slice(&[
            Vertex { x, y, color },
            Vertex { x, y: y - height, color },
            Vertex { x: x + width, y, color },
            Vertex { x: x + width, y: y - height, color },
        ]);

        if self.vertices.len() == MAX_U16_INDICES {
            self.draw_accumulated();
        }
    }

    fn draw_accumulated(&mut self) {
        if self.vertices.is_empty() {
            return;
        }

        // Generate new indices in index buffer on-demand
        assert!(self.indices.len() % 6 == 0);
        let generated_quads = (self.indices.len() / 6) as u16;
        let need_quads = (self.vertices.len() / 4) as u16;
        for index in generated_quads..need_quads {
            let index = index * 4;
            self.indices.extend_from_slice(&[
                index,
                index + 1,
                index + 2,
                index + 2,
                index + 3,
                index + 1,
            ]);
        }

        // Upload accumulated buffers.
        unsafe {
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (self.vertices.len() * std::mem::size_of::<Vertex>()) as isize,
                self.vertices.as_ptr() as *const _,
                gl::STREAM_DRAW,
            );

            // If we need more indices than have been already uploaded.
            if self.uploaded_indices < self.indices.len() {
                gl::BufferData(
                    gl::ELEMENT_ARRAY_BUFFER,
                    (self.indices.len() * std::mem::size_of::<u16>()) as isize,
                    self.indices.as_ptr() as *const _,
                    gl::STATIC_DRAW,
                );

                self.uploaded_indices = self.indices.len();
            }

            let quads = self.vertices.len() / 4;
            gl::DrawElements(gl::TRIANGLES, (quads * 6) as i32, gl::UNSIGNED_SHORT, ptr::null());
        }

        self.vertices.clear();
    }
}

/// Rectangle drawing program.
///
/// Uniforms are prefixed with "u".
#[derive(Debug)]
pub struct RectShaderProgram {
    /// Program id.
    id: GLuint,
}

impl RectShaderProgram {
    pub fn new() -> Result<Self, ShaderCreationError> {
        let (vertex_src, fragment_src) = if cfg!(feature = "live-shader-reload") {
            (None, None)
        } else {
            (Some(RECT_SHADER_V), Some(RECT_SHADER_F))
        };
        let vertex_shader = create_shader(RECT_SHADER_V_PATH, gl::VERTEX_SHADER, vertex_src)?;
        let fragment_shader = create_shader(RECT_SHADER_F_PATH, gl::FRAGMENT_SHADER, fragment_src)?;
        let program = create_program(vertex_shader, fragment_shader)?;

        unsafe {
            gl::DeleteShader(fragment_shader);
            gl::DeleteShader(vertex_shader);
            gl::UseProgram(program);
        }

        let shader = Self { id: program };

        unsafe { gl::UseProgram(0) }

        Ok(shader)
    }
}

impl Drop for RectShaderProgram {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.id);
        }
    }
}
