use bitflags::bitflags;
use crossfont::{FontKey, GlyphId, GlyphKey, RasterizedGlyph};

use alacritty_terminal::term::cell::Flags;

use crate::display::content::RenderableCell;
use crate::display::SizeInfo;
use crate::gl;
use crate::gl::types::*;

mod atlas;
mod builtin_font;
mod gles2;
mod glsl3;
pub mod glyph_cache;

use atlas::Atlas;
pub use gles2::Gles2Renderer;
pub use glsl3::Glsl3Renderer;
pub use glyph_cache::GlyphCache;
use glyph_cache::{Glyph, LoadGlyph};

use super::text_run::{TextRun, TextRunContent};

// NOTE: These flags must be in sync with their usage in the text.*.glsl shaders.
bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct RenderingGlyphFlags: u8 {
        const COLORED   = 0b0000_0001;
        const WIDE_CHAR = 0b0000_0010;
    }
}

/// Rendering passes, for both GLES2 and GLSL3 renderer.
#[repr(u8)]
enum RenderingPass {
    /// Rendering pass used to render background color in text shaders.
    Background = 0,

    /// The first pass to render text with both GLES2 and GLSL3 renderers.
    SubpixelPass1 = 1,

    /// The second pass to render text with GLES2 renderer.
    SubpixelPass2 = 2,

    /// The third pass to render text with GLES2 renderer.
    SubpixelPass3 = 3,
}

pub trait TextRenderer<'a> {
    type Shader: TextShader;
    type RenderBatch: TextRenderBatch;
    type RenderApi: TextRenderApi<Self::RenderBatch>;

    /// Get loader API for the renderer.
    fn loader_api(&mut self) -> LoaderApi<'_>;

    fn draw_text_runs<'b: 'a, I: Iterator<Item = TextRun>>(
        &'b mut self,
        size_info: &'b SizeInfo,
        glyph_cache: &'a mut GlyphCache,
        text_runs: I,
    ) {
        self.with_api(size_info, |mut api| {
            for text_run in text_runs {
                api.draw_text_run(text_run, glyph_cache, size_info);
            }
        })
    }

    fn with_api<'b: 'a, F, T>(&'b mut self, size_info: &'b SizeInfo, func: F) -> T
    where
        F: FnOnce(Self::RenderApi) -> T;

    fn program(&self) -> &Self::Shader;

    /// Resize the text rendering.
    fn resize(&self, size: &SizeInfo) {
        unsafe {
            let program = self.program();
            gl::UseProgram(program.id());
            update_projection(program.projection_uniform(), size);
            gl::UseProgram(0);
        }
    }

    /// Invoke renderer with the loader.
    fn with_loader<F: FnOnce(LoaderApi<'_>) -> T, T>(&mut self, func: F) -> T {
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0);
        }

        func(self.loader_api())
    }
}

pub trait TextRenderBatch {
    /// Check if `Batch` is empty.
    fn is_empty(&self) -> bool;

    /// Check whether the `Batch` is full.
    fn full(&self) -> bool;

    /// Get texture `Batch` is using.
    fn tex(&self) -> GLuint;

    /// Add item to the batch.
    fn add_item(&mut self, cell: &RenderableCell, glyph: &Glyph, size_info: &SizeInfo);
}

pub trait TextRenderApi<T: TextRenderBatch>: LoadGlyph {
    /// Get `Batch` the api is using.
    fn batch(&mut self) -> &mut T;

    /// Render the underlying data.
    fn render_batch(&mut self);

    /// Add item to the rendering queue.
    #[inline]
    fn add_render_item(&mut self, cell: &RenderableCell, glyph: &Glyph, size_info: &SizeInfo) {
        // Flush batch if tex changing.
        if !self.batch().is_empty() && self.batch().tex() != glyph.tex_id {
            self.render_batch();
        }

        self.batch().add_item(cell, glyph, size_info);

        // Render batch and clear if it's full.
        if self.batch().full() {
            self.render_batch();
        }
    }

    fn render_zero_widths<'r, I>(
        &mut self,
        zero_width_chars: I,
        cell: &RenderableCell,
        font_key: FontKey,
        glyph_cache: &mut GlyphCache,
        size_info: &SizeInfo,
    ) where
        I: Iterator<Item = &'r char>,
    {
        for c in zero_width_chars {
            let glyph_key =
                GlyphKey { font_key, size: glyph_cache.font_size, id: GlyphId::char(*c) };
            let glyph = glyph_cache.get(glyph_key, self, false);

            self.add_render_item(cell, &glyph, size_info);
        }
    }

    fn draw_text_run(
        &mut self,
        text_run: TextRun,
        glyph_cache: &mut GlyphCache,
        size_info: &SizeInfo,
    ) where
        Self: Sized,
    {
        let TextRunContent { text, zero_widths } = &text_run.content;

        // Get font key for cell
        let font_key = match text_run.flags & Flags::BOLD_ITALIC {
            Flags::BOLD_ITALIC => glyph_cache.bold_italic_key,
            Flags::BOLD => glyph_cache.bold_key,
            Flags::ITALIC => glyph_cache.italic_key,
            _ => glyph_cache.font_key,
        };

        let shaped_glyphs = if text_run.flags.contains(Flags::HIDDEN) {
            GlyphIter::Hidden
        } else {
            GlyphIter::Shaped(
                glyph_cache.shape_run(text, font_key, self).expect("read font").into_iter(),
            )
        };

        for ((mut cell, glyph), zero_width_chars) in
            text_run.cells().zip(shaped_glyphs).zip(zero_widths.iter())
        {
            // Ignore hidden cells and render tabs as spaces to prevent font issues.
            let hidden = cell.flags.contains(Flags::HIDDEN);
            if cell.character == '\t' || hidden {
                cell.character = ' ';
            }

            // Add cell to batch.
            self.add_render_item(&cell, &glyph, size_info);

            // Add empty spacer for full width characters
            if text_run.flags.contains(Flags::WIDE_CHAR) {
                cell.point.column += 1;
                self.add_render_item(&cell, &Glyph::default(), size_info);
                cell.point.column -= 1;
            }

            // Render visible zero-width characters.
            self.render_zero_widths(
                zero_width_chars.as_deref().unwrap_or(&[]).iter().filter(|c| **c != ' '),
                &cell,
                font_key,
                glyph_cache,
                size_info,
            );
        }
    }
}

/// Abstracts iteration over a run of hidden glyphs or shaped glyphs.
enum GlyphIter<I> {
    /// Our run was not hidden and our glyphs were shaped
    Shaped(I),
    /// Our run is hidden and was not shaped
    Hidden,
}

impl<I> Iterator for GlyphIter<I>
where
    I: Iterator<Item = Glyph>,
{
    type Item = Glyph;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            GlyphIter::Shaped(inner) => inner.next(),
            GlyphIter::Hidden => Some(Glyph::default()),
        }
    }
}

pub trait TextShader {
    fn id(&self) -> GLuint;

    /// Id of the projection uniform.
    fn projection_uniform(&self) -> GLint;
}

#[derive(Debug)]
pub struct LoaderApi<'a> {
    active_tex: &'a mut GLuint,
    atlas: &'a mut Vec<Atlas>,
    current_atlas: &'a mut usize,
}

impl<'a> LoadGlyph for LoaderApi<'a> {
    fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph {
        Atlas::load_glyph(self.active_tex, self.atlas, self.current_atlas, rasterized)
    }

    fn clear(&mut self) {
        Atlas::clear_atlas(self.atlas, self.current_atlas)
    }
}

fn update_projection(u_projection: GLint, size: &SizeInfo) {
    let width = size.width();
    let height = size.height();
    let padding_x = size.padding_x();
    let padding_y = size.padding_y();

    // Bounds check.
    if (width as u32) < (2 * padding_x as u32) || (height as u32) < (2 * padding_y as u32) {
        return;
    }

    // Compute scale and offset factors, from pixel to ndc space. Y is inverted.
    //   [0, width - 2 * padding_x] to [-1, 1]
    //   [height - 2 * padding_y, 0] to [-1, 1]
    let scale_x = 2. / (width - 2. * padding_x);
    let scale_y = -2. / (height - 2. * padding_y);
    let offset_x = -1.;
    let offset_y = 1.;

    unsafe {
        gl::Uniform4f(u_projection, offset_x, offset_y, scale_x, scale_y);
    }
}
