use std::mem::size_of;
use std::ptr;

use crossfont::RasterizedGlyph;
use log::info;

use alacritty_terminal::term::cell::Flags;

use crate::display::SizeInfo;
use crate::display::content::RenderableCell;
use crate::gl;
use crate::gl::types::*;
use crate::renderer::shader::{ShaderProgram, ShaderVersion};
use crate::renderer::{Error, GlExtensions};

use super::atlas::{ATLAS_SIZE, Atlas};
use super::{
    Glyph, LoadGlyph, LoaderApi, RenderingGlyphFlags, RenderingPass, TextRenderApi,
    TextRenderBatch, TextRenderer, TextShader, glsl3,
};

// Shader source.
const TEXT_SHADER_F: &str = include_str!("../../../res/gles2/text.f.glsl");
const TEXT_SHADER_V: &str = include_str!("../../../res/gles2/text.v.glsl");

#[derive(Debug)]
pub struct Gles2Renderer {
    program: TextShaderProgram,
    vao: GLuint,
    vbo: GLuint,
    ebo: GLuint,
    atlas: Vec<Atlas>,
    batch: Batch,
    current_atlas: usize,
    active_tex: GLuint,
    dual_source_blending: bool,
}

impl Gles2Renderer {
    pub fn new(allow_dsb: bool, is_gles_context: bool) -> Result<Self, Error> {
        info!("Using OpenGL ES 2.0 renderer");

        let dual_source_blending = allow_dsb
            && (GlExtensions::contains("GL_EXT_blend_func_extended")
                || GlExtensions::contains("GL_ARB_blend_func_extended"));

        if is_gles_context {
            info!("Running on OpenGL ES context");
        }

        if dual_source_blending {
            info!("Using dual source blending");
        }

        let program = TextShaderProgram::new(ShaderVersion::Gles2, dual_source_blending)?;
        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;
        let mut ebo: GLuint = 0;

        let mut vertex_indices = Vec::with_capacity(BATCH_MAX / 4 * 6);
        for index in 0..(BATCH_MAX / 4) as u16 {
            let index = index * 4;
            vertex_indices.push(index);
            vertex_indices.push(index + 1);
            vertex_indices.push(index + 3);

            vertex_indices.push(index + 1);
            vertex_indices.push(index + 2);
            vertex_indices.push(index + 3);
        }

        unsafe {
            gl::Enable(gl::BLEND);

            gl::DepthMask(gl::FALSE);

            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut ebo);
            gl::GenBuffers(1, &mut vbo);
            gl::BindVertexArray(vao);

            // Elements buffer.
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                (vertex_indices.capacity() * size_of::<u16>()) as isize,
                vertex_indices.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            // Vertex buffer.
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (BATCH_MAX * size_of::<TextVertex>()) as isize,
                ptr::null(),
                gl::STREAM_DRAW,
            );

            let mut index = 0;
            let mut size = 0;

            macro_rules! add_attr {
                ($count:expr, $gl_type:expr, $type:ty) => {
                    gl::VertexAttribPointer(
                        index,
                        $count,
                        $gl_type,
                        gl::FALSE,
                        size_of::<TextVertex>() as i32,
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

            // Cell coords.
            add_attr!(2, gl::SHORT, i16);

            // Glyph coords.
            add_attr!(2, gl::SHORT, i16);

            // UV.
            add_attr!(2, gl::FLOAT, u32);

            // Color and bitmap color.
            //
            // These are packed together because of an OpenGL driver issue on macOS, which caused a
            // `vec3(u8)` text color and a `u8` for glyph color to cause performance regressions.
            add_attr!(4, gl::UNSIGNED_BYTE, u8);

            // Background color.
            add_attr!(4, gl::UNSIGNED_BYTE, u8);

            // Cleanup.
            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        Ok(Self {
            program,
            vao,
            vbo,
            ebo,
            atlas: vec![Atlas::new(ATLAS_SIZE, is_gles_context)],
            batch: Batch::new(),
            current_atlas: 0,
            active_tex: 0,
            dual_source_blending,
        })
    }
}

impl Drop for Gles2Renderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteBuffers(1, &self.ebo);
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}

impl<'a> TextRenderer<'a> for Gles2Renderer {
    type RenderApi = RenderApi<'a>;
    type RenderBatch = Batch;
    type Shader = TextShaderProgram;

    fn program(&self) -> &Self::Shader {
        &self.program
    }

    fn with_api<'b: 'a, F, T>(&'b mut self, _: &'b SizeInfo, func: F) -> T
    where
        F: FnOnce(Self::RenderApi) -> T,
    {
        unsafe {
            gl::UseProgram(self.program.id());
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.ebo);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::ActiveTexture(gl::TEXTURE0);
        }

        let res = func(RenderApi {
            active_tex: &mut self.active_tex,
            batch: &mut self.batch,
            atlas: &mut self.atlas,
            current_atlas: &mut self.current_atlas,
            program: &mut self.program,
            dual_source_blending: self.dual_source_blending,
        });

        unsafe {
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            gl::UseProgram(0);
        }

        res
    }

    fn loader_api(&mut self) -> LoaderApi<'_> {
        LoaderApi {
            active_tex: &mut self.active_tex,
            atlas: &mut self.atlas,
            current_atlas: &mut self.current_atlas,
        }
    }
}

/// Maximum items to be drawn in a batch.
///
/// We use the closest number to `u16::MAX` dividable by 4 (amount of vertices we push for a glyph),
/// since it's the maximum possible index in `glDrawElements` in GLES2.
const BATCH_MAX: usize = (u16::MAX - u16::MAX % 4) as usize;

#[derive(Debug)]
pub struct Batch {
    tex: GLuint,
    vertices: Vec<TextVertex>,
}

impl Batch {
    fn new() -> Self {
        Self { tex: 0, vertices: Vec::with_capacity(BATCH_MAX) }
    }

    #[inline]
    fn len(&self) -> usize {
        self.vertices.len()
    }

    #[inline]
    fn capacity(&self) -> usize {
        BATCH_MAX
    }

    #[inline]
    fn size(&self) -> usize {
        self.len() * size_of::<TextVertex>()
    }

    #[inline]
    fn clear(&mut self) {
        self.vertices.clear();
    }
}

impl TextRenderBatch for Batch {
    #[inline]
    fn tex(&self) -> GLuint {
        self.tex
    }

    #[inline]
    fn full(&self) -> bool {
        self.capacity() == self.len()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn add_item(&mut self, cell: &RenderableCell, glyph: &Glyph, size_info: &SizeInfo) {
        if self.is_empty() {
            self.tex = glyph.tex_id;
        }

        // Calculate the cell position.
        let x = cell.point.column.0 as i16 * size_info.cell_width() as i16;
        let y = cell.point.line as i16 * size_info.cell_height() as i16;

        // Calculate the glyph position.
        let glyph_x = cell.point.column.0 as i16 * size_info.cell_width() as i16 + glyph.left;
        let glyph_y = (cell.point.line + 1) as i16 * size_info.cell_height() as i16 - glyph.top;

        let colored = if glyph.multicolor {
            RenderingGlyphFlags::COLORED
        } else {
            RenderingGlyphFlags::empty()
        };

        let is_wide = if cell.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };

        let mut vertex = TextVertex {
            x,
            y: y + size_info.cell_height() as i16,

            glyph_x,
            glyph_y: glyph_y + glyph.height,

            u: glyph.uv_left,
            v: glyph.uv_bot + glyph.uv_height,
            r: cell.fg.r,
            g: cell.fg.g,
            b: cell.fg.b,
            colored,
            bg_r: cell.bg.r,
            bg_g: cell.bg.g,
            bg_b: cell.bg.b,
            bg_a: (cell.bg_alpha * 255.0) as u8,
        };

        self.vertices.push(vertex);

        vertex.y = y;
        vertex.glyph_y = glyph_y;
        vertex.u = glyph.uv_left;
        vertex.v = glyph.uv_bot;
        self.vertices.push(vertex);

        vertex.x = x + is_wide * size_info.cell_width() as i16;
        vertex.glyph_x = glyph_x + glyph.width;
        vertex.u = glyph.uv_left + glyph.uv_width;
        vertex.v = glyph.uv_bot;
        self.vertices.push(vertex);

        vertex.x = x + is_wide * size_info.cell_width() as i16;
        vertex.y = y + size_info.cell_height() as i16;
        vertex.glyph_x = glyph_x + glyph.width;
        vertex.glyph_y = glyph_y + glyph.height;
        vertex.u = glyph.uv_left + glyph.uv_width;
        vertex.v = glyph.uv_bot + glyph.uv_height;
        self.vertices.push(vertex);
    }
}

#[derive(Debug)]
pub struct RenderApi<'a> {
    active_tex: &'a mut GLuint,
    batch: &'a mut Batch,
    atlas: &'a mut Vec<Atlas>,
    current_atlas: &'a mut usize,
    program: &'a mut TextShaderProgram,
    dual_source_blending: bool,
}

impl Drop for RenderApi<'_> {
    fn drop(&mut self) {
        if !self.batch.is_empty() {
            self.render_batch();
        }
    }
}

impl LoadGlyph for RenderApi<'_> {
    fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph {
        Atlas::load_glyph(self.active_tex, self.atlas, self.current_atlas, rasterized)
    }

    fn clear(&mut self) {
        Atlas::clear_atlas(self.atlas, self.current_atlas)
    }
}

impl TextRenderApi<Batch> for RenderApi<'_> {
    fn batch(&mut self) -> &mut Batch {
        self.batch
    }

    fn render_batch(&mut self) {
        unsafe {
            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                self.batch.size() as isize,
                self.batch.vertices.as_ptr() as *const _,
            );
        }

        if *self.active_tex != self.batch.tex() {
            unsafe {
                gl::BindTexture(gl::TEXTURE_2D, self.batch.tex());
            }
            *self.active_tex = self.batch.tex();
        }

        unsafe {
            let num_indices = (self.batch.len() / 4 * 6) as i32;

            // The rendering is inspired by
            // https://github.com/servo/webrender/blob/master/webrender/doc/text-rendering.md.

            // Draw background.
            self.program.set_rendering_pass(RenderingPass::Background);
            gl::BlendFunc(gl::ONE, gl::ZERO);
            gl::DrawElements(gl::TRIANGLES, num_indices, gl::UNSIGNED_SHORT, ptr::null());

            self.program.set_rendering_pass(RenderingPass::SubpixelPass1);
            if self.dual_source_blending {
                // Text rendering pass.
                gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
            } else {
                // First text rendering pass.
                gl::BlendFuncSeparate(gl::ZERO, gl::ONE_MINUS_SRC_COLOR, gl::ZERO, gl::ONE);
                gl::DrawElements(gl::TRIANGLES, num_indices, gl::UNSIGNED_SHORT, ptr::null());

                // Second text rendering pass.
                self.program.set_rendering_pass(RenderingPass::SubpixelPass2);
                gl::BlendFuncSeparate(gl::ONE_MINUS_DST_ALPHA, gl::ONE, gl::ZERO, gl::ONE);
                gl::DrawElements(gl::TRIANGLES, num_indices, gl::UNSIGNED_SHORT, ptr::null());

                // Third text rendering pass.
                self.program.set_rendering_pass(RenderingPass::SubpixelPass3);
                gl::BlendFuncSeparate(gl::ONE, gl::ONE, gl::ONE, gl::ONE_MINUS_SRC_ALPHA);
            }

            gl::DrawElements(gl::TRIANGLES, num_indices, gl::UNSIGNED_SHORT, ptr::null());
        }

        self.batch.clear();
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct TextVertex {
    // Cell coordinates.
    x: i16,
    y: i16,

    // Glyph coordinates.
    glyph_x: i16,
    glyph_y: i16,

    // Offsets into Atlas.
    u: f32,
    v: f32,

    // Color.
    r: u8,
    g: u8,
    b: u8,

    // Whether the glyph is colored.
    colored: RenderingGlyphFlags,

    // Background color.
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
    bg_a: u8,
}

#[derive(Debug)]
pub struct TextShaderProgram {
    /// Shader program.
    program: ShaderProgram,

    /// Projection scale and offset uniform.
    u_projection: GLint,

    /// Rendering pass.
    ///
    /// For dual source blending, there are 2 passes; one for background, another for text,
    /// similar to the GLSL3 renderer.
    ///
    /// If GL_EXT_blend_func_extended is not available, the rendering is split into 4 passes.
    /// One is used for the background and the rest to perform subpixel text rendering according to
    /// <https://github.com/servo/webrender/blob/master/webrender/doc/text-rendering.md>.
    ///
    /// Rendering is split into three passes.
    u_rendering_pass: GLint,
}

impl TextShaderProgram {
    pub fn new(shader_version: ShaderVersion, dual_source_blending: bool) -> Result<Self, Error> {
        let fragment_shader =
            if dual_source_blending { &glsl3::TEXT_SHADER_F } else { &TEXT_SHADER_F };

        let program = ShaderProgram::new(shader_version, None, TEXT_SHADER_V, fragment_shader)?;

        Ok(Self {
            u_projection: program.get_uniform_location(c"projection")?,
            u_rendering_pass: program.get_uniform_location(c"renderingPass")?,
            program,
        })
    }

    fn set_rendering_pass(&self, rendering_pass: RenderingPass) {
        unsafe { gl::Uniform1i(self.u_rendering_pass, rendering_pass as i32) }
    }
}

impl TextShader for TextShaderProgram {
    fn id(&self) -> GLuint {
        self.program.id()
    }

    fn projection_uniform(&self) -> GLint {
        self.u_projection
    }
}
