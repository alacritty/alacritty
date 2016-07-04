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
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Read};
use std::mem::size_of;
use std::path::{PathBuf};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicBool};

use cgmath;
use gl::types::*;
use gl;
use grid::Grid;
use index;
use notify::{Watcher as WatcherApi, RecommendedWatcher as Watcher, op};
use term::{self, cell, Cell};

use font::{Rasterizer, RasterizedGlyph, FontDesc};

use super::Rgb;

static TEXT_SHADER_F_PATH: &'static str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/text.f.glsl");
static TEXT_SHADER_V_PATH: &'static str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/text.v.glsl");

/// LoadGlyph allows for copying a rasterized glyph into graphics memory
pub trait LoadGlyph {
    /// Load the rasterized glyph into GPU memory
    fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph;
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
    u_background: GLint
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
    cache: HashMap<char, Glyph>,

    /// Rasterizer for loading new glyphs
    rasterizer: Rasterizer,

    /// Font description
    desc: FontDesc,

    /// Font Size
    size: f32,
}

impl GlyphCache {
    pub fn new(rasterizer: Rasterizer, desc: FontDesc, font_size: f32) -> GlyphCache {
        GlyphCache {
            cache: HashMap::new(),
            rasterizer: rasterizer,
            desc: desc,
            size: font_size,
        }
    }

    pub fn init<L>(&mut self, loader: &mut L)
        where L: LoadGlyph
    {
        for i in 32u8...128u8 {
            self.load_and_cache_glyph(i as char, loader);
        }
    }

    fn load_and_cache_glyph<L>(&mut self, c: char, loader: &mut L)
        where L: LoadGlyph
    {
        let rasterized = self.rasterizer.get_glyph(&self.desc, self.size, c);
        let glyph = loader.load_glyph(&rasterized);
        self.cache.insert(c, glyph);
    }

    pub fn get<L>(&mut self, c: char, loader: &mut L) -> Option<&Glyph>
        where L: LoadGlyph
    {
        // Return glyph if it's already loaded
        // hi borrowck
        {
            if self.cache.contains_key(&c) {
                return self.cache.get(&c);
            }
        }

        // Rasterize and load the glyph
        self.load_and_cache_glyph(c, loader);
        self.cache.get(&c)
    }
}

#[derive(Debug)]
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
    should_reload: Arc<AtomicBool>,
    vao: GLuint,
    vbo: GLuint,
    ebo: GLuint,
    vbo_instance: GLuint,
    atlas: Vec<Atlas>,
    active_tex: GLuint,
    batch: Batch,
}

#[derive(Debug)]
pub struct RenderApi<'a> {
    active_tex: &'a mut GLuint,
    batch: &'a mut Batch,
    atlas: &'a mut Vec<Atlas>,
    program: &'a mut ShaderProgram,
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
}

impl Batch {
    #[inline]
    pub fn new() -> Batch {
        Batch {
            tex: 0,
            instances: Vec::with_capacity(BATCH_MAX),
        }
    }

    pub fn add_item(&mut self, row: f32, col: f32, cell: &Cell, glyph: &Glyph) {
        if self.is_empty() {
            self.tex = glyph.tex_id;
        }

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

            r: cell.fg.r as f32,
            g: cell.fg.g as f32,
            b: cell.fg.b as f32,

            bg_r: cell.bg.r as f32,
            bg_g: cell.bg.g as f32,
            bg_b: cell.bg.b as f32,
        };

        if cell.flags.contains(cell::INVERSE) {
            instance.r = cell.bg.r as f32;
            instance.g = cell.bg.g as f32;
            instance.b = cell.bg.b as f32;

            instance.bg_r = cell.fg.r as f32;
            instance.bg_g = cell.fg.g as f32;
            instance.bg_b = cell.fg.b as f32;
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
    pub fn new(width: u32, height: u32) -> QuadRenderer {
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

        let should_reload = Arc::new(AtomicBool::new(false));
        let should_reload2 = should_reload.clone();

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

                        // This is last event we see after saving in vim
                        should_reload2.store(true, Ordering::Relaxed);
                    }
                }
            }
        });

        let mut renderer = QuadRenderer {
            program: program,
            should_reload: should_reload,
            vao: vao,
            vbo: vbo,
            ebo: ebo,
            vbo_instance: vbo_instance,
            atlas: Vec::new(),
            active_tex: 0,
            batch: Batch::new(),
        };

        let atlas = Atlas::new(ATLAS_SIZE);
        renderer.atlas.push(atlas);

        renderer
    }

    pub fn with_api<F>(&mut self, props: &term::SizeInfo, mut func: F)
        where F: FnMut(RenderApi)
    {
        if self.should_reload.load(Ordering::Relaxed) {
            self.reload_shaders(props.width as u32, props.height as u32);
        }

        unsafe {
            self.program.activate();
            self.program.set_term_uniforms(props);

            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.ebo);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo_instance);
            gl::ActiveTexture(gl::TEXTURE0);
        }

        func(RenderApi {
            active_tex: &mut self.active_tex,
            batch: &mut self.batch,
            atlas: &mut self.atlas,
            program: &mut self.program,
        });

        unsafe {
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            self.program.deactivate();
        }
    }

    pub fn reload_shaders(&mut self, width: u32, height: u32) {
        self.should_reload.store(false, Ordering::Relaxed);
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
    pub fn render_string(&mut self,
                     s: &str,
                     glyph_cache: &mut GlyphCache,
                     color: &Rgb)
    {
        let row = 40.0;
        let mut col = 100.0;

        for c in s.chars() {
            if let Some(glyph) = glyph_cache.get(c, self) {
                let cell = Cell {
                    c: c,
                    fg: *color,
                    bg: term::DEFAULT_BG,
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

    pub fn render_grid(&mut self, grid: &Grid<Cell>, glyph_cache: &mut GlyphCache) {
        for (i, line) in grid.lines().enumerate() {
            for (j, cell) in line.cells().enumerate() {
                // Skip empty cells
                if cell.c == ' ' && cell.bg == term::DEFAULT_BG {
                    continue;
                }

                // Add cell to batch if the glyph is laoded
                if let Some(glyph) = glyph_cache.get(cell.c, self) {
                    self.add_render_item(i as f32, j as f32, cell, glyph);
                }
            }
        }
    }
}

impl<'a> LoadGlyph for RenderApi<'a> {
    /// Load a glyph into a texture atlas
    ///
    /// If the current atlas is full, a new one will be created.
    fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph {
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

    pub fn new(width: u32, height: u32) -> Result<ShaderProgram, ShaderCreationError> {
        let vertex_shader = ShaderProgram::create_shader(TEXT_SHADER_V_PATH, gl::VERTEX_SHADER)?;
        let fragment_shader = ShaderProgram::create_shader(TEXT_SHADER_F_PATH,
                                                           gl::FRAGMENT_SHADER)?;
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
                $( assert_uniform_valid!($uniform) )*
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


    fn create_shader(path: &str, kind: GLenum) -> Result<GLuint, ShaderCreationError> {
        let source = CString::new(read_file(path)?).unwrap();
        let shader = unsafe {
            let shader = gl::CreateShader(kind);
            gl::ShaderSource(shader, 1, &source.as_ptr(), ptr::null());
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
