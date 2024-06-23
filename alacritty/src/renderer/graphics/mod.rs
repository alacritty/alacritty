//! This module implements the functionality to support graphics in the grid.

use std::mem;

use crate::display::SizeInfo;
use crate::renderer::{RenderRect, ShaderVersion};
use alacritty_terminal::graphics::{
    ClearSubregion, ColorType, GraphicData, GraphicId, UpdateQueues,
};

use crossfont::Metrics;
use log::trace;
use serde::{Deserialize, Serialize};

use crate::gl::types::*;
use crate::{gl, renderer};

use std::cmp;
use std::collections::{HashMap, HashSet};
use std::ffi::CStr;

mod draw;
mod shader;

/// Max. number of textures stored in the GPU.
const MAX_TEXTURES_COUNT: usize = 1000;

pub use draw::RenderList;

bitflags::bitflags! {
    /// Result of the `run_updates` operation.
    pub struct UpdateResult: u8 {
        const SUCCESS = 1 << 0;
        const NEED_RESET_ACTIVE_TEX = 1 << 1;
    }
}

/// Type for texture names generated in the GPU.
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug)]
pub struct TextureName(GLuint);

impl Drop for TextureName {
    fn drop(&mut self) {
        if self.0 != 0 {
            trace!("Delete texture {}.", self.0);
            unsafe {
                gl::DeleteTextures(1, &self.0);
            }
        }
    }
}

/// Texture for a graphic in the grid.
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct GraphicTexture {
    /// Texture in the GPU where the graphic pixels are stored.
    texture: TextureName,

    /// Cell height at the moment graphic was created.
    ///
    /// Used to scale it if the user increases or decreases the font size.
    cell_height: f32,

    /// Cell width at the moment graphic was created.
    cell_width: f32,

    /// Width in pixels of the graphic.
    width: u16,

    /// Height in pixels of the graphic.
    height: u16,
}

#[derive(Debug)]
pub struct GraphicsRenderer {
    /// Program in the GPU to render graphics.
    program: shader::GraphicsShaderProgram,

    /// Collection to associate graphic identifiers with their textures.
    graphic_textures: HashMap<GraphicId, GraphicTexture>,

    /// Indicate if the OpenGL driver has the `clear_texture` extension.
    clear_texture_ext: bool,
}

impl GraphicsRenderer {
    pub fn new(shader_version: ShaderVersion) -> Result<GraphicsRenderer, renderer::Error> {
        let program = shader::GraphicsShaderProgram::new(shader_version)?;
        let clear_texture_ext = check_opengl_extensions(&["GL_ARB_clear_texture"]);
        Ok(GraphicsRenderer { program, graphic_textures: HashMap::default(), clear_texture_ext })
    }

    /// Run the required actions to apply changes for the graphics in the grid.
    #[inline]
    pub fn run_updates(
        &mut self,
        update_queues: UpdateQueues,
        size_info: &SizeInfo,
    ) -> UpdateResult {
        self.remove_graphics(update_queues.remove_queue)
            | self.upload_pending_graphics(update_queues.pending, size_info)
            | self.clear_subregions(update_queues.clear_subregions)
    }

    /// Release resources used by removed graphics.
    fn remove_graphics(&mut self, removed_ids: Vec<GraphicId>) -> UpdateResult {
        let mut textures = Vec::with_capacity(removed_ids.len());
        for id in removed_ids {
            if let Some(mut graphic_texture) = self.graphic_textures.remove(&id) {
                // Reset the inner value of TextureName, so the Drop implementation
                // (in debug mode) can verify that the texture was deleted.
                textures.push(mem::take(&mut graphic_texture.texture.0));
            }
        }

        trace!("Call glDeleteTextures with {} items", textures.len());

        unsafe {
            gl::DeleteTextures(textures.len() as GLint, textures.as_ptr());
        }

        UpdateResult::SUCCESS
    }

    /// Create new textures in the GPU, and upload the pixels to them.
    fn upload_pending_graphics(
        &mut self,
        graphics: Vec<GraphicData>,
        size_info: &SizeInfo,
    ) -> UpdateResult {
        if graphics.is_empty() {
            return UpdateResult::SUCCESS;
        }

        for graphic in graphics {
            let mut texture = 0;

            unsafe {
                gl::GenTextures(1, &mut texture);
                trace!("Texture generated: {}", texture);

                gl::BindTexture(gl::TEXTURE_2D, texture);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAX_LEVEL, 0);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as GLint);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as GLint);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);

                let pixel_format = match graphic.color_type {
                    ColorType::Rgb => gl::RGB,
                    ColorType::Rgba => gl::RGBA,
                };

                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGBA as GLint,
                    graphic.width as GLint,
                    graphic.height as GLint,
                    0,
                    pixel_format,
                    gl::UNSIGNED_BYTE,
                    graphic.pixels.as_ptr().cast(),
                );

                gl::BindTexture(gl::TEXTURE_2D, 0);
            }

            let graphic_texture = GraphicTexture {
                texture: TextureName(texture),
                cell_height: size_info.cell_height(),
                cell_width: size_info.cell_width(),
                width: graphic.width as u16,
                height: graphic.height as u16,
            };

            self.graphic_textures.insert(graphic.id, graphic_texture);
        }

        // If we exceed the textures limit, discard the oldest ones.
        while self.graphic_textures.len() > MAX_TEXTURES_COUNT {
            match self.graphic_textures.keys().min().copied() {
                Some(id) => self.graphic_textures.remove(&id),
                None => unreachable!(),
            };
        }

        UpdateResult::NEED_RESET_ACTIVE_TEX
    }

    /// Update textures in the GPU to clear specific subregions.
    pub fn clear_subregions(&mut self, clear_subregions: Vec<ClearSubregion>) -> UpdateResult {
        // If the GL_ARB_clear_texture extension is available we can use
        // glClearTexSubImage to clear the region without sending any memory.
        //
        // If the extension is not available, we have to initialize empty memory
        // and upload it with glTexSubImage2D.

        let mut result = UpdateResult::SUCCESS;

        for clear_subregion in clear_subregions {
            let entry = match self.graphic_textures.get(&clear_subregion.id) {
                Some(entry) => entry,
                None => continue,
            };

            let x_offset = clear_subregion.x as GLint;
            let y_offset = clear_subregion.y as GLint;

            let max_width = entry.width as GLint - x_offset;
            let max_height = entry.height as GLint - y_offset;

            let width = cmp::min(entry.cell_width as GLint, max_width);
            let height = cmp::min(entry.cell_height as GLint, max_height);

            if self.clear_texture_ext {
                let empty = [0_u8; 4];

                unsafe {
                    gl::ClearTexSubImage(
                        entry.texture.0,
                        0,
                        x_offset,
                        y_offset,
                        0,
                        width,
                        height,
                        1,
                        gl::RGBA,
                        gl::UNSIGNED_BYTE,
                        empty.as_ptr().cast(),
                    );
                }
            } else {
                let buf_size = width * height * 4;
                let empty = vec![0_u8; buf_size as usize];

                unsafe {
                    gl::BindTexture(gl::TEXTURE_2D, entry.texture.0);
                    gl::TexSubImage2D(
                        gl::TEXTURE_2D,
                        0,
                        x_offset,
                        y_offset,
                        width,
                        height,
                        gl::RGBA,
                        gl::UNSIGNED_BYTE,
                        empty.as_ptr().cast(),
                    )
                }

                result = UpdateResult::NEED_RESET_ACTIVE_TEX;
            }
        }

        result
    }

    /// Draw graphics in the display.
    #[inline]
    pub fn draw(
        &mut self,
        render_list: RenderList,
        size_info: &SizeInfo,
        rects: &mut Vec<RenderRect>,
        metrics: &Metrics,
    ) {
        if !render_list.is_empty() {
            render_list.draw(self, size_info, rects, metrics);
        }
    }
}

fn check_opengl_extensions(extensions: &[&str]) -> bool {
    // Use a HashSet to track extensions needed to be found. When the set is
    // empty, we know that all extensions are available.
    let mut needed: HashSet<_> = extensions.iter().collect();

    let mut num_exts = 0;
    unsafe {
        gl::GetIntegerv(gl::NUM_EXTENSIONS, &mut num_exts);
    }

    for index in 0..num_exts as GLuint {
        let pointer = unsafe { gl::GetStringi(gl::EXTENSIONS, index) };
        if pointer.is_null() {
            log::warn!("Can't get OpenGL extension name at index {} of {}", index, num_exts);
            return false;
        }

        let extension = unsafe { CStr::from_ptr(pointer.cast()) };
        if let Ok(ext) = extension.to_str() {
            if needed.remove(&ext) && needed.is_empty() {
                return true;
            }
        }
    }

    log::debug!("Missing OpenGL extensions: {:?}", needed);
    false
}
