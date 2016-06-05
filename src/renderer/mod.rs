use std::ffi::CString;
use std::fs::File;
use std::io::{self, Read};
use std::mem::size_of;
use std::path::{PathBuf, Path};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicBool};

use cgmath::{self, Matrix};
use euclid::{Rect, Size2D, Point2D};
use gl::types::*;
use gl;
use notify::{Watcher as WatcherApi, RecommendedWatcher as Watcher, op};
use text::RasterizedGlyph;
use grid::Grid;
use term;

use super::{Rgb, TermProps, GlyphCache};

static TEXT_SHADER_F_PATH: &'static str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/text.f.glsl");
static TEXT_SHADER_V_PATH: &'static str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/text.v.glsl");

pub struct QuadRenderer {
    program: ShaderProgram,
    should_reload: Arc<AtomicBool>,
    vao: GLuint,
    vbo: GLuint,
    ebo: GLuint,
    active_color: Rgb,
    atlas: Vec<Atlas>,
    active_tex: GLuint,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct PackedVertex {
    x: f32,
    y: f32,
}


impl QuadRenderer {
    // TODO should probably hand this a transform instead of width/height
    pub fn new(width: u32, height: u32) -> QuadRenderer {
        let program = ShaderProgram::new(width, height).unwrap();

        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;
        let mut ebo: GLuint = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::GenBuffers(1, &mut ebo);
            gl::BindVertexArray(vao);

            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            // Top right, Bottom right, Bottom left, Top left
            let vertices = [
                PackedVertex { x: 1.0, y: 1.0 },
                PackedVertex { x: 1.0, y: 0.0 },
                PackedVertex { x: 0.0, y: 0.0 },
                PackedVertex { x: 0.0, y: 1.0 },
            ];

            gl::BufferData(
                gl::ARRAY_BUFFER,
                (size_of::<PackedVertex>() * vertices.len()) as GLsizeiptr,
                vertices.as_ptr() as *const _,
                gl::STATIC_DRAW
            );

            let indices: [u32; 6] = [0, 1, 3,
                                     1, 2, 3];

            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(gl::ELEMENT_ARRAY_BUFFER,
                           6 * size_of::<u32>() as isize,
                           indices.as_ptr() as *const _,
                           gl::STATIC_DRAW);

            gl::EnableVertexAttribArray(0);

            // positions
            gl::VertexAttribPointer(0, 2,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<PackedVertex>() as i32,
                                    ptr::null());

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
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
            active_color: Rgb { r: 0, g: 0, b: 0 },
            atlas: Vec::new(),
            active_tex: 0,
        };

        let atlas = renderer.create_atlas(1024);
        renderer.atlas.push(atlas);

        renderer
    }

    /// Render a string in a predefined location. Used for printing render time for profiling and
    /// optimization.
    pub fn render_string(&mut self,
                     s: &str,
                     glyph_cache: &GlyphCache,
                     props: &TermProps,
                     color: &Rgb)
    {
        self.prepare_render(props);

        let row = 40.0;
        let mut col = 100.0;
        for c in s.chars() {
            if let Some(glyph) = glyph_cache.get(&c) {
                self.render(glyph, row, col, color, c);
            }

            col += 1.0;
        }

        self.finish_render();
    }

    pub fn render_cursor(&mut self,
                         cursor: term::Cursor,
                         glyph_cache: &GlyphCache,
                         props: &TermProps)
    {
        self.prepare_render(props);
        if let Some(glyph) = glyph_cache.get(&term::CURSOR_SHAPE) {
            self.render(glyph, cursor.y as f32, cursor.x as f32,
                        &term::DEFAULT_FG, term::CURSOR_SHAPE);
        }

        self.finish_render();
    }

    pub fn render_grid(&mut self, grid: &Grid, glyph_cache: &GlyphCache, props: &TermProps) {
        self.prepare_render(props);

        for (i, row) in grid.rows().enumerate() {
            for (j, cell) in row.cells().enumerate() {
                // Skip empty cells
                if cell.c == ' ' {
                    continue;
                }

                // Render if glyph is loaded
                if let Some(glyph) = glyph_cache.get(&cell.c) {
                    self.render(glyph, i as f32, j as f32, &cell.fg, cell.c);
                }
            }
        }

        self.finish_render();
    }

    fn prepare_render(&mut self, props: &TermProps) {
        unsafe {
            self.program.activate();
            self.program.set_term_uniforms(props);

            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.ebo);
            gl::ActiveTexture(gl::TEXTURE0);
        }

        if self.should_reload.load(Ordering::Relaxed) {
            self.reload_shaders(props.width as u32, props.height as u32);
        }
    }

    fn finish_render(&mut self) {
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
        self.active_color = Rgb { r: 0, g: 0, b: 0 };
        self.program = program;
    }

    fn render(&mut self, glyph: &Glyph, row: f32, col: f32, color: &Rgb, c: char) {
        if &self.active_color != color {
            unsafe {
                gl::Uniform3i(self.program.u_color,
                              color.r as i32,
                              color.g as i32,
                              color.b as i32);
            }
            self.active_color = color.to_owned();
        }

        self.program.set_glyph_uniforms(row, col, glyph);

        unsafe {
            // Bind texture if it changed
            if self.active_tex != glyph.tex_id {
                gl::BindTexture(gl::TEXTURE_2D, glyph.tex_id);
                self.active_tex = glyph.tex_id;
            }

            gl::DrawElements(gl::TRIANGLES, 6, gl::UNSIGNED_INT, ptr::null());
        }
    }

    /// Load a glyph into a texture atlas
    ///
    /// If the current atlas is full, a new one will be created.
    pub fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph {
        match self.atlas.last_mut().unwrap().insert(rasterized, &mut self.active_tex) {
            Ok(glyph) => glyph,
            Err(_) => {
                let atlas = self.create_atlas(1024);
                self.atlas.push(atlas);
                self.load_glyph(rasterized)
            }
        }
    }

    fn create_atlas(&mut self, size: i32) -> Atlas {
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
            self.active_tex = 0;
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
}

fn get_rect(glyph: &Glyph, x: f32, y: f32) -> Rect<f32> {
    Rect::new(
        Point2D::new(x + glyph.left as f32, y - (glyph.height - glyph.top) as f32),
        Size2D::new(glyph.width as f32, glyph.height as f32)
    )
}

pub struct ShaderProgram {
    // Program id
    id: GLuint,

    /// projection matrix uniform
    u_projection: GLint,

    /// color uniform
    u_color: GLint,

    /// Terminal dimensions (pixels)
    u_term_dim: GLint,

    /// Cell dimensions (pixels)
    u_cell_dim: GLint,

    /// Cell separation (pixels)
    u_cell_sep: GLint,

    /// Cell coordinates in grid
    u_cell_coord: GLint,

    /// Glyph scale
    u_glyph_scale: GLint,

    /// Glyph offset
    u_glyph_offest: GLint,

    /// Atlas scale
    u_uv_scale: GLint,

    /// Atlas offset
    u_uv_offset: GLint,
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
        let (projection, color, term_dim, cell_dim, cell_sep) = unsafe {
            (
                gl::GetUniformLocation(program, cptr!(b"projection\0")),
                gl::GetUniformLocation(program, cptr!(b"textColor\0")),
                gl::GetUniformLocation(program, cptr!(b"termDim\0")),
                gl::GetUniformLocation(program, cptr!(b"cellDim\0")),
                gl::GetUniformLocation(program, cptr!(b"cellSep\0")),
            )
        };

        assert_uniform_valid!(projection, color, term_dim, cell_dim, cell_sep);

        let (cell_coord, glyph_scale, glyph_offest, uv_scale, uv_offset) = unsafe {
            (
                gl::GetUniformLocation(program, cptr!(b"gridCoords\0")),
                gl::GetUniformLocation(program, cptr!(b"glyphScale\0")),
                gl::GetUniformLocation(program, cptr!(b"glyphOffset\0")),
                gl::GetUniformLocation(program, cptr!(b"uvScale\0")),
                gl::GetUniformLocation(program, cptr!(b"uvOffset\0")),
            )
        };

        assert_uniform_valid!(cell_coord, glyph_scale, glyph_offest, uv_scale, uv_offset);

        // Initialize to known color (black)
        unsafe {
            gl::Uniform3i(color, 0, 0, 0);
        }

        let shader = ShaderProgram {
            id: program,
            u_projection: projection,
            u_color: color,
            u_term_dim: term_dim,
            u_cell_dim: cell_dim,
            u_cell_sep: cell_sep,
            u_cell_coord: cell_coord,
            u_glyph_scale: glyph_scale,
            u_glyph_offest: glyph_offest,
            u_uv_scale: uv_scale,
            u_uv_offset: uv_offset,
        };

        // set projection uniform
        let ortho = cgmath::ortho(0., width as f32, 0., height as f32, -1., 1.);
        let projection: [[f32; 4]; 4] = ortho.into();

        println!("width: {}, height: {}", width, height);

        unsafe {
            gl::UniformMatrix4fv(shader.u_projection,
                                 1, gl::FALSE, projection.as_ptr() as *const _);
        }

        shader.deactivate();

        Ok(shader)
    }

    fn set_term_uniforms(&self, props: &TermProps) {
        unsafe {
            gl::Uniform2f(self.u_term_dim, props.width, props.height);
            gl::Uniform2f(self.u_cell_dim, props.cell_width, props.cell_height);
            gl::Uniform2f(self.u_cell_sep, props.sep_x, props.sep_y);
        }
    }

    fn set_glyph_uniforms(&self, row: f32, col: f32, glyph: &Glyph) {
        unsafe {
            gl::Uniform2f(self.u_cell_coord, col, row); // col = x; row = y
            gl::Uniform2f(self.u_glyph_scale, glyph.width, glyph.height);
            gl::Uniform2f(self.u_glyph_offest, glyph.left, glyph.top);
            gl::Uniform2f(self.u_uv_scale, glyph.uv_width, glyph.uv_height);
            gl::Uniform2f(self.u_uv_offset, glyph.uv_left, glyph.uv_bot);
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

#[derive(Debug)]
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
