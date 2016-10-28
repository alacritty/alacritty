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
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::mem::size_of;
use std::path::{PathBuf};
use std::ptr;
use std::sync::mpsc;

use cgmath;
use font::{self, Rasterizer, RasterizedGlyph, FontDesc, GlyphKey, FontKey};
use gl::types::*;
use gl;
use notify::{Watcher as WatcherApi, RecommendedWatcher as Watcher, op};

use config::Config;
use grid::Grid;
use term::{self, cell, Cell};

use super::Rgb;

// Shader paths for live reload
static TEXT_SHADER_F_PATH: &'static str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/text.f.glsl");
static TEXT_SHADER_V_PATH: &'static str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/text.v.glsl");

// Shader source which is used when live-shader-reload feature is disable
static TEXT_SHADER_F: &'static str = include_str!(
    concat!(env!("CARGO_MANIFEST_DIR"), "/res/text.f.glsl")
);
static TEXT_SHADER_V: &'static str = include_str!(
    concat!(env!("CARGO_MANIFEST_DIR"), "/res/text.v.glsl")
);

/// LoadGlyph allows for copying a rasterized glyph into graphics memory
pub trait LoadGlyph {
    /// Load the rasterized glyph into GPU memory
    fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph;
}

enum Msg {
    ShaderReload,
}

/// Text drawing program
///
/// Uniforms are prefixed with "u", and vertex attributes are prefixed with "a".
#[derive(Debug)]
pub struct ShaderProgram {
    // Program id
    id: GLuint,

    /// projection matrix uniform
    u_projection: GLint,

    /// Terminal dimensions (pixels)
    u_term_dim: GLint,

    /// Cell dimensions (pixels)
    u_cell_dim: GLint,

    /// Background pass flag
    ///
    /// Rendering is split into two passes; 1 for backgrounds, and one for text
    u_background: GLint,
}


#[derive(Debug, Clone)]
pub struct Glyph {
    tex_id: GLuint,
    top: f32,
    left: f32,
    width: f32,
    height: f32,
    uv_bot: f32,
    uv_left: f32,
    uv_width: f32,
    uv_height: f32,
}

/// Naïve glyph cache
///
/// Currently only keyed by `char`, and thus not possible to hold different
/// representations of the same code point.
pub struct GlyphCache {
    /// Cache of buffered glyphs
    cache: HashMap<GlyphKey, Glyph>,

    /// Rasterizer for loading new glyphs
    rasterizer: Rasterizer,

    /// regular font
    font_key: FontKey,

    /// italic font
    italic_key: FontKey,

    /// bold font
    bold_key: FontKey,

    /// font size
    font_size: font::Size,
}

impl GlyphCache {
    pub fn new<L>(mut rasterizer: Rasterizer, config: &Config, loader: &mut L) -> GlyphCache
        where L: LoadGlyph
    {
        let font = config.font();
        let size = font.size();

        // Load regular font
        let regular_desc = FontDesc::new(font.family(), font.style());
        let regular = rasterizer.load_font(&regular_desc, size).expect("regular font load ok");

        // Load bold font
        let bold_style = font.bold_style().unwrap_or("Bold");
        let bold_desc = FontDesc::new(font.family(), bold_style);

        let bold = if bold_desc == regular_desc {
            regular.clone()
        } else {
            rasterizer.load_font(&bold_desc, size).unwrap_or_else(|| regular.clone())
        };

        // Load italic font
        let italic_style = font.italic_style().unwrap_or("Italic");
        let italic_desc = FontDesc::new(font.family(), italic_style);

        let italic = if italic_desc == regular_desc {
            regular.clone()
        } else {
            rasterizer.load_font(&italic_desc, size)
                      .unwrap_or_else(|| regular.clone())
        };

        let mut cache = GlyphCache {
            cache: HashMap::new(),
            rasterizer: rasterizer,
            font_size: font.size(),
            font_key: regular.clone(),
            bold_key: bold.clone(),
            italic_key: italic.clone(),
        };

        macro_rules! load_glyphs_for_font {
            ($font:expr) => {
                for i in 32u8...128u8 {
                    cache.load_and_cache_glyph(GlyphKey {
                        font_key: $font,
                        c: i as char,
                        size: font.size()
                    }, loader);
                }
            }
        }

        load_glyphs_for_font!(regular);
        load_glyphs_for_font!(bold);
        load_glyphs_for_font!(italic);

        cache
    }

    pub fn font_metrics(&self) -> font::Metrics {
        self.rasterizer.metrics(self.font_key, self.font_size)
    }

    fn load_and_cache_glyph<L>(&mut self, glyph_key: GlyphKey, loader: &mut L)
        where L: LoadGlyph
    {
        let rasterized = self.rasterizer.get_glyph(&glyph_key);
        let glyph = loader.load_glyph(&rasterized);
        self.cache.insert(glyph_key, glyph);
    }

    pub fn get<L>(&mut self, glyph_key: &GlyphKey, loader: &mut L) -> Option<&Glyph>
        where L: LoadGlyph
    {
        // Return glyph if it's already loaded
        // hi borrowck
        {
            if self.cache.contains_key(glyph_key) {
                return self.cache.get(glyph_key);
            }
        }

        // Rasterize and load the glyph
        self.load_and_cache_glyph(glyph_key.to_owned(), loader);
        self.cache.get(&glyph_key)
    }
}

#[derive(Debug)]
#[repr(C)]
struct InstanceData {
    // coords
    col: f32,
    row: f32,
    // glyph offset
    left: f32,
    top: f32,
    // glyph scale
    width: f32,
    height: f32,
    // uv offset
    uv_left: f32,
    uv_bot: f32,
    // uv scale
    uv_width: f32,
    uv_height: f32,
    // color
    r: f32,
    g: f32,
    b: f32,
    // background color
    bg_r: f32,
    bg_g: f32,
    bg_b: f32,
}

#[derive(Debug)]
pub struct QuadRenderer {
    program: ShaderProgram,
    vao: GLuint,
    vbo: GLuint,
    ebo: GLuint,
    vbo_instance: GLuint,
    atlas: Vec<Atlas>,
    active_tex: GLuint,
    batch: Batch,
    colors: [Rgb; 18],
    rx: mpsc::Receiver<Msg>,
}

#[derive(Debug)]
pub struct RenderApi<'a> {
    active_tex: &'a mut GLuint,
    batch: &'a mut Batch,
    atlas: &'a mut Vec<Atlas>,
    program: &'a mut ShaderProgram,
    colors: &'a [Rgb; 18],
}

#[derive(Debug)]
pub struct LoaderApi<'a> {
    active_tex: &'a mut GLuint,
    atlas: &'a mut Vec<Atlas>,
}

#[derive(Debug)]
pub struct PackedVertex {
    x: f32,
    y: f32,
}

#[derive(Debug)]
pub struct Batch {
    tex: GLuint,
    instances: Vec<InstanceData>,
    colors: [Rgb; 18],
}

impl Batch {
    #[inline]
    pub fn new(colors: [Rgb; 18]) -> Batch {
        Batch {
            tex: 0,
            instances: Vec::with_capacity(BATCH_MAX),
            colors: colors,
        }
    }

    pub fn add_item(&mut self, row: f32, col: f32, cell: &Cell, glyph: &Glyph) {
        if self.is_empty() {
            self.tex = glyph.tex_id;
        }

        let fg = match cell.fg {
            ::term::cell::Color::Rgb(rgb) => rgb,
            ::term::cell::Color::Ansi(ansi) => self.colors[ansi as usize],
        };

        let bg = match cell.bg {
            ::term::cell::Color::Rgb(rgb) => rgb,
            ::term::cell::Color::Ansi(ansi) => self.colors[ansi as usize],
        };

        let mut instance = InstanceData {
            col: col,
            row: row,

            top: glyph.top,
            left: glyph.left,
            width: glyph.width,
            height: glyph.height,

            uv_bot: glyph.uv_bot,
            uv_left: glyph.uv_left,
            uv_width: glyph.uv_width,
            uv_height: glyph.uv_height,

            r: fg.r as f32,
            g: fg.g as f32,
            b: fg.b as f32,

            bg_r: bg.r as f32,
            bg_g: bg.g as f32,
            bg_b: bg.b as f32,
        };

        if cell.flags.contains(cell::INVERSE) {
            instance.r = bg.r as f32;
            instance.g = bg.g as f32;
            instance.b = bg.b as f32;

            instance.bg_r = fg.r as f32;
            instance.bg_g = fg.g as f32;
            instance.bg_b = fg.b as f32;
        }

        self.instances.push(instance);
    }

    #[inline]
    pub fn full(&self) -> bool {
        self.capacity() == self.len()
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
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

/// Maximum items to be drawn in a batch.
const BATCH_MAX: usize = 4096;
const ATLAS_SIZE: i32 = 1024;

impl QuadRenderer {
    // TODO should probably hand this a transform instead of width/height
    pub fn new(config: &Config, width: u32, height: u32) -> QuadRenderer {
        let program = ShaderProgram::new(width, height).unwrap();

        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;
        let mut ebo: GLuint = 0;

        let mut vbo_instance: GLuint = 0;


        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::GenBuffers(1, &mut ebo);
            gl::GenBuffers(1, &mut vbo_instance);
            gl::BindVertexArray(vao);

            // ----------------------------
            // setup vertex position buffer
            // ----------------------------
            // Top right, Bottom right, Bottom left, Top left
            let vertices = [
                PackedVertex { x: 1.0, y: 1.0 },
                PackedVertex { x: 1.0, y: 0.0 },
                PackedVertex { x: 0.0, y: 0.0 },
                PackedVertex { x: 0.0, y: 1.0 },
            ];

            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            gl::VertexAttribPointer(0, 2,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<PackedVertex>() as i32,
                                    ptr::null());
            gl::EnableVertexAttribArray(0);

            gl::BufferData(gl::ARRAY_BUFFER,
                           (size_of::<PackedVertex>() * vertices.len()) as GLsizeiptr,
                           vertices.as_ptr() as *const _,
                           gl::STATIC_DRAW);

            // ---------------------
            // Set up element buffer
            // ---------------------
            let indices: [u32; 6] = [0, 1, 3,
                                     1, 2, 3];

            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(gl::ELEMENT_ARRAY_BUFFER,
                           (6 * size_of::<u32>()) as isize,
                           indices.as_ptr() as *const _,
                           gl::STATIC_DRAW);

            // ----------------------------
            // Setup vertex instance buffer
            // ----------------------------
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_instance);
            gl::BufferData(gl::ARRAY_BUFFER,
                           (BATCH_MAX * size_of::<InstanceData>()) as isize,
                           ptr::null(), gl::STREAM_DRAW);
            // coords
            gl::VertexAttribPointer(1, 2,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<InstanceData>() as i32,
                                    ptr::null());
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribDivisor(1, 1);
            // glyphoffset
            gl::VertexAttribPointer(2, 4,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<InstanceData>() as i32,
                                    (2 * size_of::<f32>()) as *const _);
            gl::EnableVertexAttribArray(2);
            gl::VertexAttribDivisor(2, 1);
            // uv
            gl::VertexAttribPointer(3, 4,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<InstanceData>() as i32,
                                    (6 * size_of::<f32>()) as *const _);
            gl::EnableVertexAttribArray(3);
            gl::VertexAttribDivisor(3, 1);
            // color
            gl::VertexAttribPointer(4, 3,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<InstanceData>() as i32,
                                    (10 * size_of::<f32>()) as *const _);
            gl::EnableVertexAttribArray(4);
            gl::VertexAttribDivisor(4, 1);
            // color
            gl::VertexAttribPointer(5, 3,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<InstanceData>() as i32,
                                    (13 * size_of::<f32>()) as *const _);
            gl::EnableVertexAttribArray(5);
            gl::VertexAttribDivisor(5, 1);

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        let (msg_tx, msg_rx) = mpsc::channel();

        if cfg!(feature = "live-shader-reload") {
            ::std::thread::spawn(move || {
                let (tx, rx) = ::std::sync::mpsc::channel();
                let mut watcher = Watcher::new(tx).unwrap();
                watcher.watch(TEXT_SHADER_F_PATH).expect("watch fragment shader");
                watcher.watch(TEXT_SHADER_V_PATH).expect("watch vertex shader");

                loop {
                    let event = rx.recv().expect("watcher event");
                    let ::notify::Event { path, op } = event;

                    if let Ok(op) = op {
                        if op.contains(op::RENAME) {
                            continue;
                        }

                        if op.contains(op::IGNORED) {
                            if let Some(path) = path.as_ref() {
                                if let Err(err) = watcher.watch(path) {
                                    println!("failed to establish watch on {:?}: {:?}", path, err);
                                }
                            }

                            msg_tx.send(Msg::ShaderReload)
                                .expect("msg send ok");
                        }
                    }
                }
            });
        }

        let mut renderer = QuadRenderer {
            program: program,
            vao: vao,
            vbo: vbo,
            ebo: ebo,
            vbo_instance: vbo_instance,
            atlas: Vec::new(),
            active_tex: 0,
            batch: Batch::new(config.color_list()),
            colors: config.color_list(),
            rx: msg_rx,
        };

        let atlas = Atlas::new(ATLAS_SIZE);
        renderer.atlas.push(atlas);

        renderer
    }

    pub fn update_config(&mut self, config: &Config) {
        self.colors = config.color_list();
        self.batch.colors = config.color_list();
    }

    pub fn with_api<F, T>(&mut self, props: &term::SizeInfo, func: F) -> T
        where F: FnOnce(RenderApi) -> T
    {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::ShaderReload => {
                    self.reload_shaders(props.width as u32, props.height as u32);
                }
            }
        }

        unsafe {
            self.program.activate();
            self.program.set_term_uniforms(props);

            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.ebo);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo_instance);
            gl::ActiveTexture(gl::TEXTURE0);
        }

        let res = func(RenderApi {
            active_tex: &mut self.active_tex,
            batch: &mut self.batch,
            atlas: &mut self.atlas,
            program: &mut self.program,
            colors: &self.colors,
        });

        unsafe {
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            self.program.deactivate();
        }

        res
    }

    pub fn with_loader<F, T>(&mut self, func: F) -> T
        where F: FnOnce(LoaderApi) -> T
    {
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0);
        }

        func(LoaderApi {
            active_tex: &mut self.active_tex,
            atlas: &mut self.atlas,
        })
    }

    pub fn reload_shaders(&mut self, width: u32, height: u32) {
        let program = match ShaderProgram::new(width, height) {
            Ok(program) => program,
            Err(err) => {
                match err {
                    ShaderCreationError::Io(err) => {
                        println!("Error reading shader file: {}", err);
                    },
                    ShaderCreationError::Compile(path, log) => {
                        println!("Error compiling shader at {:?}", path);
                        io::copy(&mut log.as_bytes(), &mut io::stdout()).unwrap();
                    }
                }

                return;
            }
        };

        self.active_tex = 0;
        self.program = program;
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        // viewport
        unsafe {
            gl::Viewport(0, 0, width, height);
        }

        // update projection
        self.program.activate();
        self.program.update_projection(width as f32, height as f32);
        self.program.deactivate();
    }
}

impl<'a> RenderApi<'a> {
    fn render_batch(&mut self) {
        unsafe {
            gl::BufferSubData(gl::ARRAY_BUFFER, 0, self.batch.size() as isize,
                              self.batch.instances.as_ptr() as *const _);
        }

        // Bind texture if necessary
        if *self.active_tex != self.batch.tex {
            unsafe {
                gl::BindTexture(gl::TEXTURE_2D, self.batch.tex);
            }
            *self.active_tex = self.batch.tex;
        }

        unsafe {

            self.program.set_background_pass(true);
            gl::DrawElementsInstanced(gl::TRIANGLES,
                                      6, gl::UNSIGNED_INT, ptr::null(),
                                      self.batch.len() as GLsizei);
            self.program.set_background_pass(false);
            gl::DrawElementsInstanced(gl::TRIANGLES,
                                      6, gl::UNSIGNED_INT, ptr::null(),
                                      self.batch.len() as GLsizei);
        }

        self.batch.clear();
    }
    /// Render a string in a predefined location. Used for printing render time for profiling and
    /// optimization.
    pub fn render_string(
        &mut self,
        s: &str,
        glyph_cache: &mut GlyphCache,
        color: &::term::cell::Color,
    ) {
        let row = 40.0;
        let mut col = 100.0;

        for c in s.chars() {
            let glyph_key = GlyphKey {
                font_key: glyph_cache.font_key,
                size: glyph_cache.font_size,
                c: c
            };

            if let Some(glyph) = glyph_cache.get(&glyph_key, self) {
                let cell = Cell {
                    c: c,
                    fg: color.clone(),
                    bg: cell::Color::Rgb(Rgb { r: 0, g: 0, b: 0}),
                    flags: cell::INVERSE,
                };
                self.add_render_item(row, col, &cell, glyph);
            }

            col += 1.0;
        }
    }

    #[inline]
    fn add_render_item(&mut self, row: f32, col: f32, cell: &Cell, glyph: &Glyph) {
        // Flush batch if tex changing
        if !self.batch.is_empty() {
            if self.batch.tex != glyph.tex_id {
                self.render_batch();
            }
        }

        self.batch.add_item(row, col, cell, glyph);

        // Render batch and clear if it's full
        if self.batch.full() {
            self.render_batch();
        }
    }

    pub fn render_grid(
        &mut self,
        grid: &Grid<Cell>,
        glyph_cache: &mut GlyphCache
    ) {
        // TODO should be built into renderer
        let color = self.colors[::ansi::Color::Background as usize];
        unsafe {
            gl::ClearColor(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                1.0
                );
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        for (i, line) in grid.lines().enumerate() {
            for (j, cell) in line.cells().enumerate() {
                // Skip empty cells
                if cell.c == ' ' && cell.bg == cell::Color::Ansi(::ansi::Color::Background) &&
                   !cell.flags.contains(cell::INVERSE)
                {
                    continue;
                }

                // Get font key for cell
                // FIXME this is super inefficient.
                let mut font_key = glyph_cache.font_key;
                if cell.flags.contains(cell::BOLD) {
                    font_key = glyph_cache.bold_key;
                } else if cell.flags.contains(cell::ITALIC) {
                    font_key = glyph_cache.italic_key;
                }

                let glyph_key = GlyphKey {
                    font_key: font_key,
                    size: glyph_cache.font_size,
                    c: cell.c
                };

                // Add cell to batch if glyph available
                if let Some(glyph) = glyph_cache.get(&glyph_key, self) {
                    self.add_render_item(i as f32, j as f32, cell, glyph);
                }
            }
        }
    }
}

impl<'a> LoadGlyph for LoaderApi<'a> {
    /// Load a glyph into a texture atlas
    ///
    /// If the current atlas is full, a new one will be created.
    fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph {
        // At least one atlas is guaranteed to be in the `self.atlas` list; thus the unwrap.
        match self.atlas.last_mut().unwrap().insert(rasterized, &mut self.active_tex) {
            Ok(glyph) => glyph,
            Err(_) => {
                let atlas = Atlas::new(ATLAS_SIZE);
                *self.active_tex = 0; // Atlas::new binds a texture. Ugh this is sloppy.
                self.atlas.push(atlas);
                self.load_glyph(rasterized)
            }
        }
    }
}

impl<'a> LoadGlyph for RenderApi<'a> {
    /// Load a glyph into a texture atlas
    ///
    /// If the current atlas is full, a new one will be created.
    fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph {
        // At least one atlas is guaranteed to be in the `self.atlas` list; thus the unwrap.
        match self.atlas.last_mut().unwrap().insert(rasterized, &mut self.active_tex) {
            Ok(glyph) => glyph,
            Err(_) => {
                let atlas = Atlas::new(ATLAS_SIZE);
                *self.active_tex = 0; // Atlas::new binds a texture. Ugh this is sloppy.
                self.atlas.push(atlas);
                self.load_glyph(rasterized)
            }
        }
    }
}

impl<'a> Drop for RenderApi<'a> {
    fn drop(&mut self) {
        if !self.batch.is_empty() {
            self.render_batch();
        }
    }
}

impl ShaderProgram {
    pub fn activate(&self) {
        unsafe {
            gl::UseProgram(self.id);
        }
    }

    pub fn deactivate(&self) {
        unsafe {
            gl::UseProgram(0);
        }
    }

    pub fn new(
        width: u32,
        height: u32
    ) -> Result<ShaderProgram, ShaderCreationError> {
        let vertex_source = if cfg!(feature = "live-shader-reload") {
            None
        } else {
            Some(TEXT_SHADER_V)
        };
        let vertex_shader = ShaderProgram::create_shader(
            TEXT_SHADER_V_PATH,
            gl::VERTEX_SHADER,
            vertex_source
        )?;
        let frag_source = if cfg!(feature = "live-shader-reload") {
            None
        } else {
            Some(TEXT_SHADER_F)
        };
        let fragment_shader = ShaderProgram::create_shader(
            TEXT_SHADER_F_PATH,
            gl::FRAGMENT_SHADER,
            frag_source
        )?;
        let program = ShaderProgram::create_program(vertex_shader, fragment_shader);

        unsafe {
            gl::DeleteShader(vertex_shader);
            gl::DeleteShader(fragment_shader);
            gl::UseProgram(program);
        }

        macro_rules! cptr {
            ($thing:expr) => { $thing.as_ptr() as *const _ }
        }

        macro_rules! assert_uniform_valid {
            ($uniform:expr) => {
                assert!($uniform != gl::INVALID_VALUE as i32);
                assert!($uniform != gl::INVALID_OPERATION as i32);
            };
            ( $( $uniform:expr ),* ) => {
                $( assert_uniform_valid!($uniform); )*
            };
        }

        // get uniform locations
        let (projection, term_dim, cell_dim, background) = unsafe {
            (
                gl::GetUniformLocation(program, cptr!(b"projection\0")),
                gl::GetUniformLocation(program, cptr!(b"termDim\0")),
                gl::GetUniformLocation(program, cptr!(b"cellDim\0")),
                gl::GetUniformLocation(program, cptr!(b"backgroundPass\0")),
            )
        };

        assert_uniform_valid!(projection, term_dim, cell_dim);

        let mut color_uniforms: [GLint; 18] = unsafe { ::std::mem::uninitialized() };
        for i in 0..18 {
            color_uniforms[i] = unsafe {
                let s = format!("colors[{}]\0", i).into_bytes();
                gl::GetUniformLocation(program, cptr!(&s[..]))
            };

            assert_uniform_valid!(color_uniforms[i]);
        }

        let shader = ShaderProgram {
            id: program,
            u_projection: projection,
            u_term_dim: term_dim,
            u_cell_dim: cell_dim,
            u_background: background,
        };

        shader.update_projection(width as f32, height as f32);

        shader.deactivate();

        Ok(shader)
    }

    fn update_projection(&self, width: f32, height: f32) {
        // set projection uniform
        let ortho = cgmath::ortho(0., width, 0., height, -1., 1.);
        let projection: [[f32; 4]; 4] = ortho.into();

        println!("width: {}, height: {}", width, height);

        unsafe {
            gl::UniformMatrix4fv(self.u_projection,
                                 1, gl::FALSE, projection.as_ptr() as *const _);
        }

    }

    fn set_term_uniforms(&self, props: &term::SizeInfo) {
        unsafe {
            gl::Uniform2f(self.u_term_dim, props.width, props.height);
            gl::Uniform2f(self.u_cell_dim, props.cell_width, props.cell_height);
        }
    }

    fn set_background_pass(&self, background_pass: bool) {
        let value = if background_pass {
            1
        } else {
            0
        };

        unsafe {
            gl::Uniform1i(self.u_background, value);
        }
    }

    fn create_program(vertex: GLuint, fragment: GLuint) -> GLuint {
        unsafe {
            let program = gl::CreateProgram();
            gl::AttachShader(program, vertex);
            gl::AttachShader(program, fragment);
            gl::LinkProgram(program);

            let mut success: GLint = 0;
            gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);

            if success != (gl::TRUE as GLint) {
                println!("{}", get_program_info_log(program));
                panic!("failed to link shader program");
            }
            program
        }
    }


    fn create_shader(
        path: &str,
        kind: GLenum,
        source: Option<&'static str>
    ) -> Result<GLuint, ShaderCreationError> {
        let from_disk;
        let source = if let Some(src) = source {
            src
        } else {
            from_disk = read_file(path)?;
            &from_disk[..]
        };

        let len: [GLint; 1] = [source.len() as GLint];

        let shader = unsafe {
            let shader = gl::CreateShader(kind);
            gl::ShaderSource(shader, 1, &(source.as_ptr() as *const i8), len.as_ptr());
            gl::CompileShader(shader);
            shader
        };

        let mut success: GLint = 0;
        unsafe {
            gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
        }

        if success == (gl::TRUE as GLint) {
            Ok(shader)
        } else {
            // Read log
            let log = get_shader_info_log(shader);

            // Cleanup
            unsafe { gl::DeleteShader(shader); }

            Err(ShaderCreationError::Compile(PathBuf::from(path), log))
        }
    }
}

impl Drop for ShaderProgram {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.id);
        }
    }
}

fn get_program_info_log(program: GLuint) -> String {
    // Get expected log length
    let mut max_length: GLint = 0;
    unsafe {
        gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut max_length);
    }

    // Read the info log
    let mut actual_length: GLint = 0;
    let mut buf: Vec<u8> = Vec::with_capacity(max_length as usize);
    unsafe {
        gl::GetProgramInfoLog(program, max_length, &mut actual_length, buf.as_mut_ptr() as *mut _);
    }

    // Build a string
    unsafe {
        buf.set_len(actual_length as usize);
    }

    // XXX should we expect opengl to return garbage?
    String::from_utf8(buf).unwrap()
}

fn get_shader_info_log(shader: GLuint) -> String {
    // Get expected log length
    let mut max_length: GLint = 0;
    unsafe {
        gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut max_length);
    }

    // Read the info log
    let mut actual_length: GLint = 0;
    let mut buf: Vec<u8> = Vec::with_capacity(max_length as usize);
    unsafe {
        gl::GetShaderInfoLog(shader, max_length, &mut actual_length, buf.as_mut_ptr() as *mut _);
    }

    // Build a string
    unsafe {
        buf.set_len(actual_length as usize);
    }

    // XXX should we expect opengl to return garbage?
    String::from_utf8(buf).unwrap()
}

fn read_file(path: &str) -> Result<String, io::Error> {
    let mut f = File::open(path)?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;

    Ok(buf)
}

#[derive(Debug)]
pub enum ShaderCreationError {
    /// Error reading file
    Io(io::Error),

    /// Error compiling shader
    Compile(PathBuf, String),
}

impl ::std::error::Error for ShaderCreationError {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            ShaderCreationError::Io(ref err) => Some(err),
            ShaderCreationError::Compile(_, _) => None,
        }
    }

    fn description(&self) -> &str {
        match *self {
            ShaderCreationError::Io(ref err) => err.description(),
            ShaderCreationError::Compile(ref _path, ref s) => s.as_str(),
        }
    }
}

impl ::std::fmt::Display for ShaderCreationError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            ShaderCreationError::Io(ref err) => write!(f, "Error creating shader: {}", err),
            ShaderCreationError::Compile(ref _path, ref s) => {
                write!(f, "Error compiling shader: {}", s)
            },
        }
    }
}

impl From<io::Error> for ShaderCreationError {
    fn from(val: io::Error) -> ShaderCreationError {
        ShaderCreationError::Io(val)
    }
}


/// Manages a single texture atlas
///
/// The strategy for filling an atlas looks roughly like this:
///
///                           (width, height)
///   ┌─────┬─────┬─────┬─────┬─────┐
///   │ 10  │     │     │     │     │ <- Empty spaces; can be filled while
///   │     │     │     │     │     │    glyph_height < height - row_baseline
///   ├⎼⎼⎼⎼⎼┼⎼⎼⎼⎼⎼┼⎼⎼⎼⎼⎼┼⎼⎼⎼⎼⎼┼⎼⎼⎼⎼⎼┤
///   │ 5   │ 6   │ 7   │ 8   │ 9   │
///   │     │     │     │     │     │
///   ├⎼⎼⎼⎼⎼┼⎼⎼⎼⎼⎼┼⎼⎼⎼⎼⎼┼⎼⎼⎼⎼⎼┴⎼⎼⎼⎼⎼┤ <- Row height is tallest glyph in row; this is
///   │ 1   │ 2   │ 3   │ 4         │    used as the baseline for the following row.
///   │     │     │     │           │ <- Row considered full when next glyph doesn't
///   └─────┴─────┴─────┴───────────┘    fit in the row.
/// (0, 0)  x->
#[derive(Debug)]
struct Atlas {
    /// Texture id for this atlas
    id: GLuint,

    /// Width of atlas
    width: i32,

    /// Height of atlas
    height: i32,

    /// Left-most free pixel in a row.
    ///
    /// This is called the extent because it is the upper bound of used pixels
    /// in a row.
    row_extent: i32,

    /// Baseline for glyphs in the current row
    row_baseline: i32,

    /// Tallest glyph in current row
    ///
    /// This is used as the advance when end of row is reached
    row_tallest: i32,
}

/// Error that can happen when inserting a texture to the Atlas
enum AtlasInsertError {
    /// Texture atlas is full
    Full,
}

impl Atlas {
    fn new(size: i32) -> Atlas {
        let mut id: GLuint = 0;
        unsafe {
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::GenTextures(1, &mut id);
            gl::BindTexture(gl::TEXTURE_2D, id);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB as i32,
                size,
                size,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                ptr::null()
            );

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);

            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        Atlas {
            id: id,
            width: size,
            height: size,
            row_extent: 0,
            row_baseline: 0,
            row_tallest: 0,
        }
    }

    /// Insert a RasterizedGlyph into the texture atlas
    pub fn insert(&mut self,
                  glyph: &RasterizedGlyph,
                  active_tex: &mut u32)
                  -> Result<Glyph, AtlasInsertError>
    {
        // If there's not enough room in current row, go onto next one
        if !self.room_in_row(glyph) {
            self.advance_row()?;
        }

        // If there's still not room, there's nothing that can be done here.
        if !self.room_in_row(glyph) {
            return Err(AtlasInsertError::Full);
        }

        // There appears to be room; load the glyph.
        Ok(self.insert_inner(glyph, active_tex))
    }

    /// Insert the glyph without checking for room
    ///
    /// Internal function for use once atlas has been checked for space. GL errors could still occur
    /// at this point if we were checking for them; hence, the Result.
    fn insert_inner(&mut self,
                    glyph: &RasterizedGlyph,
                    active_tex: &mut u32)
                    -> Glyph
    {
        let offset_y = self.row_baseline;
        let offset_x = self.row_extent;
        let height = glyph.height as i32;
        let width = glyph.width as i32;

        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.id);

            // Load data into OpenGL
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                offset_x,
                offset_y,
                width,
                height,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                glyph.buf.as_ptr() as *const _
            );

            gl::BindTexture(gl::TEXTURE_2D, 0);
            *active_tex = 0;
        }

        // Update Atlas state
        self.row_extent = offset_x + width;
        if height > self.row_tallest {
            self.row_tallest = height;
        }

        // Generate UV coordinates
        let uv_bot = offset_y as f32 / self.height as f32;
        let uv_left = offset_x as f32 / self.width as f32;
        let uv_height = height as f32 / self.height as f32;
        let uv_width = width as f32 / self.width as f32;

        let g = Glyph {
            tex_id: self.id,
            top: glyph.top as f32,
            width: width as f32,
            height: height as f32,
            left: glyph.left as f32,
            uv_bot: uv_bot,
            uv_left: uv_left,
            uv_width: uv_width,
            uv_height: uv_height,
        };

        // Return the glyph
        g
    }

    /// Check if there's room in the current row for given glyph
    fn room_in_row(&self, raw: &RasterizedGlyph) -> bool {
        let next_extent = self.row_extent + raw.width as i32;
        let enough_width = next_extent <= self.width;
        let enough_height = (raw.height as i32) < (self.height - self.row_baseline);

        enough_width && enough_height
    }

    /// Mark current row as finished and prepare to insert into the next row
    fn advance_row(&mut self) -> Result<(), AtlasInsertError> {
        let advance_to = self.row_baseline + self.row_tallest;
        if self.height - advance_to <= 0 {
            return Err(AtlasInsertError::Full);
        }

        self.row_baseline = advance_to;
        self.row_extent = 0;
        self.row_tallest = 0;

        Ok(())
    }
}
