use std::collections::HashSet;
use std::ffi::CStr;
use std::fmt;

use crossfont::Metrics;
use log::info;
use once_cell::sync::OnceCell;

use alacritty_terminal::index::Point;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Rgb;

use crate::display::content::RenderableCell;
use crate::display::SizeInfo;
use crate::gl;
use crate::renderer::rects::{RectRenderer, RenderRect};
use crate::renderer::shader::ShaderError;

pub mod rects;
mod shader;
mod text;

pub use text::{GlyphCache, LoaderApi};

use shader::ShaderVersion;
use text::{Gles2Renderer, Glsl3Renderer, TextRenderer};

macro_rules! cstr {
    ($s:literal) => {
        // This can be optimized into an no-op with pre-allocated NUL-terminated bytes.
        unsafe { std::ffi::CStr::from_ptr(concat!($s, "\0").as_ptr().cast()) }
    };
}
pub(crate) use cstr;

#[derive(Debug)]
pub enum Error {
    /// Shader error.
    Shader(ShaderError),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Shader(err) => err.source(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Shader(err) => {
                write!(f, "There was an error initializing the shaders: {}", err)
            },
        }
    }
}

impl From<ShaderError> for Error {
    fn from(val: ShaderError) -> Self {
        Error::Shader(val)
    }
}

#[derive(Debug)]
enum TextRendererProvider {
    Gles2(Gles2Renderer),
    Glsl3(Glsl3Renderer),
}

#[derive(Debug)]
pub struct Renderer {
    text_renderer: TextRendererProvider,
    rect_renderer: RectRenderer,
}

impl Renderer {
    /// Create a new renderer.
    ///
    /// This will automatically pick between the GLES2 and GLSL3 renderer based on the GPU's
    /// supported OpenGL version.
    pub fn new() -> Result<Self, Error> {
        let (version, renderer) = unsafe {
            let renderer = CStr::from_ptr(gl::GetString(gl::RENDERER) as *mut _);
            let version = CStr::from_ptr(gl::GetString(gl::SHADING_LANGUAGE_VERSION) as *mut _);
            (version.to_string_lossy(), renderer.to_string_lossy())
        };

        info!("Running on {}", renderer);

        let (text_renderer, rect_renderer) = if version.as_ref() >= "3.3" {
            let text_renderer = TextRendererProvider::Glsl3(Glsl3Renderer::new()?);
            let rect_renderer = RectRenderer::new(ShaderVersion::Glsl3)?;
            (text_renderer, rect_renderer)
        } else {
            let text_renderer = TextRendererProvider::Gles2(Gles2Renderer::new()?);
            let rect_renderer = RectRenderer::new(ShaderVersion::Gles2)?;
            (text_renderer, rect_renderer)
        };

        Ok(Self { text_renderer, rect_renderer })
    }

    pub fn draw_cells<I: Iterator<Item = RenderableCell>>(
        &mut self,
        size_info: &SizeInfo,
        glyph_cache: &mut GlyphCache,
        cells: I,
    ) {
        match &mut self.text_renderer {
            TextRendererProvider::Gles2(renderer) => {
                renderer.draw_cells(size_info, glyph_cache, cells)
            },
            TextRendererProvider::Glsl3(renderer) => {
                renderer.draw_cells(size_info, glyph_cache, cells)
            },
        }
    }

    /// Draw a string in a variable location. Used for printing the render timer, warnings and
    /// errors.
    pub fn draw_string(
        &mut self,
        point: Point<usize>,
        fg: Rgb,
        bg: Rgb,
        string_chars: impl Iterator<Item = char>,
        size_info: &SizeInfo,
        glyph_cache: &mut GlyphCache,
    ) {
        let cells = string_chars.enumerate().map(|(i, character)| RenderableCell {
            point: Point::new(point.line, point.column + i),
            character,
            extra: None,
            flags: Flags::empty(),
            bg_alpha: 1.0,
            fg,
            bg,
            underline: fg,
        });

        self.draw_cells(size_info, glyph_cache, cells);
    }

    pub fn with_loader<F, T>(&mut self, func: F) -> T
    where
        F: FnOnce(LoaderApi<'_>) -> T,
    {
        match &mut self.text_renderer {
            TextRendererProvider::Gles2(renderer) => renderer.with_loader(func),
            TextRendererProvider::Glsl3(renderer) => renderer.with_loader(func),
        }
    }

    /// Draw all rectangles simultaneously to prevent excessive program swaps.
    pub fn draw_rects(&mut self, size_info: &SizeInfo, metrics: &Metrics, rects: Vec<RenderRect>) {
        if rects.is_empty() {
            return;
        }

        // Prepare rect rendering state.
        unsafe {
            // Remove padding from viewport.
            gl::Viewport(0, 0, size_info.width() as i32, size_info.height() as i32);
            gl::BlendFuncSeparate(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA, gl::SRC_ALPHA, gl::ONE);
        }

        self.rect_renderer.draw(size_info, metrics, rects);

        // Activate regular state again.
        unsafe {
            // Reset blending strategy.
            gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);

            // Restore viewport with padding.
            self.set_viewport(size_info);
        }
    }

    /// Fill the window with `color` and `alpha`.
    pub fn clear(&self, color: Rgb, alpha: f32) {
        unsafe {
            gl::ClearColor(
                (f32::from(color.r) / 255.0).min(1.0) * alpha,
                (f32::from(color.g) / 255.0).min(1.0) * alpha,
                (f32::from(color.b) / 255.0).min(1.0) * alpha,
                alpha,
            );
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn finish(&self) {
        unsafe {
            gl::Finish();
        }
    }

    /// Set the viewport for cell rendering.
    #[inline]
    pub fn set_viewport(&self, size: &SizeInfo) {
        unsafe {
            gl::Viewport(
                size.padding_x() as i32,
                size.padding_y() as i32,
                size.width() as i32 - 2 * size.padding_x() as i32,
                size.height() as i32 - 2 * size.padding_y() as i32,
            );
        }
    }

    /// Resize the renderer.
    pub fn resize(&self, size_info: &SizeInfo) {
        self.set_viewport(size_info);
        match &self.text_renderer {
            TextRendererProvider::Gles2(renderer) => renderer.resize(size_info),
            TextRendererProvider::Glsl3(renderer) => renderer.resize(size_info),
        }
    }
}

struct GlExtensions;

impl GlExtensions {
    /// Check if the given `extension` is supported.
    ///
    /// This function will lazyly load OpenGL extensions.
    fn contains(extension: &str) -> bool {
        static OPENGL_EXTENSIONS: OnceCell<HashSet<&'static str>> = OnceCell::new();

        OPENGL_EXTENSIONS.get_or_init(Self::load_extensions).contains(extension)
    }

    /// Load available OpenGL extensions.
    fn load_extensions() -> HashSet<&'static str> {
        unsafe {
            let extensions = gl::GetString(gl::EXTENSIONS);

            if extensions.is_null() {
                let mut extensions_number = 0;
                gl::GetIntegerv(gl::NUM_EXTENSIONS, &mut extensions_number);

                (0..extensions_number as gl::types::GLuint)
                    .flat_map(|i| {
                        let extension = CStr::from_ptr(gl::GetStringi(gl::EXTENSIONS, i) as *mut _);
                        extension.to_str()
                    })
                    .collect()
            } else {
                match CStr::from_ptr(extensions as *mut _).to_str() {
                    Ok(ext) => ext.split_whitespace().collect(),
                    Err(_) => HashSet::new(),
                }
            }
        }
    }
}
