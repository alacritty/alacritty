use std::mem::size_of;
use std::ptr;

use memoffset::offset_of;

use alacritty_terminal::term::SizeInfo;

use crate::gl;
use crate::gl::types::*;
use crate::renderer::rects::RenderRect;
use crate::renderer::{create_program, create_shader, Error, ShaderCreationError};

/// Maxmimum number of rect vertices per batch. Limited by 16-bit integer indices.
const MAX_VERTICES: usize = u16::max_value() as usize;

/// Shader sources for rect rendering program.
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

/// Vertex to store solid-color rectangle data.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Vertex {
    // Normalized screen coordinates.
    // TODO these can certainly be i16.
    x: f32,
    y: f32,

    // Color.
    color: Rgba,
}

/// Structure to store and group together rect-related vertices data and GL objects.
#[derive(Debug)]
pub struct RectRenderer {
    // GL buffer objects. VAO stores attribute and ebo bindings.
    vao: GLuint,
    vbo: GLuint,
    ebo: GLuint,

    program: RectShaderProgram,

    // Accumulated vertices data. Allocated space reused between frames.
    vertices: Vec<Vertex>,
}

impl RectRenderer {
    /// Update program when doing live-shader-reload.
    pub fn set_program(&mut self, program: RectShaderProgram) {
        self.program = program;
    }

    pub fn new() -> Result<Self, Error> {
        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;
        let mut ebo: GLuint = 0;
        let program = RectShaderProgram::new()?;

        // Pre-generate the entire index buffer for max possible rects.
        let mut indices = Vec::<u16>::new();
        let max_rects = MAX_VERTICES / 4;
        indices.reserve(max_rects * 6);
        for index in 0..max_rects {
            let index = index as u16 * 4;
            indices.extend_from_slice(&[
                index,
                index + 1,
                index + 2,
                index + 2,
                index + 3,
                index + 1,
            ]);
        }

        unsafe {
            // Allocate buffers.
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::GenBuffers(1, &mut ebo);

            // Bind EBO to VAO
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);

            // Pre-upload index buffer for max possible rects.
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                (indices.len() * std::mem::size_of::<u16>()) as isize,
                indices.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            // VBO binding is not part ot VAO, but attributes are.
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

        Ok(Self { vao, vbo, ebo, program, vertices: Vec::new() })
    }

    pub fn draw(&mut self, size_info: &SizeInfo, rects: Vec<RenderRect>) {
        // Setup bindings. VAO will set up attribs and EBO, but not VBO.
        unsafe {
            gl::BindVertexArray(self.vao);

            // Bind VBO only once for buffer data upload in draw_accumulated.
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);

            gl::UseProgram(self.program.id);
        }

        let center_x = size_info.width() / 2.;
        let center_y = size_info.height() / 2.;

        for rect in &rects {
            self.append_rect(center_x, center_y, rect);
            if self.vertices.len() == MAX_VERTICES {
                self.draw_accumulated();
            }
        }

        if !self.vertices.is_empty() {
            self.draw_accumulated();
        }

        unsafe {
            // Disable program.
            gl::UseProgram(0);

            // Reset buffer bindings to nothing.
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
        }
    }

    fn append_rect(&mut self, center_x: f32, center_y: f32, rect: &RenderRect) {
        // Make sure there's a space for at least 4 rectangle vertices.
        debug_assert!(self.vertices.len() <= MAX_VERTICES - 4);

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

        // Append rect vertices.
        self.vertices.extend_from_slice(&[
            Vertex { x, y, color },
            Vertex { x, y: y - height, color },
            Vertex { x: x + width, y, color },
            Vertex { x: x + width, y: y - height, color },
        ]);
    }

    fn draw_accumulated(&mut self) {
        unsafe {
            // Upload accumulated vertices.
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (self.vertices.len() * std::mem::size_of::<Vertex>()) as isize,
                self.vertices.as_ptr() as *const _,
                gl::STREAM_DRAW,
            );

            // Draw rectangles. Every 4 vertices form a rectangle, and each rectangle needs 6
            // indices (2 triangles).
            let rects = self.vertices.len() / 4;
            gl::DrawElements(gl::TRIANGLES, (rects * 6) as i32, gl::UNSIGNED_SHORT, ptr::null());
        }

        // Mark vertices buffer as empty and ready for next batch of vertices.
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
