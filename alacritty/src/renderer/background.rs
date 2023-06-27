use std::path::PathBuf;

use alacritty_terminal::term::color::Rgb;
use image::GenericImageView;

use crate::{
    display::SizeInfo,
    gl::{
        self,
        types::{GLint, GLsizei, GLuint},
    },
    renderer::cstr,
};

#[derive(Debug)]
pub struct BackgroundRenderer {
    vao: GLuint,
    vbo: GLuint,
    program: u32,
    width: f32,
    height: f32,
    m0_location: i32,
    m1_location: i32,
    intensity_location: i32,

    images: Vec<Image>,
    intensity: f32,
    current_image_index: u32,
    time: std::time::SystemTime,
}

#[derive(Debug)]
struct Image {
    texture: u32,
    image_width: u32,
    image_height: u32,
}

static VERTEX_SHADER_SOURCE: &str = include_str!("../../res/background.v.glsl");
static FRAGMENT_SHADER_SOURCE: &str = include_str!("../../res/background.f.glsl");

impl BackgroundRenderer {
    pub fn new(paths: &[PathBuf], intensity: f32) -> Self {
        // Shader

        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;
        let m0_name = cstr!("u_Matrix0");
        let m1_name = cstr!("u_Matrix1");
        let intensity_name = cstr!("u_Intensity");

        let vertex_shader_sources = [VERTEX_SHADER_SOURCE.as_ptr().cast()];
        let fragment_shader_sources = [FRAGMENT_SHADER_SOURCE.as_ptr().cast()];
        let vertex_shader_len = [VERTEX_SHADER_SOURCE.len() as i32];
        let fragment_shader_len = [FRAGMENT_SHADER_SOURCE.len() as i32];
        unsafe {
            // 頂点シェーダー
            let vertex_shader = gl::CreateShader(gl::VERTEX_SHADER);
            gl::ShaderSource(
                vertex_shader,
                1,
                vertex_shader_sources.as_ptr(),
                vertex_shader_len.as_ptr(),
            );
            gl::CompileShader(vertex_shader);

            let mut success = 0;
            gl::GetShaderiv(vertex_shader, gl::COMPILE_STATUS, &mut success);
            if success != GLint::from(gl::TRUE) {
                // Get expected log length.
                let mut max_length: GLint = 0;
                gl::GetShaderiv(vertex_shader, gl::INFO_LOG_LENGTH, &mut max_length);

                // Read the info log.
                let mut actual_length: GLint = 0;
                let mut buf: Vec<u8> = Vec::with_capacity(max_length as usize);
                gl::GetShaderInfoLog(
                    vertex_shader,
                    max_length,
                    &mut actual_length,
                    buf.as_mut_ptr() as *mut _,
                );

                // Build a string.
                buf.set_len(actual_length as usize);

                println!("{}", String::from_utf8_lossy(&buf).to_string());

                panic!();
            }

            let fragment_shader = gl::CreateShader(gl::FRAGMENT_SHADER);
            gl::ShaderSource(
                fragment_shader,
                1,
                fragment_shader_sources.as_ptr(),
                fragment_shader_len.as_ptr(),
            );
            gl::CompileShader(fragment_shader);
            gl::GetShaderiv(fragment_shader, gl::COMPILE_STATUS, &mut success);
            if success != GLint::from(gl::TRUE) {
                // Get expected log length.
                let mut max_length: GLint = 0;
                gl::GetShaderiv(vertex_shader, gl::INFO_LOG_LENGTH, &mut max_length);

                // Read the info log.
                let mut actual_length: GLint = 0;
                let mut buf: Vec<u8> = Vec::with_capacity(max_length as usize);
                gl::GetShaderInfoLog(
                    vertex_shader,
                    max_length,
                    &mut actual_length,
                    buf.as_mut_ptr() as *mut _,
                );

                // Build a string.
                buf.set_len(actual_length as usize);

                println!("{}", String::from_utf8_lossy(&buf).to_string());

                panic!();
            }

            let program = gl::CreateProgram();
            gl::AttachShader(program, vertex_shader);
            gl::AttachShader(program, fragment_shader);
            gl::LinkProgram(program);
            gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);
            if success != GLint::from(gl::TRUE) {
                let mut max_length: GLint = 0;
                gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut max_length);

                // Read the info log.
                let mut actual_length: GLint = 0;
                let mut buf: Vec<u8> = Vec::with_capacity(max_length as usize);
                gl::GetProgramInfoLog(
                    program,
                    max_length,
                    &mut actual_length,
                    buf.as_mut_ptr() as *mut _,
                );

                // Build a string.
                buf.set_len(actual_length as usize);

                println!("{}", String::from_utf8_lossy(&buf).to_string());
                panic!();
            }

            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);
            {
                gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

                // Position
                gl::VertexAttribPointer(
                    0,                                       /*index */
                    2,                                       /*size*/
                    gl::FLOAT,                               /*type_*/
                    gl::FALSE,                               /*normalized*/
                    (std::mem::size_of::<f32>() * 2) as i32, /*stride*/
                    0 as *const _,                           /*offset*/
                );
                gl::EnableVertexAttribArray(0);

                gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            }
            let images =
                paths.iter().map(|path| Self::create_texture(path)).collect::<Vec<Image>>();
            gl::BindVertexArray(0);

            let m0_location = gl::GetUniformLocation(program, m0_name.as_ptr());
            let m1_location = gl::GetUniformLocation(program, m1_name.as_ptr());
            let intensity_location = gl::GetUniformLocation(program, intensity_name.as_ptr());

            Self {
                vao,
                vbo,
                program,
                width: 1280.0,
                height: 960.0,
                m0_location,
                m1_location,
                intensity_location,
                images,
                intensity,
                current_image_index: 0,
                time: std::time::SystemTime::now(),
            }
        }
    }

    pub fn resize(&mut self, size_info: &SizeInfo) {
        self.width = size_info.width();
        self.height = size_info.height();
    }

    pub fn draw(&mut self, _color: Rgb, alpha: f32) {
        let now = std::time::SystemTime::now();
        let diff = now.duration_since(self.time);
        if 1000 < diff.unwrap().as_millis() {
            self.current_image_index = (self.current_image_index + 1) % self.images.len() as u32;
            self.time = now;
        }

        unsafe {
            gl::ClearColor(
                // color.r as f32 / 255.0,
                // color.g as f32 / 255.0,
                // color.b as f32 / 255.0,
                0.0, 0.0, 0.0, alpha,
            );
            gl::Clear(gl::COLOR_BUFFER_BIT);

            // 背景画像が設定されてなかったらなにもしない
            if self.images.is_empty() {
                return;
            }

            gl::Viewport(0, 0, self.width as GLint, self.height as GLint);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);

            gl::UseProgram(self.program);

            let vertices = [
                -1.0f32, -1.0, // v0
                1.0, -1.0, // v1
                -1.0, 1.0, // v2
                -1.0, 1.0, // v3
                1.0, -1.0, // v4
                1.0, 1.0, // v5
            ];
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * std::mem::size_of::<f32>()) as isize,
                vertices.as_ptr() as *const _,
                gl::STREAM_DRAW,
            );

            // テクスチャ
            let indicies = [self.current_image_index];
            for index in indicies {
                let image = &self.images[index as usize];
                gl::BindTexture(gl::TEXTURE_2D, image.texture);

                // 画像の UV 変換
                let scale_x = self.width / image.image_width as f32;
                let scale_y = self.height / image.image_height as f32;

                // (1, 1) より外を参照してたらフィットするよう補正
                // [0, 1] だったら補正は不要なので 1 で抑えておく
                let factor = scale_x.max(scale_y).max(1.0);
                let x = scale_x / factor;
                let y = scale_y / factor;

                // 画像の中心とターミナルの中心が一致するように並行移動
                let t_x = 0.5 * (image.image_width as f32 - self.width).max(0.0) / self.width;
                let t_y = 0.5 * (image.image_height as f32 - self.height).max(0.0) / self.height;

                // 並行移動してからスケール
                gl::Uniform3f(self.m0_location, x, 0.0, x * t_x);
                gl::Uniform3f(self.m1_location, 0.0, y, y * t_y);
                gl::Uniform1f(self.intensity_location, self.intensity);

                gl::DrawArrays(gl::TRIANGLES, 0, 6 as GLsizei);
            }

            gl::UseProgram(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            // // Reset blending strategy.
            gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
    }

    fn create_texture(path: &PathBuf) -> Image {
        let img = match
            // image::open("/Users/shuto/.config/wezterm/suntry_nomu.JPG")
            image::open(path)
            {
            Ok(content) => content,
            Err(_err) => panic!(),
        };
        let mut texture = 0;
        unsafe {
            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_BORDER as i32); // set texture wrapping to gl::REPEAT (default wrapping method)
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_BORDER as i32);
            // set texture filtering parameters
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);

            let data = img.to_rgba().into_raw();
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as i32,
                img.width() as i32,
                img.height() as i32,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                &data[0] as *const u8 as *const _,
            );
            gl::GenerateMipmap(gl::TEXTURE_2D);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
        Image { texture, image_width: img.width(), image_height: img.height() }
    }
}

impl Drop for BackgroundRenderer {
    fn drop(&mut self) {
        unsafe {
            for image in &self.images {
                gl::DeleteTextures(1, &image.texture);
            }
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}
