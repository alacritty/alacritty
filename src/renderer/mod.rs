use std::ffi::CString;
use std::mem::size_of;
use std::ptr;

use cgmath::{self, Matrix};
use euclid::{Rect, Size2D, Point2D};
use gl::types::*;
use gl;

use text::RasterizedGlyph;
use grid::Grid;
use term;

use super::{Rgb, TermProps, GlyphCache};

static TEXT_SHADER_V: &'static str = include_str!("../../res/text.v.glsl");
static TEXT_SHADER_F: &'static str = include_str!("../../res/text.f.glsl");

pub struct QuadRenderer {
    program: ShaderProgram,
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
    u: f32,
    v: f32,
}


impl QuadRenderer {
    // TODO should probably hand this a transform instead of width/height
    pub fn new(width: u32, height: u32) -> QuadRenderer {
        let program = ShaderProgram::new(width, height);

        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;
        let mut ebo: GLuint = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::GenBuffers(1, &mut ebo);
            gl::BindVertexArray(vao);

            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (size_of::<PackedVertex>() * 4) as GLsizeiptr,
                ptr::null(),
                gl::DYNAMIC_DRAW
            );

            let indices: [u32; 6] = [0, 1, 3,
                                     1, 2, 3];

            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(gl::ELEMENT_ARRAY_BUFFER,
                           6 * size_of::<u32>() as isize,
                           indices.as_ptr() as *const _,
                           gl::STATIC_DRAW);

            gl::EnableVertexAttribArray(0);
            gl::EnableVertexAttribArray(1);

            // positions
            gl::VertexAttribPointer(0, 2,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<PackedVertex>() as i32,
                                    ptr::null());

            // uv mapping
            gl::VertexAttribPointer(1, 2,
                                    gl::FLOAT, gl::FALSE,
                                    size_of::<PackedVertex>() as i32,
                                    (2 * size_of::<f32>()) as *const _);

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
        }

        let mut renderer = QuadRenderer {
            program: program,
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
                     cell_width: u32,
                     color: &Rgb)
    {
        self.prepare_render();

        let (mut x, mut y) = (800f32, 20f32);

        for c in s.chars() {
            if let Some(glyph) = glyph_cache.get(&c) {
                self.render(glyph, x, y, color);
            }

            x += cell_width as f32 + 2f32;
        }

        self.finish_render();
    }

    pub fn render_cursor(&mut self,
                         cursor: term::Cursor,
                         glyph_cache: &GlyphCache,
                         props: &TermProps)
    {
        self.prepare_render();
        if let Some(glyph) = glyph_cache.get(&term::CURSOR_SHAPE) {
            let y = (props.cell_height + props.sep_y) * (cursor.y as f32);
            let x = (props.cell_width + props.sep_x) * (cursor.x as f32);

            let y_inverted = props.height - y - props.cell_height;

            self.render(glyph, x, y_inverted, &term::DEFAULT_FG);
        }

        self.finish_render();
    }

    pub fn render_grid(&mut self, grid: &Grid, glyph_cache: &GlyphCache, props: &TermProps) {
        self.prepare_render();
        for i in 0..grid.rows() {
            let row = &grid[i];
            for j in 0..row.cols() {
                let cell = &row[j];
                if cell.c != ' ' {
                    if let Some(glyph) = glyph_cache.get(&cell.c) {
                        let y = (props.cell_height + props.sep_y) * (i as f32);
                        let x = (props.cell_width + props.sep_x) * (j as f32);

                        let y_inverted = (props.height) - y - (props.cell_height);

                        self.render(glyph, x, y_inverted, &cell.fg);
                    }
                }
            }
        }
        self.finish_render();
    }

    fn prepare_render(&self) {
        unsafe {
            self.program.activate();

            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.ebo);
            gl::ActiveTexture(gl::TEXTURE0);
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

    fn render(&mut self, glyph: &Glyph, x: f32, y: f32, color: &Rgb) {
        if &self.active_color != color {
            unsafe {
                gl::Uniform3i(self.program.u_color,
                              color.r as i32,
                              color.g as i32,
                              color.b as i32);
            }
            self.active_color = color.to_owned();
        }

        let rect = get_rect(glyph, x, y);

        let uv = glyph.uv;

        // Top right, Bottom right, Bottom left, Top left
        let packed = [
            PackedVertex { x: rect.max_x(), y: rect.max_y(), u: uv.max_x(), v: uv.min_y(), },
            PackedVertex { x: rect.max_x(), y: rect.min_y(), u: uv.max_x(), v: uv.max_y(), },
            PackedVertex { x: rect.min_x(), y: rect.min_y(), u: uv.min_x(), v: uv.max_y(), },
            PackedVertex { x: rect.min_x(), y: rect.max_y(), u: uv.min_x(), v: uv.min_y(), },
        ];

        unsafe {
            // Bind texture if it changed
            if self.active_tex != glyph.tex_id {
                gl::BindTexture(gl::TEXTURE_2D, glyph.tex_id);
                self.active_tex = glyph.tex_id;
            }

            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                (packed.len() * size_of::<PackedVertex>()) as isize,
                packed.as_ptr() as *const _
            );

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
    id: GLuint,
    /// projection matrix uniform
    u_projection: GLint,
    /// color uniform
    u_color: GLint,
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

    pub fn new(width: u32, height: u32) -> ShaderProgram {
        let vertex_shader = ShaderProgram::create_vertex_shader();
        let fragment_shader = ShaderProgram::create_fragment_shader();
        let program = ShaderProgram::create_program(vertex_shader, fragment_shader);

        unsafe {
            gl::DeleteShader(vertex_shader);
            gl::DeleteShader(fragment_shader);
        }

        // get uniform locations
        let projection_str = CString::new("projection").unwrap();
        let color_str = CString::new("textColor").unwrap();

        let (projection, color) = unsafe {
            (
                gl::GetUniformLocation(program, projection_str.as_ptr()),
                gl::GetUniformLocation(program, color_str.as_ptr()),
            )
        };

        assert!(projection != gl::INVALID_VALUE as i32);
        assert!(projection != gl::INVALID_OPERATION as i32);
        assert!(color != gl::INVALID_VALUE as i32);
        assert!(color != gl::INVALID_OPERATION as i32);

        // Initialize to known color (black)
        unsafe {
            gl::Uniform3i(color, 0, 0, 0);
        }

        let shader = ShaderProgram {
            id: program,
            u_projection: projection,
            u_color: color,
        };

        // set projection uniform
        let ortho = cgmath::ortho(0., width as f32, 0., height as f32, -1., 1.);
        let projection: [[f32; 4]; 4] = ortho.into();

        println!("width: {}, height: {}", width, height);

        shader.activate();
        unsafe {
            gl::UniformMatrix4fv(shader.u_projection,
                                 1, gl::FALSE, projection.as_ptr() as *const _);
        }
        shader.deactivate();

        shader
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

    fn create_fragment_shader() -> GLuint {
        unsafe {
            let fragment_shader = gl::CreateShader(gl::FRAGMENT_SHADER);
            let fragment_source = CString::new(TEXT_SHADER_F).unwrap();
            gl::ShaderSource(fragment_shader, 1, &fragment_source.as_ptr(), ptr::null());
            gl::CompileShader(fragment_shader);

            let mut success: GLint = 0;
            gl::GetShaderiv(fragment_shader, gl::COMPILE_STATUS, &mut success);

            if success != (gl::TRUE as GLint) {
                panic!("failed to compiler fragment shader");
            }
            fragment_shader
        }
    }

    fn create_vertex_shader() -> GLuint {
        unsafe {
            let vertex_shader = gl::CreateShader(gl::VERTEX_SHADER);
            let vertex_source = CString::new(TEXT_SHADER_V).unwrap();
            gl::ShaderSource(vertex_shader, 1, &vertex_source.as_ptr(), ptr::null());
            gl::CompileShader(vertex_shader);

            let mut success: GLint = 0;
            gl::GetShaderiv(vertex_shader, gl::COMPILE_STATUS, &mut success);

            if success != (gl::TRUE as GLint) {
                panic!("failed to compiler vertex shader");
            }
            vertex_shader
        }
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

        let uv = Rect::new(
            Point2D::new(uv_left, uv_bot),
            Size2D::new(uv_width, uv_height)
        );

        // Return the glyph
        Glyph {
            tex_id: self.id,
            top: glyph.top as i32,
            width: width,
            height: height,
            left: glyph.left as i32,
            uv: uv
        }
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

pub struct Glyph {
    tex_id: GLuint,
    top: i32,
    left: i32,
    width: i32,
    height: i32,
    uv: Rect<f32>,
}
