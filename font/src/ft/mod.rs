// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//! Rasterization powered by FreeType and FontConfig
use std::collections::HashMap;

use freetype::Library;
use freetype::Face;
use freetype;

mod list_fonts;

use self::list_fonts::{Family, get_font_families};
use super::{FontDesc, RasterizedGlyph, Metrics, Size, FontKey, GlyphKey};

/// Rasterizes glyphs for a single font face.
pub struct Rasterizer {
    faces: HashMap<FontKey, Face<'static>>,
    library: Library,
    system_fonts: HashMap<String, Family>,
    dpi_x: u32,
    dpi_y: u32,
    dpr: f32,
}

#[inline]
fn to_freetype_26_6(f: f32) -> isize {
    ((1i32 << 6) as f32 * f) as isize
}

// #[inline]
// fn freetype_26_6_to_float(val: i64) -> f64 {
//     val as f64 / (1i64 << 6) as f64
// }

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

    pub fn metrics(&self, key: FontKey, size: Size) -> Metrics {
        let face = self.faces.get(&key).unwrap();

        let scale_size = self.dpr as f64 * size.as_f32_pts() as f64;

        let em_size = face.em_size() as f64;
        let w = face.max_advance_width() as f64;
        let h = (face.ascender() - face.descender() + face.height()) as f64;

        let w_scale = w * scale_size / em_size;
        let h_scale = h * scale_size / em_size;

        Metrics {
            average_advance: w_scale,
            line_height: h_scale,
        }
    }

    pub fn load_font(&mut self, desc: &FontDesc, _size: Size) -> Option<FontKey> {
        self.get_face(desc)
            .map(|face| {
                let key = FontKey::next();
                self.faces.insert(key, face);
                key
            })
    }

    fn get_face(&mut self, desc: &FontDesc) -> Option<Face<'static>> {
        self.system_fonts
            .get(&desc.name[..])
            .and_then(|font| font.variants().get(&desc.style[..]))
            .map(|variant| {
                self.library.new_face(variant.path(), variant.index())
                            .expect("TODO handle new_face error")
            })
    }

    pub fn get_glyph(&mut self, glyph_key: &GlyphKey) -> RasterizedGlyph {
        let face = self.faces
            .get(&glyph_key.font_key)
            .expect("TODO handle get_face error");

        let size = glyph_key.size.as_f32_pts() * self.dpr;
        let c = glyph_key.c;

        face.set_char_size(to_freetype_26_6(size), 0, self.dpi_x, self.dpi_y).unwrap();
        face.load_char(c as usize, freetype::face::TARGET_LIGHT).unwrap();
        let glyph = face.glyph();
        glyph.render_glyph(freetype::render_mode::RenderMode::Lcd).unwrap();

        unsafe {
            let ft_lib = self.library.raw();
            freetype::ffi::FT_Library_SetLcdFilter(ft_lib, freetype::ffi::FT_LCD_FILTER_DEFAULT);
        }

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
            c: c,
            top: glyph.bitmap_top(),
            left: glyph.bitmap_left(),
            width: glyph.bitmap().width() / 3,
            height: glyph.bitmap().rows(),
            buf: packed,
        }
    }
}

unsafe impl Send for Rasterizer {}

#[cfg(test)]
mod tests {
    use ::FontDesc;

    fn font_desc() -> FontDesc {
        FontDesc::new("Ubuntu Mono", "Regular")
    }
}
