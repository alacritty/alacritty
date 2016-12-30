//! Process window events
use std::borrow::Cow;
use std::fs::File;
use std::io::Write;
use std::sync::mpsc;

use serde_json as json;
use parking_lot::MutexGuard;
use glutin::{self, ElementState};

use config::Config;
use display::OnResize;
use index::{Line, Column, Side};
use input::{self, ActionContext, MouseBinding, KeyBinding};
use selection::Selection;
use sync::FairMutex;
use term::{Term, SizeInfo};
use util::limit;
use window::Window;

/// Byte sequences are sent to a `Notify` in response to some events
pub trait Notify {
    /// Notify that an escape sequence should be written to the pty
    ///
    /// TODO this needs to be able to error somehow
    fn notify<B: Into<Cow<'static, [u8]>>>(&mut self, B);
}

/// State of the mouse
pub struct Mouse {
    pub x: u32,
    pub y: u32,
    pub left_button_state: ElementState,
    pub scroll_px: i32,
    pub line: Line,
    pub column: Column,
    pub cell_side: Side
}

impl Default for Mouse {
    fn default() -> Mouse {
        Mouse {
            x: 0,
            y: 0,
            left_button_state: ElementState::Released,
            scroll_px: 0,
            line: Line(0),
            column: Column(0),
            cell_side: Side::Left,
        }
    }
}

/// The event processor
///
/// Stores some state from received events and dispatches actions when they are
/// triggered.
pub struct Processor<N> {
    key_bindings: Vec<KeyBinding>,
    mouse_bindings: Vec<MouseBinding>,
    notifier: N,
    mouse: Mouse,
    resize_tx: mpsc::Sender<(u32, u32)>,
    ref_test: bool,
    size_info: SizeInfo,
    pub selection: Selection,
}

/// Notify that the terminal was resized
///
/// Currently this just forwards the notice to the input processor.
impl<N> OnResize for Processor<N> {
    fn on_resize(&mut self, size: &SizeInfo) {
        self.size_info = size.to_owned();
    }
}

impl<N: Notify> Processor<N> {
    /// Create a new event processor
    ///
    /// Takes a writer which is expected to be hooked up to the write end of a
    /// pty.
    pub fn new(
        notifier: N,
        resize_tx: mpsc::Sender<(u32, u32)>,
        config: &Config,
        ref_test: bool,
        size_info: SizeInfo,
    ) -> Processor<N> {
        Processor {
            key_bindings: config.key_bindings().to_vec(),
            mouse_bindings: config.mouse_bindings().to_vec(),
            notifier: notifier,
            resize_tx: resize_tx,
            ref_test: ref_test,
            mouse: Default::default(),
            selection: Default::default(),
            size_info: size_info,
        }
    }

    /// Handle events from glutin
    ///
    /// Doesn't take self mutably due to borrow checking. Kinda uggo but w/e.
    fn handle_event<'a>(
        processor: &mut input::Processor<'a, N>,
        event: glutin::Event,
        wakeup_request: &mut bool,
        ref_test: bool,
        resize_tx: &mpsc::Sender<(u32, u32)>,
    ) {
        match event {
            glutin::Event::Closed => {
                if ref_test {
                    // dump grid state
                    let grid = processor.ctx.terminal.grid();

                    let serialized_grid = json::to_string(&grid)
                        .expect("serialize grid");

                    let serialized_size = json::to_string(processor.ctx.terminal.size_info())
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
                resize_tx.send((w, h)).expect("send new size");

                // Previously, this marked the terminal state as "dirty", but
                // now the wakeup_request controls whether a display update is
                // triggered.
                *wakeup_request = true;
            },
            glutin::Event::KeyboardInput(state, _code, key, mods, string) => {
                processor.process_key(state, key, mods, string);
            },
            glutin::Event::MouseInput(state, button) => {
                processor.mouse_input(state, button);
                *wakeup_request = true;
            },
            glutin::Event::MouseMoved(x, y) => {
                let x = limit(x, 0, processor.ctx.size_info.width as i32);
                let y = limit(y, 0, processor.ctx.size_info.height as i32);

                processor.mouse_moved(x as u32, y as u32);

                if !processor.ctx.selection.is_empty() {
                    *wakeup_request = true;
                }
            },
            glutin::Event::Focused(true) => {
                *wakeup_request = true;
            },
            glutin::Event::MouseWheel(scroll_delta, touch_phase) => {
                processor.on_mouse_wheel(scroll_delta, touch_phase);
            },
            glutin::Event::Awakened => {
                *wakeup_request = true;
            },
            _ => (),
        }
    }

    /// Process at least one event and handle any additional queued events.
    pub fn process_events<'a>(
        &mut self,
        term: &'a FairMutex<Term>,
        window: &Window
    ) -> (MutexGuard<'a, Term>, bool) {
        let mut wakeup_request = false;

        // Terminal is lazily initialized the first time an event is returned
        // from the blocking WaitEventsIterator. Otherwise, the pty reader would
        // be blocked the entire time we wait for input!
        let terminal;

        {
            // Ditto on lazy initialization for context and processor.
            let context;
            let mut processor: input::Processor<N>;

            // Convenience macro which curries most arguments to handle_event.
            macro_rules! process {
                ($event:expr) => {
                    Processor::handle_event(
                        &mut processor,
                        $event,
                        &mut wakeup_request,
                        self.ref_test,
                        &self.resize_tx,
                    )
                }
            }

            match window.wait_events().next() {
                Some(event) => {
                    terminal = term.lock();
                    context = ActionContext {
                        terminal: &terminal,
                        notifier: &mut self.notifier,
                        selection: &mut self.selection,
                        mouse: &mut self.mouse,
                        size_info: &self.size_info,
                    };

                    processor = input::Processor {
                        ctx: context,
                        key_bindings: &self.key_bindings[..],
                        mouse_bindings: &self.mouse_bindings[..]
                    };

                    process!(event);
                },
                // Glutin guarantees the WaitEventsIterator never returns None.
                None => unreachable!(),
            }

            for event in window.poll_events() {
                process!(event);
            }
        }

        (terminal, wakeup_request)
    }

    pub fn update_config(&mut self, config: &Config) {
        self.key_bindings = config.key_bindings().to_vec();
        self.mouse_bindings = config.mouse_bindings().to_vec();
    }
}
