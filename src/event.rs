//! Process window events
use std::borrow::Cow;
use std::fs::File;
use std::io::Write;
use std::sync::mpsc;
use std::time::{Instant};

use serde_json as json;
use parking_lot::MutexGuard;
use glutin::{self, ModifiersState, Event, ElementState};
use copypasta::{Clipboard, Load, Store};

use grid::Scroll;
use config::{self, Config};
use cli::Options;
use display::OnResize;
use index::{Line, Column, Side, Point};
use input::{self, MouseBinding, KeyBinding};
use selection::Selection;
use sync::FairMutex;
use term::{Term, SizeInfo, TermMode};
use util::limit;
use util::fmt::Red;
use window::Window;

/// Byte sequences are sent to a `Notify` in response to some events
pub trait Notify {
    /// Notify that an escape sequence should be written to the pty
    ///
    /// TODO this needs to be able to error somehow
    fn notify<B: Into<Cow<'static, [u8]>>>(&mut self, B);
}

pub struct ActionContext<'a, N: 'a> {
    pub notifier: &'a mut N,
    pub terminal: &'a mut Term,
    pub size_info: &'a SizeInfo,
    pub mouse: &'a mut Mouse,
    pub received_count: &'a mut usize,
    pub suppress_chars: &'a mut bool,
    pub last_modifiers: &'a mut ModifiersState,
}

impl<'a, N: Notify + 'a> input::ActionContext for ActionContext<'a, N> {
    fn write_to_pty<B: Into<Cow<'static, [u8]>>>(&mut self, val: B) {
        self.notifier.notify(val);
    }

    fn terminal_mode(&self) -> TermMode {
        *self.terminal.mode()
    }

    fn size_info(&self) -> SizeInfo {
        *self.size_info
    }

    fn scroll(&mut self, scroll: Scroll) {
        self.terminal.scroll_display(scroll);
    }

    fn copy_selection(&self, buffer: ::copypasta::Buffer) {
        self.terminal
            .selection_to_string()
            .map(|selected| {
                if !selected.is_empty() {
                    Clipboard::new()
                        .and_then(|mut clipboard| clipboard.store(selected, buffer))
                        .unwrap_or_else(|err| {
                            warn!("Error storing selection to clipboard. {}", Red(err));
                        });
                }
            });
    }

    fn clear_selection(&mut self) {
        *self.terminal.selection_mut() = None;
        self.terminal.dirty = true;
    }

    fn update_selection(&mut self, point: Point, side: Side) {
        self.terminal.dirty = true;
        let point = self.terminal.visible_to_buffer(point);

        // Update selection if one exists
        if let Some(ref mut selection) = *self.terminal.selection_mut() {
            selection.update(point, side);
            return;
        }

        // Otherwise, start a regular selection
        *self.terminal.selection_mut() = Some(Selection::simple(point, side));
    }

    fn simple_selection(&mut self, point: Point, side: Side) {
        let point = self.terminal.visible_to_buffer(point);
        *self.terminal.selection_mut() = Some(Selection::simple(point, side));
        self.terminal.dirty = true;
    }

    fn semantic_selection(&mut self, point: Point) {
        let point = self.terminal.visible_to_buffer(point);
        *self.terminal.selection_mut() = Some(Selection::semantic(point, &*self.terminal));
        self.terminal.dirty = true;
    }

    fn line_selection(&mut self, point: Point) {
        let point = self.terminal.visible_to_buffer(point);
        *self.terminal.selection_mut() = Some(Selection::lines(point));
        self.terminal.dirty = true;
    }

    fn mouse_coords(&self) -> Option<Point> {
        self.terminal.pixels_to_coords(self.mouse.x as usize, self.mouse.y as usize)
    }

    fn change_font_size(&mut self, delta: i8) {
        self.terminal.change_font_size(delta);
    }

    fn reset_font_size(&mut self) {
        self.terminal.reset_font_size();
    }

    #[inline]
    fn mouse_mut(&mut self) -> &mut Mouse {
        self.mouse
    }

    #[inline]
    fn received_count(&mut self) -> &mut usize {
        &mut self.received_count
    }

    #[inline]
    fn suppress_chars(&mut self) -> &mut bool {
        &mut self.suppress_chars
    }

    #[inline]
    fn last_modifiers(&mut self) -> &mut ModifiersState {
        &mut self.last_modifiers
    }
}

pub enum ClickState {
    None,
    Click,
    DoubleClick,
    TripleClick,
}

/// State of the mouse
pub struct Mouse {
    pub x: usize,
    pub y: usize,
    pub left_button_state: ElementState,
    pub middle_button_state: ElementState,
    pub right_button_state: ElementState,
    pub last_click_timestamp: Instant,
    pub click_state: ClickState,
    pub scroll_px: i32,
    pub line: Line,
    pub column: Column,
    pub cell_side: Side,
    pub lines_scrolled: f32,
}

impl Default for Mouse {
    fn default() -> Mouse {
        Mouse {
            x: 0,
            y: 0,
            last_click_timestamp: Instant::now(),
            left_button_state: ElementState::Released,
            middle_button_state: ElementState::Released,
            right_button_state: ElementState::Released,
            click_state: ClickState::None,
            scroll_px: 0,
            line: Line(0),
            column: Column(0),
            cell_side: Side::Left,
            lines_scrolled: 0.0,
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
    mouse_config: config::Mouse,
    scrolling_config: config::Scrolling,
    print_events: bool,
    wait_for_event: bool,
    notifier: N,
    mouse: Mouse,
    resize_tx: mpsc::Sender<(u32, u32)>,
    ref_test: bool,
    size_info: SizeInfo,
    hide_cursor_when_typing: bool,
    hide_cursor: bool,
    received_count: usize,
    suppress_chars: bool,
    last_modifiers: ModifiersState,
    pending_events: Vec<Event>,
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
        options: &Options,
        config: &Config,
        ref_test: bool,
        size_info: SizeInfo,
    ) -> Processor<N> {
        Processor {
            key_bindings: config.key_bindings().to_vec(),
            mouse_bindings: config.mouse_bindings().to_vec(),
            mouse_config: config.mouse().to_owned(),
            scrolling_config: config.scrolling(),
            print_events: options.print_events,
            wait_for_event: true,
            notifier,
            resize_tx,
            ref_test,
            mouse: Default::default(),
            size_info: size_info,
            hide_cursor_when_typing: config.hide_cursor_when_typing(),
            hide_cursor: false,
            received_count: 0,
            suppress_chars: false,
            last_modifiers: Default::default(),
            pending_events: Vec::with_capacity(4),
        }
    }

    /// Handle events from glutin
    ///
    /// Doesn't take self mutably due to borrow checking. Kinda uggo but w/e.
    fn handle_event<'a>(
        processor: &mut input::Processor<'a, ActionContext<'a, N>>,
        event: Event,
        ref_test: bool,
        resize_tx: &mpsc::Sender<(u32, u32)>,
        hide_cursor: &mut bool,
        window_is_focused: &mut bool,
    ) {
        match event {
            // Pass on device events
            Event::DeviceEvent { .. } | Event::Suspended { .. } => (),
            Event::WindowEvent { event, .. } => {
                use glutin::WindowEvent::*;
                match event {
                    CloseRequested => {
                        if ref_test {
                            // dump grid state
                            let mut grid = processor.ctx.terminal.grid().clone();
                            grid.truncate();

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

                        // FIXME should do a more graceful shutdown
                        ::std::process::exit(0);
                    },
                    Resized(w, h) => {
                        resize_tx.send((w, h)).expect("send new size");
                        processor.ctx.terminal.dirty = true;
                    },
                    KeyboardInput { input, .. } => {
                        let glutin::KeyboardInput { state, virtual_keycode, modifiers, .. } = input;
                        processor.process_key(state, virtual_keycode, &modifiers);
                        if state == ElementState::Pressed {
                            // Hide cursor while typing
                            *hide_cursor = true;
                        }
                    },
                    ReceivedCharacter(c) => {
                        processor.received_char(c);
                    },
                    MouseInput { state, button, modifiers, .. } => {
                        *hide_cursor = false;
                        processor.mouse_input(state, button, modifiers);
                        processor.ctx.terminal.dirty = true;
                    },
                    CursorMoved { position: (x, y), modifiers, .. } => {
                        let x = limit(x as i32, 0, processor.ctx.size_info.width as i32);
                        let y = limit(y as i32, 0, processor.ctx.size_info.height as i32);

                        *hide_cursor = false;
                        processor.mouse_moved(x as usize, y as usize, modifiers);
                    },
                    MouseWheel { delta, phase, modifiers, .. } => {
                        *hide_cursor = false;
                        processor.on_mouse_wheel(delta, phase, modifiers);
                    },
                    Refresh => {
                        processor.ctx.terminal.dirty = true;
                    },
                    Focused(is_focused) => {
                        *window_is_focused = is_focused;

                        if is_focused {
                            processor.ctx.terminal.dirty = true;
                            processor.ctx.terminal.next_is_urgent = Some(false);
                        } else {
                            processor.ctx.terminal.dirty = true;
                            *hide_cursor = false;
                        }

                        processor.on_focus_change(is_focused);
                    },
                    DroppedFile(path) => {
                        use input::ActionContext;
                        let path: String = path.to_string_lossy().into();
                        processor.ctx.write_to_pty(path.into_bytes());
                    }
                    _ => (),
                }
            },
            Event::Awakened => {
                processor.ctx.terminal.dirty = true;
            }
        }
    }

    /// Process events. When `wait_for_event` is set, this method is guaranteed
    /// to process at least one event.
    pub fn process_events<'a>(
        &mut self,
        term: &'a FairMutex<Term>,
        window: &mut Window
    ) -> MutexGuard<'a, Term> {
        // Terminal is lazily initialized the first time an event is returned
        // from the blocking WaitEventsIterator. Otherwise, the pty reader would
        // be blocked the entire time we wait for input!
        let mut terminal;

        self.pending_events.clear();

        {
            // Ditto on lazy initialization for context and processor.
            let context;
            let mut processor: input::Processor<ActionContext<N>>;

            let print_events = self.print_events;

            let ref_test = self.ref_test;
            let resize_tx = &self.resize_tx;

            if self.wait_for_event {
                // A Vec is used here since wait_events can potentially yield
                // multiple events before the interrupt is handled. For example,
                // Resize and Moved events.
                let pending_events = &mut self.pending_events;
                window.wait_events(|e| {
                    pending_events.push(e);
                    glutin::ControlFlow::Break
                });
            }

            terminal = term.lock();

            context = ActionContext {
                terminal: &mut terminal,
                notifier: &mut self.notifier,
                mouse: &mut self.mouse,
                size_info: &self.size_info,
                received_count: &mut self.received_count,
                suppress_chars: &mut self.suppress_chars,
                last_modifiers: &mut self.last_modifiers,
            };

            processor = input::Processor {
                ctx: context,
                scrolling_config: &self.scrolling_config,
                mouse_config: &self.mouse_config,
                key_bindings: &self.key_bindings[..],
                mouse_bindings: &self.mouse_bindings[..],
            };

            let mut window_is_focused = window.is_focused;

            // Scope needed to that hide_cursor isn't borrowed after the scope
            // ends.
            {
                let hide_cursor = &mut self.hide_cursor;
                let mut process = |event| {
                    if print_events {
                        println!("glutin event: {:?}", event);
                    }
                    Processor::handle_event(
                        &mut processor,
                        event,
                        ref_test,
                        resize_tx,
                        hide_cursor,
                        &mut window_is_focused,
                    );
                };

                for event in self.pending_events.drain(..) {
                    process(event);
                }

                window.poll_events(process);
            }

            if self.hide_cursor_when_typing {
                window.set_cursor_visible(!self.hide_cursor);
            }

            window.is_focused = window_is_focused;
        }

        self.wait_for_event = !terminal.dirty;

        terminal
    }

    pub fn update_config(&mut self, config: &Config) {
        self.key_bindings = config.key_bindings().to_vec();
        self.mouse_bindings = config.mouse_bindings().to_vec();
        self.mouse_config = config.mouse().to_owned();
    }
}
