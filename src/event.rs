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
use PhysicalSize;

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
    pub selection: &'a mut Option<Selection>,
    pub size_info: &'a mut SizeInfo,
    pub mouse: &'a mut Mouse,
    pub selection_modified: bool,
    pub received_count: &'a mut usize,
    pub suppress_chars: &'a mut bool,
    pub last_modifiers: &'a mut ModifiersState,
    pub window_changes: &'a mut WindowChanges,
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

    fn copy_selection(&self, buffer: ::copypasta::Buffer) {
        if let Some(ref selection) = *self.selection {
            if let Some(ref span) = selection.to_span(self.terminal) {
                let buf = self.terminal.string_from_selection(&span);
                if !buf.is_empty() {
                    Clipboard::new()
                        .and_then(|mut clipboard| clipboard.store(buf, buffer))
                        .unwrap_or_else(|err| {
                            warn!("Error storing selection to clipboard. {}", Red(err));
                        });
                }
            }
        }
    }

    fn clear_selection(&mut self) {
        *self.selection = None;
        self.selection_modified = true;
    }

    fn update_selection(&mut self, point: Point, side: Side) {
        self.selection_modified = true;
        // Update selection if one exists
        if let Some(ref mut selection) = *self.selection {
            selection.update(point, side);
            return;
        }

        // Otherwise, start a regular selection
        self.simple_selection(point, side);
    }

    fn simple_selection(&mut self, point: Point, side: Side) {
        *self.selection = Some(Selection::simple(point, side));
        self.selection_modified = true;
    }

    fn semantic_selection(&mut self, point: Point) {
        *self.selection = Some(Selection::semantic(point, self.terminal));
        self.selection_modified = true;
    }

    fn line_selection(&mut self, point: Point) {
        *self.selection = Some(Selection::lines(point));
        self.selection_modified = true;
    }

    fn mouse_coords(&self) -> Option<Point> {
        self.terminal.pixels_to_coords(self.mouse.x as usize, self.mouse.y as usize)
    }

    fn change_font_size(&mut self, delta: f32) {
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

    #[inline]
    fn hide_window(&mut self) {
        self.window_changes.hide = true;
    }
}

/// The ActionContext can't really have direct access to the Window
/// with the current design. Event handlers that want to change the
/// window must set these flags instead. The processor will trigger
/// the actual changes.
pub struct WindowChanges {
    pub hide: bool,
}

impl WindowChanges {
    fn clear(&mut self) {
        self.hide = false;
    }
}

impl Default for WindowChanges {
    fn default() -> WindowChanges {
        WindowChanges {
            hide: false,
        }
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
    pub x: u32,
    pub y: u32,
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
    print_events: bool,
    wait_for_event: bool,
    notifier: N,
    mouse: Mouse,
    resize_tx: mpsc::Sender<PhysicalSize>,
    ref_test: bool,
    size_info: SizeInfo,
    pub selection: Option<Selection>,
    hide_cursor_when_typing: bool,
    hide_cursor: bool,
    received_count: usize,
    suppress_chars: bool,
    last_modifiers: ModifiersState,
    pending_events: Vec<Event>,
    window_changes: WindowChanges,
}

/// Notify that the terminal was resized
///
/// Currently this just forwards the notice to the input processor.
impl<N> OnResize for Processor<N> {
    fn on_resize(&mut self, size: &SizeInfo) {
        self.size_info = size.to_owned();
        self.selection = None;
    }
}

impl<N: Notify> Processor<N> {
    /// Create a new event processor
    ///
    /// Takes a writer which is expected to be hooked up to the write end of a
    /// pty.
    pub fn new(
        notifier: N,
        resize_tx: mpsc::Sender<PhysicalSize>,
        options: &Options,
        config: &Config,
        ref_test: bool,
        size_info: SizeInfo,
    ) -> Processor<N> {
        Processor {
            key_bindings: config.key_bindings().to_vec(),
            mouse_bindings: config.mouse_bindings().to_vec(),
            mouse_config: config.mouse().to_owned(),
            print_events: options.print_events,
            wait_for_event: true,
            notifier,
            resize_tx,
            ref_test,
            mouse: Default::default(),
            selection: None,
            size_info,
            hide_cursor_when_typing: config.hide_cursor_when_typing(),
            hide_cursor: false,
            received_count: 0,
            suppress_chars: false,
            last_modifiers: Default::default(),
            pending_events: Vec::with_capacity(4),
            window_changes: Default::default(),
        }
    }

    /// Handle events from glutin
    ///
    /// Doesn't take self mutably due to borrow checking. Kinda uggo but w/e.
    fn handle_event<'a>(
        processor: &mut input::Processor<'a, ActionContext<'a, N>>,
        event: Event,
        ref_test: bool,
        resize_tx: &mpsc::Sender<PhysicalSize>,
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

                        // FIXME should do a more graceful shutdown
                        ::std::process::exit(0);
                    },
                    Resized(lsize) => {
                        resize_tx.send(lsize.to_physical(processor.ctx.size_info.dpr)).expect("send new size");
                        processor.ctx.terminal.dirty = true;
                    },
                    KeyboardInput { input, .. } => {
                        let glutin::KeyboardInput { state, virtual_keycode, modifiers, .. } = input;
                        processor.process_key(state, virtual_keycode, modifiers);
                        if state == ElementState::Pressed {
                            // Hide cursor while typing
                            *hide_cursor = true;
                        }
                    },
                    ReceivedCharacter(c) => {
                        processor.received_char(c);
                    },
                    MouseInput { state, button, modifiers, .. } => {
                        if *window_is_focused {
                            *hide_cursor = false;
                            processor.mouse_input(state, button, modifiers);
                            processor.ctx.terminal.dirty = true;
                        }
                    },
                    CursorMoved { position: lpos, modifiers, .. } => {
                        let (x, y) = lpos.to_physical(processor.ctx.size_info.dpr).into();
                        let x: i32 = limit(x, 0, processor.ctx.size_info.width as i32);
                        let y: i32 = limit(y, 0, processor.ctx.size_info.height as i32);

                        *hide_cursor = false;
                        processor.mouse_moved(x as u32, y as u32, modifiers);

                        if !processor.ctx.selection.is_none() {
                            processor.ctx.terminal.dirty = true;
                        }
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
                    },
                    HiDpiFactorChanged(new_dpr) => {
                        processor.ctx.size_info.dpr = new_dpr;
                        processor.ctx.terminal.dirty = true;
                    },
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
                selection: &mut self.selection,
                mouse: &mut self.mouse,
                size_info: &mut self.size_info,
                selection_modified: false,
                received_count: &mut self.received_count,
                suppress_chars: &mut self.suppress_chars,
                last_modifiers: &mut self.last_modifiers,
                window_changes: &mut self.window_changes,
            };

            processor = input::Processor {
                ctx: context,
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

            if processor.ctx.selection_modified {
                processor.ctx.terminal.dirty = true;
            }
        }

        if self.window_changes.hide {
            window.hide();
        }

        self.window_changes.clear();
        self.wait_for_event = !terminal.dirty;

        terminal
    }

    pub fn update_config(&mut self, config: &Config) {
        self.key_bindings = config.key_bindings().to_vec();
        self.mouse_bindings = config.mouse_bindings().to_vec();
        self.mouse_config = config.mouse().to_owned();
    }
}
