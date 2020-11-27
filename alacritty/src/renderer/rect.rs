use std::mem::size_of;

use alacritty_terminal::term::SizeInfo;

use crate::gl;
use crate::gl::types::*;
use crate::renderer;
use crate::renderer::rects::RenderRect;

/// Shader sources for rect rendering program.
pub static RECT_SHADER_F_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/rect.f.glsl");
pub static RECT_SHADER_V_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/rect.v.glsl");
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

/// Struct that stores vertex 2D coordinates and color for rect rendering.
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

/// Struct to group together rect-related GL objects and rendering functionality.
#[derive(Debug)]
pub struct RectRenderer {
    // GL buffer objects. VAO stores vertex attributes binding.
    vao: GLuint,
    vbo: GLuint,

    program: RectShaderProgram,
}

impl RectRenderer {
    /// Update program when doing live-shader-reload.
    pub fn set_program(&mut self, program: RectShaderProgram) {
        self.program = program;
    }

    pub fn new() -> Result<Self, renderer::Error> {
        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;
        let program = RectShaderProgram::new()?;

        unsafe {
            // Allocate buffers.
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);

            // VBO binding is not part ot VAO itself, but VBO binding is stored in attributes.
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            let mut index = 0;
            let mut size = 0;

            macro_rules! add_attr {
                ($count:expr, $gl_type:expr, $normalize:expr, $type:ty) => {
                    gl::VertexAttribPointer(
                        index,
                        $count,
                        $gl_type,
                        $normalize,
                        size_of::<Vertex>() as i32,
                        size as *const _,
                    );
                    gl::EnableVertexAttribArray(index);

                    #[allow(unused_assignments)]
                    {
                        size += $count * size_of::<$type>();
                        index += 1;
                    }
                };
            }

            // Position.
            add_attr!(2, gl::FLOAT, gl::FALSE, f32);

            // Color.
            add_attr!(4, gl::UNSIGNED_BYTE, gl::TRUE, u8);

            // Reset buffer bindings.
            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        Ok(Self { vao, vbo, program })
    }

    pub fn draw(&mut self, size_info: &SizeInfo, rects: Vec<RenderRect>) {
        unsafe {
            // Bind VAO to enable vertex attribute slots specified in new().
            gl::BindVertexArray(self.vao);

            // Bind VBO only once for buffer data upload only.
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);

            gl::UseProgram(self.program.id);
        }

        let center_x = size_info.width() / 2.;
        let center_y = size_info.height() / 2.;

        // Build rect vertices vector.
        let mut vertices = RectVertices::new(rects.len());
        for rect in &rects {
            vertices.add_rect(center_x, center_y, rect);
        }

        unsafe {
            // Upload and render accumulated vertices.
            vertices.draw();

            // Disable program.
            gl::UseProgram(0);

            // Reset buffer bindings to nothing.
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
        }
    }
}

/// Helper struct to hold transient vertices for rendering.
struct RectVertices {
    vertices: Vec<Vertex>,
}

impl RectVertices {
    fn new(rects: usize) -> Self {
        let mut vertices = Vec::new();
        vertices.reserve(rects * 6);
        Self { vertices }
    }

    fn add_rect(&mut self, center_x: f32, center_y: f32, rect: &RenderRect) {
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

        // Make quad vertices.
        let quad = [
            Vertex { x, y, color },
            Vertex { x, y: y - height, color },
            Vertex { x: x + width, y, color },
            Vertex { x: x + width, y: y - height, color },
        ];

        // Append the vertices to form two triangles.
        self.vertices.push(quad[0]);
        self.vertices.push(quad[1]);
        self.vertices.push(quad[2]);
        self.vertices.push(quad[2]);
        self.vertices.push(quad[3]);
        self.vertices.push(quad[1]);
    }

    unsafe fn draw(&self) {
        // Upload accumulated vertices.
        gl::BufferData(
            gl::ARRAY_BUFFER,
            (self.vertices.len() * std::mem::size_of::<Vertex>()) as isize,
            self.vertices.as_ptr() as *const _,
            gl::STREAM_DRAW,
        );

        // Draw all vertices as list of triangles.
        gl::DrawArrays(gl::TRIANGLES, 0, self.vertices.len() as i32);
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
        let (vertex_src, fragment_src) = if cfg!(feature = "live-shader-reload") {
            (None, None)
        } else {
            (Some(RECT_SHADER_V), Some(RECT_SHADER_F))
        };
        let vertex_shader =
            renderer::create_shader(RECT_SHADER_V_PATH, gl::VERTEX_SHADER, vertex_src)?;
        let fragment_shader =
            renderer::create_shader(RECT_SHADER_F_PATH, gl::FRAGMENT_SHADER, fragment_src)?;
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
