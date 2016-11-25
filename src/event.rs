//! Process window events
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, mpsc};
use serde_json as json;

use glutin;

use input;
use sync::FairMutex;
use term::Term;
use config::Config;

/// The event processor
pub struct Processor<N> {
    notifier: N,
    input_processor: input::Processor,
    terminal: Arc<FairMutex<Term>>,
    resize_tx: mpsc::Sender<(u32, u32)>,
    ref_test: bool,
}

impl<N: input::Notify> Processor<N> {
    /// Create a new event processor
    ///
    /// Takes a writer which is expected to be hooked up to the write end of a
    /// pty.
    pub fn new(
        notifier: N,
        terminal: Arc<FairMutex<Term>>,
        resize_tx: mpsc::Sender<(u32, u32)>,
        config: &Config,
        ref_test: bool,
    ) -> Processor<N> {
        Processor {
            notifier: notifier,
            terminal: terminal,
            input_processor: input::Processor::new(config),
            resize_tx: resize_tx,
            ref_test: ref_test,
        }
    }

    fn handle_event(&mut self, event: glutin::Event) {
        match event {
            glutin::Event::Closed => {
                if self.ref_test {
                    // dump grid state
                    let terminal = self.terminal.lock();
                    let grid = terminal.grid();

                    let serialized_grid = json::to_string(&grid)
                        .expect("serialize grid");

                    let serialized_size = json::to_string(terminal.size_info())
                        .expect("serialize size");

                    File::create("./grid.json")
                        .and_then(|mut f| f.write_all(serialized_grid.as_bytes()))
                        .expect("write grid.json");

                    File::create("./size.json")
                        .and_then(|mut f| f.write_all(serialized_size.as_bytes()))
                        .expect("write size.json");
                }

                // FIXME
                panic!("window closed");
            },
            glutin::Event::Resized(w, h) => {
                self.resize_tx.send((w, h)).expect("send new size");
                // Acquire term lock
                let mut terminal = self.terminal.lock();
                terminal.dirty = true;
            },
            glutin::Event::KeyboardInput(state, _code, key, mods, string) => {
                // Acquire term lock
                let terminal = self.terminal.lock();
                let processor = &mut self.input_processor;
                let notifier = &mut self.notifier;

                processor.process_key(state, key, mods, notifier, *terminal.mode(), string);
            },
            glutin::Event::MouseInput(state, button) => {
                let terminal = self.terminal.lock();
                let processor = &mut self.input_processor;
                let notifier = &mut self.notifier;

                processor.mouse_input(state, button, notifier, &terminal);
            },
            glutin::Event::MouseMoved(x, y) => {
                if x > 0 && y > 0 {
                    self.input_processor.mouse_moved(x as u32, y as u32);
                }
            },
            glutin::Event::Focused(true) => {
                let mut terminal = self.terminal.lock();
                terminal.dirty = true;
            },
            glutin::Event::MouseWheel(scroll_delta, touch_phase) => {
                let terminal = self.terminal.lock();
                let processor = &mut self.input_processor;
                let notifier = &mut self.notifier;

                processor.on_mouse_wheel(
                    notifier,
                    scroll_delta,
                    touch_phase,
                    &terminal
                );
            },
            _ => (),
        }
    }

    /// Process at least one event and handle any additional queued events.
    pub fn process_events(&mut self, window: &glutin::Window) {
        for event in window.wait_events() {
            self.handle_event(event);
            break;
        }

        for event in window.poll_events() {
            self.handle_event(event);
        }
    }

    pub fn update_config(&mut self, config: &Config) {
        self.input_processor.update_config(config);
    }
}
