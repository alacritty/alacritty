//! Process window events
use std::io;
use std::sync::{Arc, mpsc};
use std;

use glutin;

use input;
use sync::FairMutex;
use term::Term;

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
        resize_tx: mpsc::Sender<(u32, u32)>
    ) -> Processor<N> {
        Processor {
            notifier: notifier,
            terminal: terminal,
            input_processor: input::Processor::new(),
            resize_tx: resize_tx,
        }
    }

    fn handle_event(&mut self, event: glutin::Event) {
        match event {
            glutin::Event::Closed => panic!("window closed"), // TODO ...
            glutin::Event::ReceivedCharacter(c) => {
                match c {
                    // Ignore BACKSPACE and DEL. These are handled specially.
                    '\u{8}' | '\u{7f}' => (),
                    // OSX arrow keys send invalid characters; ignore.
                    '\u{f700}' | '\u{f701}' | '\u{f702}' | '\u{f703}' => (),
                    _ => {
                        let encoded = c.encode_utf8();
                        self.notifier.notify(encoded.as_slice().to_vec());
                    }
                }
            },
            glutin::Event::Resized(w, h) => {
                self.resize_tx.send((w, h)).expect("send new size");
                // Acquire term lock
                let mut terminal = self.terminal.lock();
                terminal.dirty = true;
            },
            glutin::Event::KeyboardInput(state, _code, key, mods) => {
                // Acquire term lock
                let terminal = self.terminal.lock();
                let processor = &mut self.input_processor;
                let notifier = &mut self.notifier;

                processor.process(state, key, mods, notifier, *terminal.mode());
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
}
