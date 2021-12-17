use std::mem;

use alacritty_terminal::term::color::Rgb;
use alacritty_terminal::term::SizeInfo;

use crate::gl::types::*;
use crate::text_run::TextRun;
use crate::{gl, renderer};

#[derive(Debug, Copy, Clone)]
pub struct RenderRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Rgb,
    pub alpha: f32,
}

impl RenderRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: Rgb, alpha: f32) -> Self {
        RenderRect { x, y, width, height, color, alpha }
    }

    pub fn from_text_run(
        text_run: &TextRun,
        (descent, position, thickness): (f32, f32, f32),
        size: &SizeInfo,
    ) -> Self {
        let start_point = text_run.start_point();
        let start_x = start_point.column.0 as f32 * size.cell_width();
        let end_x = (text_run.end_point().column.0 + 1) as f32 * size.cell_width();
        let width = end_x - start_x;
        let line_bottom = (start_point.line as f32 + 1.) * size.cell_height();
        let baseline = line_bottom + descent;
        let height = thickness.max(1.);
        let mut y = baseline - position - height / 2.;
        let max_y = line_bottom - height;
        y = y.min(max_y);

        RenderRect::new(
            start_x + size.padding_x(),
            y + size.padding_y(),
            width,
            height,
            text_run.fg,
            1.,
        )
    }
}

/// Shader sources for rect rendering program.
static RECT_SHADER_F: &str = include_str!("../../res/rect.f.glsl");
static RECT_SHADER_V: &str = include_str!("../../res/rect.v.glsl");

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Vertex {
    // Normalized screen coordinates.
    x: f32,
    y: f32,

    // Color.
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(Debug)]
pub struct RectRenderer {
    // GL buffer objects.
    vao: GLuint,
    vbo: GLuint,

    program: RectShaderProgram,

    vertices: Vec<Vertex>,
}

impl RectRenderer {
    pub fn new() -> Result<Self, renderer::Error> {
        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;
        let program = RectShaderProgram::new()?;

        unsafe {
            // Allocate buffers.
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);

            // VBO binding is not part of VAO itself, but VBO binding is stored in attributes.
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            let mut attribute_offset = 0;

            // Position.
            gl::VertexAttribPointer(
                0,
                2,
                gl::FLOAT,
                gl::FALSE,
                mem::size_of::<Vertex>() as i32,
                attribute_offset as *const _,
            );
            gl::EnableVertexAttribArray(0);
            attribute_offset += mem::size_of::<f32>() * 2;

            // Color.
            gl::VertexAttribPointer(
                1,
                4,
                gl::UNSIGNED_BYTE,
                gl::TRUE,
                mem::size_of::<Vertex>() as i32,
                attribute_offset as *const _,
            );
            gl::EnableVertexAttribArray(1);

            // Reset buffer bindings.
            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        Ok(Self { vao, vbo, program, vertices: Vec::new() })
    }

    pub fn draw(&mut self, size_info: &SizeInfo, rects: Vec<RenderRect>) {
        unsafe {
            // Bind VAO to enable vertex attribute slots.
            gl::BindVertexArray(self.vao);

            // Bind VBO only once for buffer data upload only.
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);

            gl::UseProgram(self.program.id);
        }

        let half_width = size_info.width() / 2.;
        let half_height = size_info.height() / 2.;

        // Build rect vertices vector.
        self.vertices.clear();
        for rect in &rects {
            self.add_rect(half_width, half_height, rect);
        }

        unsafe {
            // Upload accumulated vertices.
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (self.vertices.len() * mem::size_of::<Vertex>()) as isize,
                self.vertices.as_ptr() as *const _,
                gl::STREAM_DRAW,
            );

            // Draw all vertices as list of triangles.
            gl::DrawArrays(gl::TRIANGLES, 0, self.vertices.len() as i32);

            // Disable program.
            gl::UseProgram(0);

            // Reset buffer bindings to nothing.
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
        }
    }

    fn add_rect(&mut self, half_width: f32, half_height: f32, rect: &RenderRect) {
        // Calculate rectangle vertices positions in normalized device coordinates.
        // NDC range from -1 to +1, with Y pointing up.
        let x = rect.x / half_width - 1.0;
        let y = -rect.y / half_height + 1.0;
        let width = rect.width / half_width;
        let height = rect.height / half_height;
        let Rgb { r, g, b } = rect.color;
        let a = (rect.alpha * 255.) as u8;

        // Make quad vertices.
        let quad = [
            Vertex { x, y, r, g, b, a },
            Vertex { x, y: y - height, r, g, b, a },
            Vertex { x: x + width, y, r, g, b, a },
            Vertex { x: x + width, y: y - height, r, g, b, a },
        ];

        // Append the vertices to form two triangles.
        self.vertices.push(quad[0]);
        self.vertices.push(quad[1]);
        self.vertices.push(quad[2]);
        self.vertices.push(quad[2]);
        self.vertices.push(quad[3]);
        self.vertices.push(quad[1]);
    }
}

impl Drop for RectRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}

/// Rectangle drawing program.
#[derive(Debug)]
pub struct RectShaderProgram {
    /// Program id.
    id: GLuint,
}

impl RectShaderProgram {
    pub fn new() -> Result<Self, renderer::ShaderCreationError> {
        let vertex_shader = renderer::create_shader(gl::VERTEX_SHADER, RECT_SHADER_V)?;
        let fragment_shader = renderer::create_shader(gl::FRAGMENT_SHADER, RECT_SHADER_F)?;
        let program = renderer::create_program(vertex_shader, fragment_shader)?;

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
