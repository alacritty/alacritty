use std::collections::HashMap;
use list_fonts::get_font_families;

use freetype::Library;
use freetype::Face;
use freetype;

/// Rasterizes glyphs for a single font face.
pub struct Rasterizer {
    faces: HashMap<FontDesc, Face<'static>>,
    library: Library,
    system_fonts: HashMap<String, ::list_fonts::Family>,
    dpi_x: u32,
    dpi_y: u32,
    dpr: f32,
}

#[inline]
fn to_freetype_26_6(f: f32) -> isize {
    ((1i32 << 6) as f32 * f) as isize
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FontDesc {
    name: String,
    style: String,
}

impl FontDesc {
    pub fn new<S>(name: S, style: S) -> FontDesc
        where S: Into<String>
    {
        FontDesc {
            name: name.into(),
            style: style.into()
        }
    }
}

impl Rasterizer {
    pub fn new(dpi_x: f32, dpi_y: f32, device_pixel_ratio: f32) -> Rasterizer {
        let library = Library::init().unwrap();

        Rasterizer {
            system_fonts: get_font_families(),
            faces: HashMap::new(),
            library: library,
            dpi_x: dpi_x as u32,
            dpi_y: dpi_y as u32,
            dpr: device_pixel_ratio,
        }
    }

    pub fn box_size_for_font(&mut self, desc: &FontDesc, size: f32) -> (u32, u32) {
        let face = self.get_face(&desc).unwrap();

        let scale_size = self.dpr * size;

        let em_size = face.em_size() as f32;
        let w = face.max_advance_width() as f32;
        let h = face.height() as f32;

        let w_scale = w / em_size;
        let h_scale = h / em_size;

        ((w_scale * scale_size) as u32, (h_scale * scale_size) as u32)
    }

    pub fn get_face(&mut self, desc: &FontDesc) -> Option<Face<'static>> {
        if let Some(face) = self.faces.get(desc) {
            return Some(face.clone());
        }

        if let Some(font) = self.system_fonts.get(&desc.name[..]) {
            if let Some(variant) = font.variants().get(&desc.style[..]) {
                let face = self.library.new_face(variant.path(), variant.index())
                                       .expect("TODO handle new_face error");

                self.faces.insert(desc.to_owned(), face);
                return Some(self.faces.get(desc).unwrap().clone());
            }
        }

        None
    }

    pub fn get_glyph(&mut self, desc: &FontDesc, size: f32, c: char) -> RasterizedGlyph {
        let face = self.get_face(desc).expect("TODO handle get_face error");
        face.set_char_size(to_freetype_26_6(size * self.dpr), 0, self.dpi_x, self.dpi_y).unwrap();
        face.load_char(c as usize, freetype::face::TARGET_LIGHT).unwrap();
        let glyph = face.glyph();
        glyph.render_glyph(freetype::render_mode::RenderMode::Lcd).unwrap();

        // FIXME need LCD filtering to reduce color fringes with subpixel rendering. The freetype
        // bindings don't currently expose this!

        let bitmap = glyph.bitmap();
        let buf = bitmap.buffer();
        let pitch = bitmap.pitch() as usize;

        let mut packed = Vec::with_capacity((bitmap.rows() * bitmap.width()) as usize);
        for i in 0..bitmap.rows() {
            let start = (i as usize) * pitch;
            let stop = start + bitmap.width() as usize;
            packed.extend_from_slice(&buf[start..stop]);
        }

        RasterizedGlyph {
            top: glyph.bitmap_top() as usize,
            left: glyph.bitmap_left() as usize,
            width: glyph.bitmap().width() as usize / 3,
            height: glyph.bitmap().rows() as usize,
            buf: packed,
        }
    }
}

#[derive(Debug)]
pub struct RasterizedGlyph {
    pub width: usize,
    pub height: usize,
    pub top: usize,
    pub left: usize,
    pub buf: Vec<u8>,
}


#[cfg(test)]
mod tests {
    use super::{Rasterizer, FontDesc};

    #[cfg(target_os = "linux")]
    fn font_desc() -> FontDesc {
        FontDesc::new("Ubuntu Mono", "Regular")
    }

    #[test]
    fn create_rasterizer_and_render_glyph() {
        let mut rasterizer = Rasterizer::new();
        let glyph = rasterizer.get_glyph(&font_desc(), 24., 'U');

        println!("glyph: {:?}", glyph);

        for j in 0..glyph.height {
            for i in 0..glyph.width {
                let val = glyph.buf[j * glyph.width + i];
                print!("{}", if val < 122 { " " } else { "%"});
            }

            print!("\n");
        }
    }
}
