//! This module implements the logic to manage graphic items included in a
//! `Grid` instance.

pub mod sixel;

use std::mem;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::grid::Dimensions;
use crate::term::color::Rgb;

/// Max allowed dimensions (width, height) for the graphic, in pixels.
pub const MAX_GRAPHIC_DIMENSIONS: [usize; 2] = [4096, 4096];

/// Unique identifier for every graphic added to a grid.
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, Copy, Hash, PartialOrd, Ord)]
pub struct GraphicId(u64);

/// Reference to a texture stored in the display.
///
/// When all references to a single texture are removed, its identifier is
/// added to the remove queue.
#[derive(Clone, Debug)]
pub struct TextureRef {
    /// Graphic identifier.
    pub id: GraphicId,

    /// Queue to track removed references.
    pub texture_operations: Weak<Mutex<Vec<TextureOperation>>>,
}

impl PartialEq for TextureRef {
    fn eq(&self, t: &Self) -> bool {
        // Ignore texture_operations.
        self.id == t.id
    }
}

impl Eq for TextureRef {}

impl Drop for TextureRef {
    fn drop(&mut self) {
        if let Some(texture_operations) = self.texture_operations.upgrade() {
            texture_operations.lock().push(TextureOperation::Remove(self.id));
        }
    }
}

/// Graphic data stored in a single cell.
#[derive(Clone, Debug)]
pub struct GraphicCell {
    /// Texture to draw the graphic in this cell.
    pub texture: Arc<TextureRef>,

    /// Offset in the x direction.
    pub offset_x: u16,

    /// Offset in the y direction.
    pub offset_y: u16,

    /// Queue to track empty subregions.
    pub texture_operations: Weak<Mutex<Vec<TextureOperation>>>,
}

impl PartialEq for GraphicCell {
    fn eq(&self, c: &Self) -> bool {
        // Ignore texture_operations.
        self.texture == c.texture && self.offset_x == c.offset_x && self.offset_y == c.offset_y
    }
}

impl Eq for GraphicCell {}

impl Drop for GraphicCell {
    fn drop(&mut self) {
        if let Some(texture_operations) = self.texture_operations.upgrade() {
            let tex_op = TextureOperation::ClearSubregion(ClearSubregion {
                id: self.texture.id,
                x: self.offset_x,
                y: self.offset_y,
            });

            texture_operations.lock().push(tex_op);
        }
    }
}

impl GraphicCell {
    /// Graphic identifier of the texture in this cell.
    #[inline]
    pub fn graphic_id(&self) -> GraphicId {
        self.texture.id
    }
}

/// Specifies the format of the pixel data.
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, Copy)]
pub enum ColorType {
    /// 3 bytes per pixel (red, green, blue).
    Rgb,

    /// 4 bytes per pixel (red, green, blue, alpha).
    Rgba,
}

/// Defines a single graphic read from the PTY.
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug)]
pub struct GraphicData {
    /// Graphics identifier.
    pub id: GraphicId,

    /// Width, in pixels, of the graphic.
    pub width: usize,

    /// Height, in pixels, of the graphic.
    pub height: usize,

    /// Color type of the pixels.
    pub color_type: ColorType,

    /// Pixels data.
    pub pixels: Vec<u8>,
}

/// Operation to clear a subregion in an existing graphic.
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct ClearSubregion {
    /// Graphics identifier.
    pub id: GraphicId,

    /// X coordinate.
    pub x: u16,

    /// Y coordinate.
    pub y: u16,
}

/// Queues to add or to remove the textures in the display.
#[derive(Default)]
pub struct UpdateQueues {
    /// Graphics read from the PTY.
    pub pending: Vec<GraphicData>,

    /// Graphics removed from the grid.
    pub remove_queue: Vec<GraphicId>,

    /// Subregions in a graphic to be clear.
    pub clear_subregions: Vec<ClearSubregion>,
}

/// Operations on the existing textures.
#[derive(Debug)]
pub enum TextureOperation {
    /// Remove a texture from the GPU.
    Remove(GraphicId),

    /// Clear a subregion.
    ClearSubregion(ClearSubregion),
}

/// Track changes in the grid to add or to remove graphics.
#[derive(Clone, Debug, Default)]
pub struct Graphics {
    /// Last generated identifier.
    pub last_id: u64,

    /// New graphics, received from the PTY.
    pub pending: Vec<GraphicData>,

    /// Graphics removed from the grid.
    pub texture_operations: Arc<Mutex<Vec<TextureOperation>>>,

    /// Shared palette for Sixel graphics.
    pub sixel_shared_palette: Option<Vec<Rgb>>,

    /// Cell height in pixels.
    pub cell_height: f32,

    /// Cell width in pixels.
    pub cell_width: f32,
}

impl Graphics {
    /// Generate a new graphic identifier.
    pub fn next_id(&mut self) -> GraphicId {
        self.last_id += 1;
        GraphicId(self.last_id)
    }

    /// Get queues to update graphics in the grid.
    ///
    /// If all queues are empty, it returns `None`.
    pub fn take_queues(&mut self) -> Option<UpdateQueues> {
        let texture_operations = {
            let mut queue = self.texture_operations.lock();
            if queue.is_empty() {
                Vec::new()
            } else {
                mem::take(&mut *queue)
            }
        };

        if texture_operations.is_empty() && self.pending.is_empty() {
            return None;
        }

        let mut remove_queue = Vec::new();
        let mut clear_subregions = Vec::new();

        for operation in texture_operations {
            match operation {
                TextureOperation::Remove(id) => remove_queue.push(id),
                TextureOperation::ClearSubregion(cs) => clear_subregions.push(cs),
            }
        }

        Some(UpdateQueues { pending: mem::take(&mut self.pending), remove_queue, clear_subregions })
    }

    /// Update cell dimensions.
    pub fn resize<S: Dimensions>(&mut self, size: S) {
        self.cell_height = size.cell_height();
        self.cell_width = size.cell_width();
    }
}
