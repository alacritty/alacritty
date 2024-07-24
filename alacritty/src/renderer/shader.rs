use std::ffi::CStr;
use std::fmt;

use crate::gl;
use crate::gl::types::*;

/// A wrapper for a shader program id, with automatic lifetime management.
#[derive(Debug)]
pub struct ShaderProgram(GLuint);

#[derive(Copy, Clone, Debug)]
pub enum ShaderVersion {
    /// OpenGL 3.3 core shaders.
    Glsl3,

    /// OpenGL ES 2.0 shaders.
    Gles2,
}

impl ShaderVersion {
    // Header to which we concatenate the entire shader. The newlines are required.
    fn shader_header(&self) -> &'static str {
        match self {
            Self::Glsl3 => "#version 330 core\n",
            Self::Gles2 => "#version 100\n#define GLES2_RENDERER\n",
        }
    }
}

impl ShaderProgram {
    pub fn new(
        shader_version: ShaderVersion,
        shader_header: Option<&str>,
        vertex_shader: &'static str,
        fragment_shader: &'static str,
    ) -> Result<Self, ShaderError> {
        let vertex_shader =
            Shader::new(shader_version, shader_header, gl::VERTEX_SHADER, vertex_shader)?;
        let fragment_shader =
            Shader::new(shader_version, shader_header, gl::FRAGMENT_SHADER, fragment_shader)?;

        let program = unsafe { Self(gl::CreateProgram()) };

        let mut success: GLint = 0;
        unsafe {
            gl::AttachShader(program.id(), vertex_shader.id());
            gl::AttachShader(program.id(), fragment_shader.id());
            gl::LinkProgram(program.id());
            gl::GetProgramiv(program.id(), gl::LINK_STATUS, &mut success);
        }

        if success != i32::from(gl::TRUE) {
            return Err(ShaderError::Link(get_program_info_log(program.id())));
        }

        Ok(program)
    }

    /// Get uniform location by name. Panic if failed.
    pub fn get_uniform_location(&self, name: &'static CStr) -> Result<GLint, ShaderError> {
        // This call doesn't require `UseProgram`.
        let ret = unsafe { gl::GetUniformLocation(self.id(), name.as_ptr()) };
        if ret == -1 {
            return Err(ShaderError::Uniform(name));
        }
        Ok(ret)
    }

    /// Get the shader program id.
    pub fn id(&self) -> GLuint {
        self.0
    }
}

impl Drop for ShaderProgram {
    fn drop(&mut self) {
        unsafe { gl::DeleteProgram(self.0) }
    }
}

/// A wrapper for a shader id, with automatic lifetime management.
#[derive(Debug)]
struct Shader(GLuint);

impl Shader {
    fn new(
        shader_version: ShaderVersion,
        shader_header: Option<&str>,
        kind: GLenum,
        source: &'static str,
    ) -> Result<Self, ShaderError> {
        let version_header = shader_version.shader_header();
        let mut sources = Vec::<*const GLchar>::with_capacity(3);
        let mut lengths = Vec::<GLint>::with_capacity(3);
        sources.push(version_header.as_ptr().cast());
        lengths.push(version_header.len() as GLint);

        if let Some(shader_header) = shader_header {
            sources.push(shader_header.as_ptr().cast());
            lengths.push(shader_header.len() as GLint);
        }

        sources.push(source.as_ptr().cast());
        lengths.push(source.len() as GLint);

        let shader = unsafe { Self(gl::CreateShader(kind)) };

        let mut success: GLint = 0;
        unsafe {
            gl::ShaderSource(
                shader.id(),
                lengths.len() as GLint,
                sources.as_ptr().cast(),
                lengths.as_ptr(),
            );
            gl::CompileShader(shader.id());
            gl::GetShaderiv(shader.id(), gl::COMPILE_STATUS, &mut success);
        }

        if success == GLint::from(gl::TRUE) {
            Ok(shader)
        } else {
            Err(ShaderError::Compile(get_shader_info_log(shader.id())))
        }
    }

    fn id(&self) -> GLuint {
        self.0
    }
}

impl Drop for Shader {
    fn drop(&mut self) {
        unsafe { gl::DeleteShader(self.0) }
    }
}

fn get_program_info_log(program: GLuint) -> String {
    // Get expected log length.
    let mut max_length: GLint = 0;
    unsafe {
        gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut max_length);
    }

    // Read the info log.
    let mut actual_length: GLint = 0;
    let mut buf: Vec<u8> = Vec::with_capacity(max_length as usize);
    unsafe {
        gl::GetProgramInfoLog(program, max_length, &mut actual_length, buf.as_mut_ptr() as *mut _);
    }

    // Build a string.
    unsafe {
        buf.set_len(actual_length as usize);
    }

    String::from_utf8_lossy(&buf).to_string()
}

fn get_shader_info_log(shader: GLuint) -> String {
    // Get expected log length.
    let mut max_length: GLint = 0;
    unsafe {
        gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut max_length);
    }

    // Read the info log.
    let mut actual_length: GLint = 0;
    let mut buf: Vec<u8> = Vec::with_capacity(max_length as usize);
    unsafe {
        gl::GetShaderInfoLog(shader, max_length, &mut actual_length, buf.as_mut_ptr() as *mut _);
    }

    // Build a string.
    unsafe {
        buf.set_len(actual_length as usize);
    }

    String::from_utf8_lossy(&buf).to_string()
}

#[derive(Debug)]
pub enum ShaderError {
    /// Error compiling shader.
    Compile(String),

    /// Error linking shader.
    Link(String),

    /// Error getting uniform location.
    Uniform(&'static CStr),
}

impl std::error::Error for ShaderError {}

impl fmt::Display for ShaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile(reason) => write!(f, "Failed compiling shader: {reason}"),
            Self::Link(reason) => write!(f, "Failed linking shader: {reason}"),
            Self::Uniform(name) => write!(f, "Failed to get uniform location of {name:?}"),
        }
    }
}
