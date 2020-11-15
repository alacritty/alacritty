//! Helpers for creating different cursor glyphs from font metrics.

use crossfont::{BitmapBuffer, Metrics, RasterizedGlyph};

use alacritty_terminal::ansi::CursorShape;

pub fn get_cursor_glyph(
    cursor: CursorShape,
    metrics: Metrics,
    offset_x: i8,
    offset_y: i8,
    is_wide: bool,
    cursor_thickness: f64,
) -> RasterizedGlyph {
    // Calculate the cell metrics.
    //
    // NOTE: With Rust 1.47+ `f64 as usize` is defined to clamp automatically:
    // https://github.com/rust-lang/rust/commit/14d608f1d8a0b84da5f3bccecb3efb3d35f980dc
    let height = (metrics.line_height + f64::from(offset_y)).max(1.) as usize;
    let mut width = (metrics.average_advance + f64::from(offset_x)).max(1.) as usize;
    let line_width = (cursor_thickness * width as f64).round().max(1.) as usize;

    // Double the cursor width if it's above a double-width glyph.
    if is_wide {
        width *= 2;
    }

    match cursor {
        CursorShape::HollowBlock => get_box_cursor_glyph(height, width, line_width),
        CursorShape::Underline => get_underline_cursor_glyph(width, line_width),
        CursorShape::Beam => get_beam_cursor_glyph(height, line_width),
        CursorShape::Block => get_block_cursor_glyph(height, width),
        CursorShape::Hidden => RasterizedGlyph::default(),
    }
}

/// Return a custom underline cursor character.
pub fn get_underline_cursor_glyph(width: usize, line_width: usize) -> RasterizedGlyph {
    // Create a new rectangle, the height is relative to the font width.
    let buf = vec![255u8; width * line_width * 3];

    // Create a custom glyph with the rectangle data attached to it.
    RasterizedGlyph {
        c: ' ',
        top: line_width as i32,
        left: 0,
        height: line_width as i32,
        width: width as i32,
        buf: BitmapBuffer::RGB(buf),
    }
}

/// Return a custom beam cursor character.
pub fn get_beam_cursor_glyph(height: usize, line_width: usize) -> RasterizedGlyph {
    // Create a new rectangle that is at least one pixel wide
    let buf = vec![255u8; line_width * height * 3];

    // Create a custom glyph with the rectangle data attached to it
    RasterizedGlyph {
        c: ' ',
        top: height as i32,
        left: 0,
        height: height as i32,
        width: line_width as i32,
        buf: BitmapBuffer::RGB(buf),
    }
}

/// Returns a custom box cursor character.
pub fn get_box_cursor_glyph(height: usize, width: usize, line_width: usize) -> RasterizedGlyph {
    // Create a new box outline rectangle.
    let mut buf = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            if y < line_width
                || y >= height - line_width
                || x < line_width
                || x >= width - line_width
            {
                buf.append(&mut vec![255u8; 3]);
            } else {
                buf.append(&mut vec![0u8; 3]);
            }
        }
    }

    // Create a custom glyph with the rectangle data attached to it.
    RasterizedGlyph {
        c: ' ',
        top: height as i32,
        left: 0,
        height: height as i32,
        width: width as i32,
        buf: BitmapBuffer::RGB(buf),
    }
}

/// Return a custom block cursor character.
pub fn get_block_cursor_glyph(height: usize, width: usize) -> RasterizedGlyph {
    // Create a completely filled glyph.
    let buf = vec![255u8; width * height * 3];

    // Create a custom glyph with the rectangle data attached to it.
    RasterizedGlyph {
        c: ' ',
        top: height as i32,
        left: 0,
        height: height as i32,
        width: width as i32,
        buf: BitmapBuffer::RGB(buf),
    }
}
