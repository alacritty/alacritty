extern crate glutin;

mod support;

use glutin::GlContext;
use std::io::{self, Write};

fn main() {
    let mut events_loop = glutin::EventsLoop::new();

    // enumerating monitors
    let monitor = {
        for (num, monitor) in events_loop.get_available_monitors().enumerate() {
            println!("Monitor #{}: {:?}", num, monitor.get_name());
        }

        print!("Please write the number of the monitor to use: ");
        io::stdout().flush().unwrap();

        let mut num = String::new();
        io::stdin().read_line(&mut num).unwrap();
        let num = num.trim().parse().ok().expect("Please enter a number");
        let monitor = events_loop.get_available_monitors().nth(num).expect("Please enter a valid ID");

        println!("Using {:?}", monitor.get_name());

        monitor
    };

    let window = glutin::WindowBuilder::new()
        .with_title("Hello world!")
        .with_fullscreen(Some(monitor));
    let context = glutin::ContextBuilder::new();
    let gl_window = glutin::GlWindow::new(window, context, &events_loop).unwrap();

    let _ = unsafe { gl_window.make_current() };

    let gl = support::load(&gl_window.context());

    let mut fullscreen = true;
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
                    glutin::WindowEvent::KeyboardInput { input, .. } => {
                        match input.virtual_keycode {
                            Some(glutin::VirtualKeyCode::Escape) => running = false,
                            Some(glutin::VirtualKeyCode::F) => {
                                let monitor = if fullscreen {
                                    None
                                } else {
                                    Some(gl_window.get_current_monitor())
                                };
                                gl_window.set_fullscreen(monitor);
                                fullscreen = !fullscreen;
                            },
                            _ => (),
                        }
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
