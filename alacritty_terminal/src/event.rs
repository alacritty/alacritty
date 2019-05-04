//! Process window events
use std::borrow::Cow;
use std::env;
#[cfg(unix)]
use std::fs;
use std::fs::File;
use std::io::Write;
use std::ops::RangeInclusive;
use std::sync::mpsc;
use std::time::Instant;

use glutin::dpi::PhysicalSize;
use glutin::{self, ElementState, Event, ModifiersState, MouseButton, MouseCursor};
use parking_lot::MutexGuard;
use serde_json as json;
use unicode_width::UnicodeWidthStr;

use crate::ansi::Handler;
use crate::cli::Options;
use crate::clipboard::ClipboardType;
use crate::config::{self, Config};
use crate::display::OnResize;
use crate::grid::Scroll;
use crate::index::{Column, Line, Linear, Point, Side};
use crate::input::{self, KeyBinding, MouseBinding, RelaxedEq};
use crate::selection::Selection;
use crate::sync::FairMutex;
use crate::term::cell::Cell;
use crate::term::mode::TermMode;
use crate::term::{Search, SizeInfo, Term};
#[cfg(unix)]
use crate::tty;
use crate::url::Url;
use crate::util::{limit, start_daemon};
use crate::window::Window;

/// Byte sequences are sent to a `Notify` in response to some events
pub trait Notify {
    /// Notify that an escape sequence should be written to the pty
    ///
    /// TODO this needs to be able to error somehow
    fn notify<B: Into<Cow<'static, [u8]>>>(&mut self, _: B);
}

pub struct ActionContext<'a, N> {
    pub notifier: &'a mut N,
    pub terminal: &'a mut Term,
    pub size_info: &'a mut SizeInfo,
    pub mouse: &'a mut Mouse,
    pub received_count: &'a mut usize,
    pub suppress_chars: &'a mut bool,
    pub last_modifiers: &'a mut ModifiersState,
    pub window_changes: &'a mut WindowChanges,
}

impl<'a, N: Notify + 'a> input::ActionContext for ActionContext<'a, N> {
    fn write_to_pty<B: Into<Cow<'static, [u8]>>>(&mut self, val: B) {
        self.notifier.notify(val);
    }

    fn size_info(&self) -> SizeInfo {
        *self.size_info
    }

    fn scroll(&mut self, scroll: Scroll) {
        self.terminal.scroll_display(scroll);

        if let ElementState::Pressed = self.mouse().left_button_state {
            let (x, y) = (self.mouse().x, self.mouse().y);
            let size_info = self.size_info();
            let point = size_info.pixels_to_coords(x, y);
            let cell_side = self.mouse().cell_side;
            self.update_selection(Point { line: point.line, col: point.col }, cell_side);
        }
    }

    fn copy_selection(&mut self, ty: ClipboardType) {
        if let Some(selected) = self.terminal.selection_to_string() {
            if !selected.is_empty() {
                self.terminal.clipboard().store(ty, selected);
            }
        }
    }

    fn selection_is_empty(&self) -> bool {
        self.terminal.selection().as_ref().map(Selection::is_empty).unwrap_or(true)
    }

    fn clear_selection(&mut self) {
        *self.terminal.selection_mut() = None;
        self.terminal.dirty = true;
    }

    fn update_selection(&mut self, point: Point, side: Side) {
        let point = self.terminal.visible_to_buffer(point);

        // Update selection if one exists
        if let Some(ref mut selection) = self.terminal.selection_mut() {
            selection.update(point, side);
        }

        self.terminal.dirty = true;
    }

    fn simple_selection(&mut self, point: Point, side: Side) {
        let point = self.terminal.visible_to_buffer(point);
        *self.terminal.selection_mut() = Some(Selection::simple(point, side));
        self.terminal.dirty = true;
    }

    fn semantic_selection(&mut self, point: Point) {
        let point = self.terminal.visible_to_buffer(point);
        *self.terminal.selection_mut() = Some(Selection::semantic(point));
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

    #[inline]
    fn mouse_mut(&mut self) -> &mut Mouse {
        self.mouse
    }

    #[inline]
    fn mouse(&self) -> &Mouse {
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

    #[inline]
    fn terminal(&self) -> &Term {
        self.terminal
    }

    #[inline]
    fn terminal_mut(&mut self) -> &mut Term {
        self.terminal
    }

    fn spawn_new_instance(&mut self) {
        let alacritty = env::args().next().unwrap();

        #[cfg(unix)]
        let args = {
            #[cfg(not(target_os = "freebsd"))]
            let proc_prefix = "";
            #[cfg(target_os = "freebsd")]
            let proc_prefix = "/compat/linux";
            let link_path = format!("{}/proc/{}/cwd", proc_prefix, tty::child_pid());
            if let Ok(path) = fs::read_link(link_path) {
                vec!["--working-directory".into(), path]
            } else {
                Vec::new()
            }
        };
        #[cfg(not(unix))]
        let args: Vec<String> = Vec::new();

        match start_daemon(&alacritty, &args) {
            Ok(_) => debug!("Started new Alacritty process: {} {:?}", alacritty, args),
            Err(_) => warn!("Unable to start new Alacritty process: {} {:?}", alacritty, args),
        }
    }

    fn toggle_fullscreen(&mut self) {
        self.window_changes.toggle_fullscreen();
    }

    #[cfg(target_os = "macos")]
    fn toggle_simple_fullscreen(&mut self) {
        self.window_changes.toggle_simple_fullscreen()
    }
}

/// The ActionContext can't really have direct access to the Window
/// with the current design. Event handlers that want to change the
/// window must set these flags instead. The processor will trigger
/// the actual changes.
#[derive(Default)]
pub struct WindowChanges {
    pub hide: bool,
    pub toggle_fullscreen: bool,
    #[cfg(target_os = "macos")]
    pub toggle_simple_fullscreen: bool,
}

impl WindowChanges {
    fn clear(&mut self) {
        *self = WindowChanges::default();
    }

    fn toggle_fullscreen(&mut self) {
        self.toggle_fullscreen = !self.toggle_fullscreen;
    }

    #[cfg(target_os = "macos")]
    fn toggle_simple_fullscreen(&mut self) {
        self.toggle_simple_fullscreen = !self.toggle_simple_fullscreen;
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
    pub block_url_launcher: bool,
    pub last_button: MouseButton,
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
            block_url_launcher: false,
            last_button: MouseButton::Other(0),
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
    resize_tx: mpsc::Sender<PhysicalSize>,
    ref_test: bool,
    size_info: SizeInfo,
    hide_mouse_when_typing: bool,
    hide_mouse: bool,
    received_count: usize,
    suppress_chars: bool,
    last_modifiers: ModifiersState,
    pending_events: Vec<Event>,
    window_changes: WindowChanges,
    save_to_clipboard: bool,
    alt_send_esc: bool,
    is_fullscreen: bool,
    is_simple_fullscreen: bool,
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
            scrolling_config: config.scrolling(),
            print_events: options.print_events,
            wait_for_event: true,
            notifier,
            resize_tx,
            ref_test,
            mouse: Default::default(),
            size_info,
            hide_mouse_when_typing: config.hide_mouse_when_typing(),
            hide_mouse: false,
            received_count: 0,
            suppress_chars: false,
            last_modifiers: Default::default(),
            pending_events: Vec::with_capacity(4),
            window_changes: Default::default(),
            save_to_clipboard: config.selection().save_to_clipboard,
            alt_send_esc: config.alt_send_esc(),
            is_fullscreen: false,
            is_simple_fullscreen: false,
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
        hide_mouse: &mut bool,
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
                            grid.initialize_all(&Cell::default());
                            grid.truncate();

                            let serialized_grid = json::to_string(&grid).expect("serialize grid");

                            let serialized_size =
                                json::to_string(processor.ctx.terminal.size_info())
                                    .expect("serialize size");

                            let serialized_config =
                                format!("{{\"history_size\":{}}}", grid.history_size());

                            File::create("./grid.json")
                                .and_then(|mut f| f.write_all(serialized_grid.as_bytes()))
                                .expect("write grid.json");

                            File::create("./size.json")
                                .and_then(|mut f| f.write_all(serialized_size.as_bytes()))
                                .expect("write size.json");

                            File::create("./config.json")
                                .and_then(|mut f| f.write_all(serialized_config.as_bytes()))
                                .expect("write config.json");
                        }

                        processor.ctx.terminal.exit();
                    },
                    Resized(lsize) => {
                        // Resize events are emitted via glutin/winit with logical sizes
                        // However the terminal, window and renderer use physical sizes
                        // so a conversion must be done here
                        resize_tx
                            .send(lsize.to_physical(processor.ctx.size_info.dpr))
                            .expect("send new size");
                        processor.ctx.terminal.dirty = true;
                    },
                    KeyboardInput { input, .. } => {
                        processor.process_key(input);
                        if input.state == ElementState::Pressed {
                            // Hide cursor while typing
                            *hide_mouse = true;
                        }
                    },
                    ReceivedCharacter(c) => {
                        processor.received_char(c);
                    },
                    MouseInput { state, button, modifiers, .. } => {
                        if !cfg!(target_os = "macos") || *window_is_focused {
                            *hide_mouse = false;
                            processor.mouse_input(state, button, modifiers);
                            processor.ctx.terminal.dirty = true;
                        }
                    },
                    CursorMoved { position: lpos, modifiers, .. } => {
                        let (x, y) = lpos.to_physical(processor.ctx.size_info.dpr).into();
                        let x: i32 = limit(x, 0, processor.ctx.size_info.width as i32);
                        let y: i32 = limit(y, 0, processor.ctx.size_info.height as i32);

                        *hide_mouse = false;
                        processor.mouse_moved(x as usize, y as usize, modifiers);
                    },
                    MouseWheel { delta, phase, modifiers, .. } => {
                        *hide_mouse = false;
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
                            processor.ctx.terminal.reset_url_highlight();
                            processor.ctx.terminal.dirty = true;
                            *hide_mouse = false;
                        }

                        processor.on_focus_change(is_focused);
                    },
                    DroppedFile(path) => {
                        use crate::input::ActionContext;
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
            },
        }
    }

    /// Process events. When `wait_for_event` is set, this method is guaranteed
    /// to process at least one event.
    pub fn process_events<'a>(
        &mut self,
        term: &'a FairMutex<Term>,
        window: &mut Window,
    ) -> MutexGuard<'a, Term> {
        // Terminal is lazily initialized the first time an event is returned
        // from the blocking WaitEventsIterator. Otherwise, the pty reader would
        // be blocked the entire time we wait for input!
        let mut terminal;

        self.pending_events.clear();

        {
            // Ditto on lazy initialization for context and processor.
            let context;
            let mut processor: input::Processor<'_, ActionContext<'_, N>>;

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
                size_info: &mut self.size_info,
                received_count: &mut self.received_count,
                suppress_chars: &mut self.suppress_chars,
                last_modifiers: &mut self.last_modifiers,
                window_changes: &mut self.window_changes,
            };

            processor = input::Processor {
                ctx: context,
                scrolling_config: &self.scrolling_config,
                mouse_config: &self.mouse_config,
                key_bindings: &self.key_bindings[..],
                mouse_bindings: &self.mouse_bindings[..],
                save_to_clipboard: self.save_to_clipboard,
                alt_send_esc: self.alt_send_esc,
            };

            let mut window_is_focused = window.is_focused;

            // Scope needed to that hide_mouse isn't borrowed after the scope
            // ends.
            {
                let hide_mouse = &mut self.hide_mouse;
                let mut process = |event| {
                    if print_events {
                        info!("glutin event: {:?}", event);
                    }
                    Processor::handle_event(
                        &mut processor,
                        event,
                        ref_test,
                        resize_tx,
                        hide_mouse,
                        &mut window_is_focused,
                    );
                };

                for event in self.pending_events.drain(..) {
                    process(event);
                }

                window.poll_events(process);
            }

            if self.hide_mouse_when_typing {
                window.set_mouse_visible(!self.hide_mouse);
            }

            window.is_focused = window_is_focused;
        }

        if self.window_changes.hide {
            window.hide();
        }

        #[cfg(target_os = "macos")]
        {
            if self.window_changes.toggle_simple_fullscreen && !self.is_fullscreen {
                window.set_simple_fullscreen(!self.is_simple_fullscreen);
                self.is_simple_fullscreen = !self.is_simple_fullscreen;
            }
        }

        if self.window_changes.toggle_fullscreen && !self.is_simple_fullscreen {
            window.set_fullscreen(!self.is_fullscreen);
            self.is_fullscreen = !self.is_fullscreen;
        }

        self.window_changes.clear();
        self.wait_for_event = !terminal.dirty;

        terminal
    }

    pub fn update_config(&mut self, config: &Config) {
        self.key_bindings = config.key_bindings().to_vec();
        self.mouse_bindings = config.mouse_bindings().to_vec();
        self.mouse_config = config.mouse().to_owned();
        self.save_to_clipboard = config.selection().save_to_clipboard;
        self.alt_send_esc = config.alt_send_esc();
    }


    /// Underline URLs and change the mouse cursor when URL hover state changes.
    pub fn update_url_highlight(&mut self, terminal: &mut Term) {
        let mouse_mode =
            TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG | TermMode::MOUSE_REPORT_CLICK;

        let point = Point::new(self.mouse.line, self.mouse.column);
        let modifiers = self.last_modifiers;

        // Only show URLs as launchable when all required modifiers are pressed
        let url = if self.mouse_config.url.modifiers.relaxed_eq(modifiers)
            && (!terminal.mode().intersects(mouse_mode) || modifiers.shift)
            && self.mouse_config.url.launcher.is_some()
        {
            terminal.url_search(point.into())
        } else {
            None
        };

        if let Some(Url { origin, text }) = url {
            let cols = self.size_info.cols().0;

            // Calculate the URL's start position
            let lines_before = (origin + cols - point.col.0 - 1) / cols;
            let (start_col, start_line) = if lines_before > point.line.0 {
                (0, 0)
            } else {
                let start_col = (cols + point.col.0 - origin % cols) % cols;
                let start_line = point.line.0 - lines_before;
                (start_col, start_line)
            };
            let start = Point::new(start_line, Column(start_col));

            // Calculate the URL's end position
            let len = text.width();
            let end_col = (point.col.0 + len - origin) % cols - 1;
            let end_line = point.line.0 + (point.col.0 + len - origin) / cols;
            let end = Point::new(end_line, Column(end_col));

            let start = Linear::from_point(Column(cols), start);
            let end = Linear::from_point(Column(cols), end);

            terminal.set_url_highlight(RangeInclusive::new(start, end));
            terminal.set_mouse_cursor(MouseCursor::Hand);
            terminal.dirty = true;
        } else {
            terminal.reset_url_highlight();
        }
    }
}
