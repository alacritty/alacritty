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
}

#[inline]
fn to_freetype_26_6(f: f32) -> isize {
    ((1i32 << 6) as f32 * f) as isize
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct FontDesc {
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
    pub fn new() -> Rasterizer {
        let library = Library::init().unwrap();

        Rasterizer {
            system_fonts: get_font_families(),
            faces: HashMap::new(),
            library: library,
        }
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

    pub fn get_glyph(&mut self, size: f32, c: char) -> RasterizedGlyph {
        let face = self.get_face(&FontDesc::new("Ubuntu Mono", "Regular"))
                       .expect("TODO handle get_face error");
        // TODO DPI
        face.set_char_size(to_freetype_26_6(size), 0, 96, 0).unwrap();
        face.load_char(c as usize, freetype::face::RENDER).unwrap();
        let glyph = face.glyph();

        RasterizedGlyph {
            top: glyph.bitmap_top() as usize,
            left: glyph.bitmap_left() as usize,
            width: glyph.bitmap().width() as usize,
            height: glyph.bitmap().rows() as usize,
            buf: glyph.bitmap().buffer().to_vec(),
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
    use super::Rasterizer;

    #[test]
    fn create_rasterizer_and_render_glyph() {
        let mut rasterizer = Rasterizer::new();
        let glyph = rasterizer.get_glyph(24., 'U');

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
