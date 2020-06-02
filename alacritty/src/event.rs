//! Process window events.
use std::borrow::Cow;
use std::cmp::max;
use std::env;
#[cfg(unix)]
use std::fs;
use std::fs::File;
use std::io::Write;
use std::mem;
use std::path::PathBuf;
#[cfg(not(any(target_os = "macos", windows)))]
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use glutin::dpi::PhysicalSize;
use glutin::event::{ElementState, Event as GlutinEvent, ModifiersState, WindowEvent};
use glutin::event_loop::{ControlFlow, EventLoop, EventLoopProxy, EventLoopWindowTarget};
use glutin::platform::desktop::EventLoopExtDesktop;
#[cfg(not(any(target_os = "macos", windows)))]
use glutin::platform::unix::EventLoopWindowTargetExtUnix;
use log::{debug, info, warn};
use serde_json as json;

#[cfg(target_os = "macos")]
use font::set_font_smoothing;
use font::{self, Size};

use alacritty_terminal::clipboard::ClipboardType;
use alacritty_terminal::config::Font;
use alacritty_terminal::config::LOG_TARGET_CONFIG;
use alacritty_terminal::event::OnResize;
use alacritty_terminal::event::{Event, EventListener, Notify};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::message_bar::{Message, MessageBuffer};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{SizeInfo, Term, TermMode};
#[cfg(not(windows))]
use alacritty_terminal::tty;
use alacritty_terminal::util::{limit, start_daemon};

use crate::cli::Options;
use crate::config;
use crate::config::Config;
use crate::display::Display;
use crate::input::{self, ActionContext as _, FONT_SIZE_STEP};
use crate::url::{Url, Urls};
use crate::window::Window;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct DisplayUpdate {
    pub dimensions: Option<PhysicalSize<u32>>,
    pub message_buffer: bool,
    pub font: Option<Font>,
    pub cursor: bool,
}

impl DisplayUpdate {
    fn is_empty(&self) -> bool {
        self.dimensions.is_none() && self.font.is_none() && !self.message_buffer && !self.cursor
    }
}

pub struct ActionContext<'a, N, T> {
    pub notifier: &'a mut N,
    pub terminal: &'a mut Term<T>,
    pub size_info: &'a mut SizeInfo,
    pub mouse: &'a mut Mouse,
    pub received_count: &'a mut usize,
    pub suppress_chars: &'a mut bool,
    pub modifiers: &'a mut ModifiersState,
    pub window: &'a mut Window,
    pub message_buffer: &'a mut MessageBuffer,
    pub display_update_pending: &'a mut DisplayUpdate,
    pub config: &'a mut Config,
    pub event_loop: &'a EventLoopWindowTarget<Event>,
    pub urls: &'a Urls,
    font_size: &'a mut Size,
}

impl<'a, N: Notify + 'a, T: EventListener> input::ActionContext<T> for ActionContext<'a, N, T> {
    fn write_to_pty<B: Into<Cow<'static, [u8]>>>(&mut self, val: B) {
        self.notifier.notify(val);
    }

    fn size_info(&self) -> SizeInfo {
        *self.size_info
    }

    fn scroll(&mut self, scroll: Scroll) {
        self.terminal.scroll_display(scroll);

        // Update selection.
        if self.terminal.mode().contains(TermMode::VI)
            && self.terminal.selection.as_ref().map(|s| s.is_empty()) != Some(true)
        {
            self.update_selection(self.terminal.vi_mode_cursor.point, Side::Right);
        } else if ElementState::Pressed == self.mouse().left_button_state {
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
        self.terminal.selection.as_ref().map(Selection::is_empty).unwrap_or(true)
    }

    fn clear_selection(&mut self) {
        self.terminal.selection = None;
        self.terminal.dirty = true;
    }

    fn update_selection(&mut self, point: Point, side: Side) {
        let point = self.terminal.visible_to_buffer(point);

        // Update selection if one exists.
        let vi_mode = self.terminal.mode().contains(TermMode::VI);
        if let Some(selection) = &mut self.terminal.selection {
            selection.update(point, side);

            if vi_mode {
                selection.include_all();
            }

            self.terminal.dirty = true;
        }
    }

    fn start_selection(&mut self, ty: SelectionType, point: Point, side: Side) {
        let point = self.terminal.visible_to_buffer(point);
        self.terminal.selection = Some(Selection::new(ty, point, side));
        self.terminal.dirty = true;
    }

    fn toggle_selection(&mut self, ty: SelectionType, point: Point, side: Side) {
        match &mut self.terminal.selection {
            Some(selection) if selection.ty == ty && !selection.is_empty() => {
                self.clear_selection();
            },
            Some(selection) if !selection.is_empty() => {
                selection.ty = ty;
                self.terminal.dirty = true;
            },
            _ => self.start_selection(ty, point, side),
        }
    }

    fn mouse_coords(&self) -> Option<Point> {
        let x = self.mouse.x as usize;
        let y = self.mouse.y as usize;

        if self.size_info.contains_point(x, y) {
            Some(self.size_info.pixels_to_coords(x, y))
        } else {
            None
        }
    }

    #[inline]
    fn mouse_mode(&self) -> bool {
        self.terminal.mode().intersects(TermMode::MOUSE_MODE)
            && !self.terminal.mode().contains(TermMode::VI)
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
    fn modifiers(&mut self) -> &mut ModifiersState {
        &mut self.modifiers
    }

    #[inline]
    fn window(&self) -> &Window {
        self.window
    }

    #[inline]
    fn window_mut(&mut self) -> &mut Window {
        self.window
    }

    #[inline]
    fn terminal(&self) -> &Term<T> {
        self.terminal
    }

    #[inline]
    fn terminal_mut(&mut self) -> &mut Term<T> {
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

    fn change_font_size(&mut self, delta: f32) {
        *self.font_size = max(*self.font_size + delta, Size::new(FONT_SIZE_STEP));
        let font = self.config.font.clone().with_size(*self.font_size);
        self.display_update_pending.font = Some(font);
        self.terminal.dirty = true;
    }

    fn reset_font_size(&mut self) {
        *self.font_size = self.config.font.size;
        self.display_update_pending.font = Some(self.config.font.clone());
        self.terminal.dirty = true;
    }

    fn pop_message(&mut self) {
        self.display_update_pending.message_buffer = true;
        self.message_buffer.pop();
    }

    fn message(&self) -> Option<&Message> {
        self.message_buffer.message()
    }

    fn config(&self) -> &Config {
        self.config
    }

    fn event_loop(&self) -> &EventLoopWindowTarget<Event> {
        self.event_loop
    }

    fn urls(&self) -> &Urls {
        self.urls
    }

    /// Spawn URL launcher when clicking on URLs.
    fn launch_url(&self, url: Url) {
        if self.mouse.block_url_launcher {
            return;
        }

        if let Some(ref launcher) = self.config.ui_config.mouse.url.launcher {
            let mut args = launcher.args().to_vec();
            let start = self.terminal.visible_to_buffer(url.start());
            let end = self.terminal.visible_to_buffer(url.end());
            args.push(self.terminal.bounds_to_string(start, end));

            match start_daemon(launcher.program(), &args) {
                Ok(_) => debug!("Launched {} with args {:?}", launcher.program(), args),
                Err(_) => warn!("Unable to launch {} with args {:?}", launcher.program(), args),
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ClickState {
    None,
    Click,
    DoubleClick,
    TripleClick,
}

/// State of the mouse.
#[derive(Debug)]
pub struct Mouse {
    pub x: usize,
    pub y: usize,
    pub left_button_state: ElementState,
    pub middle_button_state: ElementState,
    pub right_button_state: ElementState,
    pub last_click_timestamp: Instant,
    pub click_state: ClickState,
    pub scroll_px: f64,
    pub line: Line,
    pub column: Column,
    pub cell_side: Side,
    pub lines_scrolled: f32,
    pub block_url_launcher: bool,
    pub inside_grid: bool,
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
            scroll_px: 0.,
            line: Line(0),
            column: Column(0),
            cell_side: Side::Left,
            lines_scrolled: 0.,
            block_url_launcher: false,
            inside_grid: false,
        }
    }
}

/// The event processor.
///
/// Stores some state from received events and dispatches actions when they are
/// triggered.
pub struct Processor<N> {
    notifier: N,
    mouse: Mouse,
    received_count: usize,
    suppress_chars: bool,
    modifiers: ModifiersState,
    config: Config,
    message_buffer: MessageBuffer,
    display: Display,
    font_size: Size,
    event_queue: Vec<GlutinEvent<'static, alacritty_terminal::event::Event>>,
}

impl<N: Notify + OnResize> Processor<N> {
    /// Create a new event processor.
    ///
    /// Takes a writer which is expected to be hooked up to the write end of a PTY.
    pub fn new(
        notifier: N,
        message_buffer: MessageBuffer,
        config: Config,
        display: Display,
    ) -> Processor<N> {
        Processor {
            notifier,
            mouse: Default::default(),
            received_count: 0,
            suppress_chars: false,
            modifiers: Default::default(),
            font_size: config.font.size,
            config,
            message_buffer,
            display,
            event_queue: Vec::new(),
        }
    }

    /// Return `true` if `event_queue` is empty, `false` otherwise.
    #[inline]
    #[cfg(not(any(target_os = "macos", windows)))]
    fn event_queue_empty(&mut self) -> bool {
        let wayland_event_queue = match self.display.wayland_event_queue.as_mut() {
            Some(wayland_event_queue) => wayland_event_queue,
            // Since frame callbacks do not exist on X11, just check for event queue.
            None => return self.event_queue.is_empty(),
        };

        // Check for pending frame callbacks on Wayland.
        let events_dispatched = wayland_event_queue
            .dispatch_pending(&mut (), |_, _, _| {})
            .expect("failed to dispatch event queue");

        self.event_queue.is_empty() && events_dispatched == 0
    }

    /// Return `true` if `event_queue` is empty, `false` otherwise.
    #[inline]
    #[cfg(any(target_os = "macos", windows))]
    fn event_queue_empty(&mut self) -> bool {
        self.event_queue.is_empty()
    }

    /// Run the event loop.
    pub fn run<T>(&mut self, terminal: Arc<FairMutex<Term<T>>>, mut event_loop: EventLoop<Event>)
    where
        T: EventListener,
    {
        event_loop.run_return(|event, event_loop, control_flow| {
            if self.config.debug.print_events {
                info!("glutin event: {:?}", event);
            }

            // Ignore all events we do not care about.
            if Self::skip_event(&event) {
                return;
            }

            match event {
                // Check for shutdown.
                GlutinEvent::UserEvent(Event::Exit) => {
                    *control_flow = ControlFlow::Exit;
                    return;
                },
                // Process events.
                GlutinEvent::RedrawEventsCleared => {
                    *control_flow = ControlFlow::Wait;

                    if self.event_queue_empty() {
                        return;
                    }
                },
                // Remap DPR change event to remove lifetime.
                GlutinEvent::WindowEvent {
                    event: WindowEvent::ScaleFactorChanged { scale_factor, new_inner_size },
                    ..
                } => {
                    *control_flow = ControlFlow::Poll;
                    let size = (new_inner_size.width, new_inner_size.height);
                    let event = GlutinEvent::UserEvent(Event::DPRChanged(scale_factor, size));
                    self.event_queue.push(event);
                    return;
                },
                // Transmute to extend lifetime, which exists only for `ScaleFactorChanged` event.
                // Since we remap that event to remove the lifetime, this is safe.
                event => unsafe {
                    *control_flow = ControlFlow::Poll;
                    self.event_queue.push(mem::transmute(event));
                    return;
                },
            }

            let mut terminal = terminal.lock();

            let mut display_update_pending = DisplayUpdate::default();

            let context = ActionContext {
                terminal: &mut terminal,
                notifier: &mut self.notifier,
                mouse: &mut self.mouse,
                size_info: &mut self.display.size_info,
                received_count: &mut self.received_count,
                suppress_chars: &mut self.suppress_chars,
                modifiers: &mut self.modifiers,
                message_buffer: &mut self.message_buffer,
                display_update_pending: &mut display_update_pending,
                window: &mut self.display.window,
                font_size: &mut self.font_size,
                config: &mut self.config,
                urls: &self.display.urls,
                event_loop,
            };
            let mut processor = input::Processor::new(context, &self.display.highlighted_url);

            for event in self.event_queue.drain(..) {
                Processor::handle_event(event, &mut processor);
            }

            // Process DisplayUpdate events.
            if !display_update_pending.is_empty() {
                self.display.handle_update(
                    &mut terminal,
                    &mut self.notifier,
                    &self.message_buffer,
                    &self.config,
                    display_update_pending,
                );
            }

            #[cfg(not(any(target_os = "macos", windows)))]
            {
                // Skip rendering on Wayland until we get frame event from compositor.
                if event_loop.is_wayland()
                    && !self.display.window.should_draw.load(Ordering::Relaxed)
                {
                    return;
                }
            }

            if terminal.dirty {
                terminal.dirty = false;

                // Request immediate re-draw if visual bell animation is not finished yet.
                if !terminal.visual_bell.completed() {
                    self.event_queue.push(GlutinEvent::UserEvent(Event::Wakeup));
                }

                // Redraw screen.
                self.display.draw(
                    terminal,
                    &self.message_buffer,
                    &self.config,
                    &self.mouse,
                    self.modifiers,
                );
            }
        });

        // Write ref tests to disk.
        self.write_ref_test_results(&terminal.lock());
    }

    /// Handle events from glutin.
    ///
    /// Doesn't take self mutably due to borrow checking.
    fn handle_event<T>(
        event: GlutinEvent<Event>,
        processor: &mut input::Processor<T, ActionContext<N, T>>,
    ) where
        T: EventListener,
    {
        match event {
            GlutinEvent::UserEvent(event) => match event {
                Event::DPRChanged(scale_factor, (width, height)) => {
                    let display_update_pending = &mut processor.ctx.display_update_pending;

                    // Push current font to update its DPR.
                    display_update_pending.font =
                        Some(processor.ctx.config.font.clone().with_size(*processor.ctx.font_size));

                    // Resize to event's dimensions, since no resize event is emitted on Wayland.
                    display_update_pending.dimensions = Some(PhysicalSize::new(width, height));

                    processor.ctx.size_info.dpr = scale_factor;
                    processor.ctx.terminal.dirty = true;
                },
                Event::Title(title) => processor.ctx.window.set_title(&title),
                Event::Wakeup => processor.ctx.terminal.dirty = true,
                Event::Urgent => {
                    processor.ctx.window.set_urgent(!processor.ctx.terminal.is_focused)
                },
                Event::ConfigReload(path) => Self::reload_config(&path, processor),
                Event::Message(message) => {
                    processor.ctx.message_buffer.push(message);
                    processor.ctx.display_update_pending.message_buffer = true;
                    processor.ctx.terminal.dirty = true;
                },
                Event::MouseCursorDirty => processor.reset_mouse_cursor(),
                Event::Exit => (),
            },
            GlutinEvent::RedrawRequested(_) => processor.ctx.terminal.dirty = true,
            GlutinEvent::WindowEvent { event, window_id, .. } => {
                match event {
                    WindowEvent::CloseRequested => processor.ctx.terminal.exit(),
                    WindowEvent::Resized(size) => {
                        #[cfg(windows)]
                        {
                            // Minimizing the window sends a Resize event with zero width and
                            // height. But there's no need to ever actually resize to this.
                            // Both WinPTY & ConPTY have issues when resizing down to zero size
                            // and back.
                            if size.width == 0 && size.height == 0 {
                                return;
                            }
                        }

                        processor.ctx.display_update_pending.dimensions = Some(size);
                        processor.ctx.terminal.dirty = true;
                    },
                    WindowEvent::KeyboardInput { input, is_synthetic: false, .. } => {
                        processor.key_input(input);
                    },
                    WindowEvent::ReceivedCharacter(c) => processor.received_char(c),
                    WindowEvent::MouseInput { state, button, .. } => {
                        processor.ctx.window.set_mouse_visible(true);
                        processor.mouse_input(state, button);
                        processor.ctx.terminal.dirty = true;
                    },
                    WindowEvent::ModifiersChanged(modifiers) => {
                        processor.modifiers_input(modifiers)
                    },
                    WindowEvent::CursorMoved { position, .. } => {
                        let (x, y) = position.into();
                        let x = limit(x, 0, processor.ctx.size_info.width as i32);
                        let y = limit(y, 0, processor.ctx.size_info.height as i32);

                        processor.ctx.window.set_mouse_visible(true);
                        processor.mouse_moved(x as usize, y as usize);
                    },
                    WindowEvent::MouseWheel { delta, phase, .. } => {
                        processor.ctx.window.set_mouse_visible(true);
                        processor.mouse_wheel_input(delta, phase);
                    },
                    WindowEvent::Focused(is_focused) => {
                        if window_id == processor.ctx.window.window_id() {
                            processor.ctx.terminal.is_focused = is_focused;
                            processor.ctx.terminal.dirty = true;

                            if is_focused {
                                processor.ctx.window.set_urgent(false);
                            } else {
                                processor.ctx.window.set_mouse_visible(true);
                            }

                            processor.on_focus_change(is_focused);
                        }
                    },
                    WindowEvent::DroppedFile(path) => {
                        let path: String = path.to_string_lossy().into();
                        processor.ctx.write_to_pty((path + " ").into_bytes());
                    },
                    WindowEvent::CursorLeft { .. } => {
                        processor.ctx.mouse.inside_grid = false;

                        if processor.highlighted_url.is_some() {
                            processor.ctx.terminal.dirty = true;
                        }
                    },
                    WindowEvent::KeyboardInput { is_synthetic: true, .. }
                    | WindowEvent::TouchpadPressure { .. }
                    | WindowEvent::ScaleFactorChanged { .. }
                    | WindowEvent::CursorEntered { .. }
                    | WindowEvent::AxisMotion { .. }
                    | WindowEvent::HoveredFileCancelled
                    | WindowEvent::Destroyed
                    | WindowEvent::ThemeChanged(_)
                    | WindowEvent::HoveredFile(_)
                    | WindowEvent::Touch(_)
                    | WindowEvent::Moved(_) => (),
                }
            },
            GlutinEvent::Suspended { .. }
            | GlutinEvent::NewEvents { .. }
            | GlutinEvent::DeviceEvent { .. }
            | GlutinEvent::MainEventsCleared
            | GlutinEvent::RedrawEventsCleared
            | GlutinEvent::Resumed
            | GlutinEvent::LoopDestroyed => (),
        }
    }

    /// Check if an event is irrelevant and can be skipped.
    fn skip_event(event: &GlutinEvent<Event>) -> bool {
        match event {
            GlutinEvent::WindowEvent { event, .. } => match event {
                WindowEvent::KeyboardInput { is_synthetic: true, .. }
                | WindowEvent::TouchpadPressure { .. }
                | WindowEvent::CursorEntered { .. }
                | WindowEvent::AxisMotion { .. }
                | WindowEvent::HoveredFileCancelled
                | WindowEvent::Destroyed
                | WindowEvent::HoveredFile(_)
                | WindowEvent::Touch(_)
                | WindowEvent::Moved(_) => true,
                _ => false,
            },
            GlutinEvent::Suspended { .. }
            | GlutinEvent::NewEvents { .. }
            | GlutinEvent::MainEventsCleared
            | GlutinEvent::LoopDestroyed => true,
            _ => false,
        }
    }

    pub fn reload_config<T>(
        path: &PathBuf,
        processor: &mut input::Processor<T, ActionContext<N, T>>,
    ) where
        T: EventListener,
    {
        processor.ctx.message_buffer.remove_target(LOG_TARGET_CONFIG);
        processor.ctx.display_update_pending.message_buffer = true;

        let config = match config::reload_from(&path) {
            Ok(config) => config,
            Err(_) => return,
        };

        let options = Options::new();
        let config = options.into_config(config);

        processor.ctx.terminal.update_config(&config);

        // Reload cursor if we've changed its thickness.
        if (processor.ctx.config.cursor.thickness() - config.cursor.thickness()).abs()
            > std::f64::EPSILON
        {
            processor.ctx.display_update_pending.cursor = true;
        }

        if processor.ctx.config.font != config.font {
            // Do not update font size if it has been changed at runtime.
            if *processor.ctx.font_size == processor.ctx.config.font.size {
                *processor.ctx.font_size = config.font.size;
            }

            let font = config.font.clone().with_size(*processor.ctx.font_size);
            processor.ctx.display_update_pending.font = Some(font);
        }

        #[cfg(not(any(target_os = "macos", windows)))]
        {
            if processor.ctx.event_loop.is_wayland() {
                processor.ctx.window.set_wayland_theme(&config.colors);
            }
        }

        // Set subpixel anti-aliasing.
        #[cfg(target_os = "macos")]
        set_font_smoothing(config.font.use_thin_strokes());

        *processor.ctx.config = config;

        processor.ctx.terminal.dirty = true;
    }

    // Write the ref test results to the disk.
    pub fn write_ref_test_results<T>(&self, terminal: &Term<T>) {
        if !self.config.debug.ref_test {
            return;
        }

        // Dump grid state.
        let mut grid = terminal.grid().clone();
        grid.initialize_all(Cell::default());
        grid.truncate();

        let serialized_grid = json::to_string(&grid).expect("serialize grid");

        let serialized_size = json::to_string(&self.display.size_info).expect("serialize size");

        let serialized_config = format!("{{\"history_size\":{}}}", grid.history_size());

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
}

#[derive(Debug, Clone)]
pub struct EventProxy(EventLoopProxy<Event>);

impl EventProxy {
    pub fn new(proxy: EventLoopProxy<Event>) -> Self {
        EventProxy(proxy)
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        let _ = self.0.send_event(event);
    }
}
