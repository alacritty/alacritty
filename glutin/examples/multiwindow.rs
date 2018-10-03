extern crate glutin;

mod support;

use glutin::GlContext;

fn main() {
    let mut events_loop = glutin::EventsLoop::new();

    let mut windows = std::collections::HashMap::new();
    for index in 0..3 {
        let title = format!("Charming Window #{}", index + 1);
        let window = glutin::WindowBuilder::new().with_title(title);
        let context = glutin::ContextBuilder::new();
        let gl_window = glutin::GlWindow::new(window, context, &events_loop).unwrap();
        let _ = unsafe { gl_window.make_current() };
        let gl = support::load(&gl_window.context());
        let window_id = gl_window.id();
        windows.insert(window_id, (gl_window, gl));
    }

    while !windows.is_empty() {
        events_loop.poll_events(|event| {
            println!("{:?}", event);
            match event {
                glutin::Event::WindowEvent { event, window_id } => match event {
                    glutin::WindowEvent::Resized(logical_size) => {
                        let gl_window = &windows[&window_id].0;
                        let dpi_factor = gl_window.get_hidpi_factor();
                        gl_window.resize(logical_size.to_physical(dpi_factor));
                    },
                    glutin::WindowEvent::CloseRequested => {
                        if windows.remove(&window_id).is_some() {
                            println!("Window with ID {:?} has been closed", window_id);
                        }
                    },
                    _ => (),
                },
                _ => (),
            }
        });

        for (index, window) in windows.values().enumerate() {
            let mut color = [1.0, 0.5, 0.7, 1.0];
            color.swap(0, index % 3);
            let _ = unsafe { window.0.make_current() };
            window.1.draw_frame(color);
            let _ = window.0.swap_buffers();
        }
    }
}
