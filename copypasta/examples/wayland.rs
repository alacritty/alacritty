#[cfg(any(not(unix), target_os = "macos", target_os = "android", target_os = "emscripten"))]
fn main() {
    unimplemented!()
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
fn main() {
    wayland::main();
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
mod wayland {
    extern crate andrew;
    extern crate copypasta;
    extern crate smithay_client_toolkit as sctk;

    use wayland::copypasta::wayland_clipboard::create_clipboards;
    use wayland::copypasta::ClipboardProvider;

    use std::io::{Read, Seek, SeekFrom, Write};
    use std::sync::{atomic, Arc, Mutex};

    use wayland::sctk::keyboard::{map_keyboard_auto, Event as KbEvent, KeyState};
    use wayland::sctk::utils::{DoubleMemPool, MemPool};
    use wayland::sctk::window::{ConceptFrame, Event as WEvent, Window};
    use wayland::sctk::Environment;

    use wayland::sctk::reexports::client::protocol::{wl_shm, wl_surface};
    use wayland::sctk::reexports::client::{Display, NewProxy};

    use wayland::andrew::shapes::rectangle;
    use wayland::andrew::text;
    use wayland::andrew::text::fontconfig;

    pub fn main() {
        let (display, mut event_queue) =
            Display::connect_to_env().expect("Failed to connect to the wayland server.");
        let env = Environment::from_display(&*display, &mut event_queue).unwrap();

        let (mut ctx, _) = create_clipboards(&display);
        let cb_contents = Arc::new(Mutex::new(String::new()));

        let seat = env.manager.instantiate_range(2, 6, NewProxy::implement_dummy).unwrap();

        let need_redraw = Arc::new(atomic::AtomicBool::new(false));
        let need_redraw_clone = need_redraw.clone();
        let cb_contents_clone = cb_contents.clone();
        map_keyboard_auto(&seat, move |event: KbEvent, _| {
            if let KbEvent::Key { state: KeyState::Pressed, utf8: Some(text), .. } = event {
                if text == " " {
                    *cb_contents_clone.lock().unwrap() = ctx.get_contents().unwrap();
                    need_redraw_clone.store(true, atomic::Ordering::Relaxed)
                } else if text == "s" {
                    ctx.set_contents(
                        "This is an example text thats been copied to the wayland clipboard :)"
                            .to_string(),
                    )
                    .unwrap();
                } else if text == "t" {
                    ctx.set_contents("Alternative text :)".to_string()).unwrap();
                }
            }
        })
        .unwrap();

        let mut dimensions = (320u32, 240u32);
        let surface = env.compositor.create_surface(NewProxy::implement_dummy).unwrap();

        let next_action = Arc::new(Mutex::new(None::<WEvent>));

        let waction = next_action.clone();
        let mut window =
            Window::<ConceptFrame>::init_from_env(&env, surface, dimensions, move |evt| {
                let mut next_action = waction.lock().unwrap();
                // Keep last event in priority order : Close > Configure > Refresh
                let replace = match (&evt, &*next_action) {
                    (_, &None)
                    | (_, &Some(WEvent::Refresh))
                    | (&WEvent::Configure { .. }, &Some(WEvent::Configure { .. }))
                    | (&WEvent::Close, _) => true,
                    _ => false,
                };
                if replace {
                    *next_action = Some(evt);
                }
            })
            .expect("Failed to create a window !");

        window.new_seat(&seat);
        window.set_title("Clipboard".to_string());

        let mut pools =
            DoubleMemPool::new(&env.shm, || {}).expect("Failed to create a memory pool !");

        let mut font_data = Vec::new();
        std::fs::File::open(
            &fontconfig::FontConfig::new().unwrap().get_regular_family_fonts("sans").unwrap()[0],
        )
        .unwrap()
        .read_to_end(&mut font_data)
        .unwrap();

        if !env.shell.needs_configure() {
            // initial draw to bootstrap on wl_shell
            if let Some(pool) = pools.pool() {
                redraw(pool, window.surface(), dimensions, &font_data, "".to_string());
            }
            window.refresh();
        }

        loop {
            match next_action.lock().unwrap().take() {
                Some(WEvent::Close) => break,
                Some(WEvent::Refresh) => {
                    window.refresh();
                    window.surface().commit();
                },
                Some(WEvent::Configure { new_size, .. }) => {
                    if let Some((w, h)) = new_size {
                        window.resize(w, h);
                        dimensions = (w, h)
                    }
                    window.refresh();
                    if let Some(pool) = pools.pool() {
                        redraw(
                            pool,
                            window.surface(),
                            dimensions,
                            &font_data,
                            cb_contents.lock().unwrap().clone(),
                        );
                    }
                },
                None => {},
            }

            if need_redraw.swap(false, atomic::Ordering::Relaxed) {
                if let Some(pool) = pools.pool() {
                    redraw(
                        pool,
                        window.surface(),
                        dimensions,
                        &font_data,
                        cb_contents.lock().unwrap().clone(),
                    );
                }
                window.surface().damage_buffer(0, 0, dimensions.0 as i32, dimensions.1 as i32);
                window.surface().commit();
            }

            event_queue.dispatch().unwrap();
        }
    }

    fn redraw(
        pool: &mut MemPool,
        surface: &wl_surface::WlSurface,
        dimensions: (u32, u32),
        font_data: &[u8],
        cb_contents: String,
    ) {
        let (buf_x, buf_y) = (dimensions.0 as usize, dimensions.1 as usize);

        pool.resize(4 * buf_x * buf_y).expect("Failed to resize the memory pool.");

        let mut buf = vec![0; 4 * buf_x * buf_y];
        let mut canvas =
            andrew::Canvas::new(&mut buf, buf_x, buf_y, 4 * buf_x, andrew::Endian::native());

        let bg = rectangle::Rectangle::new((0, 0), (buf_x, buf_y), None, Some([255, 170, 20, 45]));
        canvas.draw(&bg);

        let text_box = rectangle::Rectangle::new(
            (buf_x / 30, buf_y / 35),
            (buf_x - 2 * (buf_x / 30), (buf_x as f32 / 14.) as usize),
            Some((3, [255, 255, 255, 255], rectangle::Sides::ALL, Some(4))),
            None,
        );
        canvas.draw(&text_box);

        let helper_text = text::Text::new(
            (buf_x / 25, buf_y / 30),
            [255, 255, 255, 255],
            font_data,
            buf_x as f32 / 40.,
            2.0,
            "Press space to draw clipboard contents",
        );
        canvas.draw(&helper_text);

        let helper_text = text::Text::new(
            (buf_x / 25, buf_y / 15),
            [255, 255, 255, 255],
            font_data,
            buf_x as f32 / 40.,
            2.0,
            "Press 's' to store example text to clipboard",
        );
        canvas.draw(&helper_text);

        for i in (0..cb_contents.len()).step_by(36) {
            let content = if cb_contents.len() < i + 36 {
                cb_contents[i..].to_string()
            } else {
                cb_contents[i..i + 36].to_string()
            };
            let text = text::Text::new(
                (buf_x / 10, buf_y / 8 + (i as f32 * buf_y as f32 / 1000.) as usize),
                [255, 255, 255, 255],
                font_data,
                buf_x as f32 / 40.,
                2.0,
                content,
            );
            canvas.draw(&text);
        }

        pool.seek(SeekFrom::Start(0)).unwrap();
        pool.write_all(canvas.buffer).unwrap();
        pool.flush().unwrap();

        let new_buffer =
            pool.buffer(0, buf_x as i32, buf_y as i32, 4 * buf_x as i32, wl_shm::Format::Argb8888);
        surface.attach(Some(&new_buffer), 0, 0);
        surface.commit();
    }
}
