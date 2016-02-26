use list_fonts::get_font_families;

use freetype::Library;
use freetype::Face;
use freetype;

/// Rasterizes glyphs for a single font face.
pub struct Rasterizer {
    face: Face<'static>,
    _library: Library,
}

#[inline]
fn to_freetype_26_6(f: f32) -> isize {
    ((1i32 << 6) as f32 * f) as isize
}

impl Rasterizer {
    pub fn new() -> Rasterizer {
        let library = Library::init().unwrap();

        let family = get_font_families().into_iter()
                                        .filter(|f| f.name() == "Inconsolata-dz")
                                        .nth(0).unwrap(); // TODO

        let variant = family.variants().first().unwrap();
        let path = variant.filepath();

        Rasterizer {
            face: library.new_face(path, 0).expect("Create font face"),
            _library: library,
        }
    }

    pub fn get_glyph(&self, size: f32, c: char) -> RasterizedGlyph {
        // TODO DPI
        self.face.set_char_size(to_freetype_26_6(size), 0, 96, 0).unwrap();
        self.face.load_char(c as usize, freetype::face::RENDER).unwrap();
        let glyph = self.face.glyph();

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
        let rasterizer = Rasterizer::new();
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
