//! Process window events
use std::sync::{Arc, mpsc};

use glutin;

use input;
use sync::FairMutex;
use term::Term;
use util::encode_char;
use config::Config;

/// The event processor
pub struct Processor<N> {
    notifier: N,
    input_processor: input::Processor,
    terminal: Arc<FairMutex<Term>>,
    resize_tx: mpsc::Sender<(u32, u32)>,
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
    ) -> Processor<N> {
        Processor {
            notifier: notifier,
            terminal: terminal,
            input_processor: input::Processor::new(config),
            resize_tx: resize_tx,
        }
    }

    fn handle_event(&mut self, event: glutin::Event) {
        match event {
            glutin::Event::Closed => panic!("window closed"), // TODO ...
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

                processor.mouse_input(state, button, notifier, *terminal.mode());
            },
            glutin::Event::Focused(true) => {
                let mut terminal = self.terminal.lock();
                terminal.dirty = true;
            }
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
