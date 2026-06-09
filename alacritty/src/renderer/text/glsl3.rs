use std::mem::size_of;
use std::ptr;

use crossfont::RasterizedGlyph;
use log::info;

use alacritty_terminal::term::cell::Flags;

use crate::display::SizeInfo;
use crate::display::content::RenderableCell;
use crate::gl;
use crate::gl::types::*;
use crate::renderer::Error;
use crate::renderer::shader::{ShaderProgram, ShaderVersion};

use super::atlas::{ATLAS_SIZE, Atlas};
use super::{
    Glyph, LoadGlyph, LoaderApi, RenderingGlyphFlags, RenderingPass, TextRenderApi,
    TextRenderBatch, TextRenderer, TextShader,
};

// Shader source.
pub const TEXT_SHADER_F: &str = include_str!("../../../res/glsl3/text.f.glsl");
const TEXT_SHADER_V: &str = include_str!("../../../res/glsl3/text.v.glsl");

/// Maximum items to be drawn in a batch.
const BATCH_MAX: usize = 0x1_0000;

#[derive(Debug)]
pub struct Glsl3Renderer {
    program: TextShaderProgram,
    vao: GLuint,
    ebo: GLuint,
    vbo_instance: GLuint,
    atlas: Vec<Atlas>,
    current_atlas: usize,
    active_tex: GLuint,
    batch: Batch,
}

impl Glsl3Renderer {
    pub fn new() -> Result<Self, Error> {
        info!("Using OpenGL 3.3 renderer");

        let program = TextShaderProgram::new(ShaderVersion::Glsl3)?;
        let mut vao: GLuint = 0;
        let mut ebo: GLuint = 0;
        let mut vbo_instance: GLuint = 0;

        unsafe {
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);

            // Disable depth mask, as the renderer never uses depth tests.
            gl::DepthMask(gl::FALSE);

            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut ebo);
            gl::GenBuffers(1, &mut vbo_instance);
            gl::BindVertexArray(vao);

            // ---------------------
            // Set up element buffer
            // ---------------------
            let indices: [u32; 6] = [0, 1, 3, 1, 2, 3];

            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                (6 * size_of::<u32>()) as isize,
                indices.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            // ----------------------------
            // Setup vertex instance buffer
            // ----------------------------
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_instance);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (BATCH_MAX * size_of::<InstanceData>()) as isize,
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
                        size_of::<InstanceData>() as i32,
                        size as *const _,
                    );
                    gl::EnableVertexAttribArray(index);
                    gl::VertexAttribDivisor(index, 1);

                    #[allow(unused_assignments)]
                    {
                        size += $count * size_of::<$type>();
                        index += 1;
                    }
                };
            }

            // Coords.
            add_attr!(2, gl::UNSIGNED_SHORT, u16);

            // Glyph offset and size.
            add_attr!(4, gl::SHORT, i16);

            // UV offset.
            add_attr!(4, gl::FLOAT, f32);

            // Color and cell flags.
            //
            // These are packed together because of an OpenGL driver issue on macOS, which caused a
            // `vec3(u8)` text color and a `u8` cell flags to increase the rendering time by a
            // huge margin.
            add_attr!(4, gl::UNSIGNED_BYTE, u8);

            // Background color.
            add_attr!(4, gl::UNSIGNED_BYTE, u8);

            // Cleanup.
            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
        }

        Ok(Self {
            program,
            vao,
            ebo,
            vbo_instance,
            atlas: vec![Atlas::new(ATLAS_SIZE, false)],
            current_atlas: 0,
            active_tex: 0,
            batch: Batch::new(),
        })
    }
}

impl<'a> TextRenderer<'a> for Glsl3Renderer {
    type RenderApi = RenderApi<'a>;
    type RenderBatch = Batch;
    type Shader = TextShaderProgram;

    fn with_api<'b: 'a, F, T>(&'b mut self, size_info: &'b SizeInfo, func: F) -> T
    where
        F: FnOnce(Self::RenderApi) -> T,
    {
        unsafe {
            gl::UseProgram(self.program.id());
            self.program.set_term_uniforms(size_info);

            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.ebo);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo_instance);
            gl::ActiveTexture(gl::TEXTURE0);
        }

        let res = func(RenderApi {
            active_tex: &mut self.active_tex,
            batch: &mut self.batch,
            atlas: &mut self.atlas,
            current_atlas: &mut self.current_atlas,
            program: &mut self.program,
        });

        unsafe {
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            gl::UseProgram(0);
        }

        res
    }

    fn program(&self) -> &Self::Shader {
        &self.program
    }

    fn loader_api(&mut self) -> LoaderApi<'_> {
        LoaderApi {
            active_tex: &mut self.active_tex,
            atlas: &mut self.atlas,
            current_atlas: &mut self.current_atlas,
        }
    }
}

impl Drop for Glsl3Renderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.vbo_instance);
            gl::DeleteBuffers(1, &self.ebo);
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}

#[derive(Debug)]
pub struct RenderApi<'a> {
    active_tex: &'a mut GLuint,
    batch: &'a mut Batch,
    atlas: &'a mut Vec<Atlas>,
    current_atlas: &'a mut usize,
    program: &'a mut TextShaderProgram,
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
                self.batch.instances.as_ptr() as *const _,
            );
        }

        // Bind texture if necessary.
        if *self.active_tex != self.batch.tex() {
            unsafe {
                gl::BindTexture(gl::TEXTURE_2D, self.batch.tex());
            }
            *self.active_tex = self.batch.tex();
        }

        unsafe {
            self.program.set_rendering_pass(RenderingPass::Background);
            gl::DrawElementsInstanced(
                gl::TRIANGLES,
                6,
                gl::UNSIGNED_INT,
                ptr::null(),
                self.batch.len() as GLsizei,
            );
            self.program.set_rendering_pass(RenderingPass::SubpixelPass1);
            gl::DrawElementsInstanced(
                gl::TRIANGLES,
                6,
                gl::UNSIGNED_INT,
                ptr::null(),
                self.batch.len() as GLsizei,
            );
        }

        self.batch.clear();
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

impl Drop for RenderApi<'_> {
    fn drop(&mut self) {
        if !self.batch.is_empty() {
            self.render_batch();
        }
    }
}

#[derive(Debug)]
#[repr(C)]
struct InstanceData {
    // Coords.
    col: u16,
    row: u16,

    // Glyph offset.
    left: i16,
    top: i16,

    // Glyph size.
    width: i16,
    height: i16,

    // UV offset.
    uv_left: f32,
    uv_bot: f32,

    // uv scale.
    uv_width: f32,
    uv_height: f32,

    // Color.
    r: u8,
    g: u8,
    b: u8,

    // Cell flags like multicolor or fullwidth character.
    cell_flags: RenderingGlyphFlags,

    // Background color.
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
    bg_a: u8,
}

#[derive(Debug, Default)]
pub struct Batch {
    tex: GLuint,
    instances: Vec<InstanceData>,
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

    fn add_item(&mut self, cell: &RenderableCell, glyph: &Glyph, _: &SizeInfo) {
        if self.is_empty() {
            self.tex = glyph.tex_id;
        }

        let mut cell_flags = RenderingGlyphFlags::empty();
        cell_flags.set(RenderingGlyphFlags::COLORED, glyph.multicolor);
        cell_flags.set(RenderingGlyphFlags::WIDE_CHAR, cell.flags.contains(Flags::WIDE_CHAR));

        self.instances.push(InstanceData {
            col: cell.point.column.0 as u16,
            row: cell.point.line as u16,

            top: glyph.top,
            left: glyph.left,
            width: glyph.width,
            height: glyph.height,

            uv_bot: glyph.uv_bot,
            uv_left: glyph.uv_left,
            uv_width: glyph.uv_width,
            uv_height: glyph.uv_height,

            r: cell.fg.r,
            g: cell.fg.g,
            b: cell.fg.b,
            cell_flags,

            bg_r: cell.bg.r,
            bg_g: cell.bg.g,
            bg_b: cell.bg.b,
            bg_a: (cell.bg_alpha * 255.0) as u8,
        });
    }
}

impl Batch {
    #[inline]
    pub fn new() -> Self {
        Self { tex: 0, instances: Vec::with_capacity(BATCH_MAX) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        BATCH_MAX
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.len() * size_of::<InstanceData>()
    }

    pub fn clear(&mut self) {
        self.tex = 0;
        self.instances.clear();
    }
}

/// Text drawing program.
///
/// Uniforms are prefixed with "u", and vertex attributes are prefixed with "a".
#[derive(Debug)]
pub struct TextShaderProgram {
    /// Shader program.
    program: ShaderProgram,

    /// Projection scale and offset uniform.
    u_projection: GLint,

    /// Cell dimensions (pixels).
    u_cell_dim: GLint,

    /// Background pass flag.
    ///
    /// Rendering is split into two passes; one for backgrounds, and one for text.
    u_rendering_pass: GLint,
}

impl TextShaderProgram {
    pub fn new(shader_version: ShaderVersion) -> Result<TextShaderProgram, Error> {
        let program = ShaderProgram::new(shader_version, None, TEXT_SHADER_V, TEXT_SHADER_F)?;
        Ok(Self {
            u_projection: program.get_uniform_location(c"projection")?,
            u_cell_dim: program.get_uniform_location(c"cellDim")?,
            u_rendering_pass: program.get_uniform_location(c"renderingPass")?,
            program,
        })
    }

    fn set_term_uniforms(&self, props: &SizeInfo) {
        unsafe {
            gl::Uniform2f(self.u_cell_dim, props.cell_width(), props.cell_height());
        }
    }

    fn set_rendering_pass(&self, rendering_pass: RenderingPass) {
        let value = match rendering_pass {
            RenderingPass::Background | RenderingPass::SubpixelPass1 => rendering_pass as i32,
            _ => unreachable!("provided pass is not supported in GLSL3 renderer"),
        };

        unsafe {
            gl::Uniform1i(self.u_rendering_pass, value);
        }
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
