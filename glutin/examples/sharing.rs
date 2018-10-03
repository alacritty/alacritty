extern crate glutin;

mod support;

use support::gl;
use glutin::GlContext;

fn main() {
    let mut events_loop = glutin::EventsLoop::new();
    let mut size = glutin::dpi::PhysicalSize::new(768., 480.);

    let context = glutin::ContextBuilder::new();
    let gl_context = glutin::Context::new(&events_loop, context, true).unwrap();

    let window = glutin::WindowBuilder::new().with_title("A fantastic window!")
        .with_dimensions(glutin::dpi::LogicalSize::from_physical(size, 1.0));
    let context = glutin::ContextBuilder::new()
        .with_shared_lists(&gl_context);
    let gl_window = unsafe { glutin::GlWindow::new_shared(window, context, &events_loop).unwrap() };

    let _ = unsafe { gl_window.make_current() };
    println!("Pixel format of the window's GL context: {:?}", gl_window.get_pixel_format());
    let glw = support::load(&gl_window.context());

    let mut render_tex = 0;
    unsafe {
        glw.gl.GenTextures(1, &mut render_tex);
        glw.gl.BindTexture(gl::TEXTURE_2D, render_tex);
        glw.gl.TexStorage2D(
            gl::TEXTURE_2D,
            1,
            gl::SRGB8_ALPHA8,
            size.width as _,
            size.height as _,
        );
    }

    let mut window_fb = 0;
    unsafe {
        glw.gl.GenFramebuffers(1, &mut window_fb);
        glw.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, window_fb);
        glw.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        glw.gl.FramebufferTexture2D(
            gl::READ_FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            render_tex,
            0,
        );
    }

    let _ = unsafe { gl_context.make_current() };
    let glc = support::load(&gl_context);

    let mut context_fb = 0;
    unsafe {
        glc.gl.GenFramebuffers(1, &mut context_fb);
        glc.gl.BindFramebuffer(gl::FRAMEBUFFER, context_fb);
        glc.gl.FramebufferTexture2D(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            render_tex,
            0,
        );
        glc.gl.Viewport(0, 0, size.width as _, size.height as _);
    }

    let mut running = true;
    while running {
        events_loop.poll_events(|event| {
            println!("{:?}", event);
            match event {
                glutin::Event::WindowEvent { event, .. } => match event {
                    glutin::WindowEvent::CloseRequested => running = false,
                    glutin::WindowEvent::Resized(logical_size) => {
                        let _ = unsafe { gl_window.make_current() };
                        let dpi_factor = gl_window.get_hidpi_factor();
                        size = logical_size.to_physical(dpi_factor);
                        gl_window.resize(size);

                        unsafe {
                            let _ = gl_window.make_current();
                            glw.gl.DeleteTextures(1, &render_tex);
                            glw.gl.DeleteFramebuffers(1, &window_fb);

                            glw.gl.GenTextures(1, &mut render_tex);
                            glw.gl.BindTexture(gl::TEXTURE_2D, render_tex);
                            glw.gl.TexStorage2D(
                                gl::TEXTURE_2D,
                                1,
                                gl::SRGB8_ALPHA8,
                                size.width as _,
                                size.height as _,
                            );

                            glw.gl.GenFramebuffers(1, &mut window_fb);
                            glw.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, window_fb);
                            glw.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
                            glw.gl.FramebufferTexture2D(
                                gl::READ_FRAMEBUFFER,
                                gl::COLOR_ATTACHMENT0,
                                gl::TEXTURE_2D,
                                render_tex,
                                0,
                            );

                            let _ = gl_context.make_current();
                            glc.gl.DeleteFramebuffers(1, &context_fb);

                            glc.gl.GenFramebuffers(1, &mut context_fb);
                            glc.gl.BindFramebuffer(gl::FRAMEBUFFER, context_fb);
                            glc.gl.FramebufferTexture2D(
                                gl::FRAMEBUFFER,
                                gl::COLOR_ATTACHMENT0,
                                gl::TEXTURE_2D,
                                render_tex,
                                0,
                            );

                            glc.gl.Viewport(0, 0, size.width as _, size.height as _);
                        }
                    },
                    _ => (),
                },
                _ => ()
            }
        });

        let _ = unsafe { gl_context.make_current() };
        glc.draw_frame([1.0, 0.5, 0.7, 1.0]);

        let _ = unsafe { gl_window.make_current() };
        unsafe {
            glw.gl.BlitFramebuffer(
                0, 0, size.width as _, size.height as _,
                0, 0, size.width as _, size.height as _,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
        }
        let _ = gl_window.swap_buffers();
    }

    unsafe {
        let _ = gl_window.make_current();
        glw.gl.DeleteTextures(1, &render_tex);
        glw.gl.DeleteFramebuffers(1, &window_fb);
        let _ = gl_context.make_current();
        glc.gl.DeleteFramebuffers(1, &context_fb);
    }
}
