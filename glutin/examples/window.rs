extern crate glutin;

mod support;

use glutin::GlContext;

fn main() {
    let mut events_loop = glutin::EventsLoop::new();
    let window = glutin::WindowBuilder::new().with_title("A fantastic window!");
    let context = glutin::ContextBuilder::new();
    let gl_window = glutin::GlWindow::new(window, context, &events_loop).unwrap();

    let _ = unsafe { gl_window.make_current() };

    println!("Pixel format of the window's GL context: {:?}", gl_window.get_pixel_format());

    let gl = support::load(&gl_window.context());

    let mut running = true;
    while running {
        events_loop.poll_events(|event| {
            println!("{:?}", event);
            match event {
                glutin::Event::WindowEvent { event, .. } => match event {
                    glutin::WindowEvent::CloseRequested => running = false,
                    glutin::WindowEvent::Resized(logical_size) => {
                        let dpi_factor = gl_window.get_hidpi_factor();
                        gl_window.resize(logical_size.to_physical(dpi_factor));
                    },
                    _ => (),
                },
                _ => ()
            }
        });

        gl.draw_frame([1.0, 0.5, 0.7, 1.0]);
        let _ = gl_window.swap_buffers();
    }
}
