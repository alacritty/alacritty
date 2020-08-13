//! The display subsystem including window management, font rasterization, and
//! GPU drawing.

use std::cmp::min;
use std::f64;
use std::fmt::{self, Formatter};
#[cfg(not(any(target_os = "macos", windows)))]
use std::sync::atomic::Ordering;
use std::time::Instant;

use glutin::dpi::{PhysicalPosition, PhysicalSize};
use glutin::event::ModifiersState;
use glutin::event_loop::EventLoop;
#[cfg(not(any(target_os = "macos", windows)))]
use glutin::platform::unix::EventLoopWindowTargetExtUnix;
use glutin::window::CursorIcon;
use log::{debug, info};
use parking_lot::MutexGuard;
use unicode_width::UnicodeWidthChar;
#[cfg(not(any(target_os = "macos", windows)))]
use wayland_client::{Display as WaylandDisplay, EventQueue};

#[cfg(target_os = "macos")]
use crossfont::set_font_smoothing;
use crossfont::{self, Rasterize, Rasterizer};

use alacritty_terminal::event::{EventListener, OnResize};
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::Selection;
use alacritty_terminal::term::{RenderableCell, SizeInfo, Term, TermMode};

use crate::config::font::Font;
use crate::config::window::StartupMode;
use crate::config::Config;
use crate::event::{Mouse, SearchState};
use crate::message_bar::{MessageBuffer, MessageType};
use crate::meter::Meter;
use crate::renderer::rects::{RenderLines, RenderRect};
use crate::renderer::{self, GlyphCache, QuadRenderer};
use crate::url::{Url, Urls};
use crate::window::{self, Window};

const FORWARD_SEARCH_LABEL: &str = "Search: ";
const BACKWARD_SEARCH_LABEL: &str = "Backward Search: ";

#[derive(Debug)]
pub enum Error {
    /// Error with window management.
    Window(window::Error),

    /// Error dealing with fonts.
    Font(crossfont::Error),

    /// Error in renderer.
    Render(renderer::Error),

    /// Error during buffer swap.
    ContextError(glutin::ContextError),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Window(err) => err.source(),
            Error::Font(err) => err.source(),
            Error::Render(err) => err.source(),
            Error::ContextError(err) => err.source(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Window(err) => err.fmt(f),
            Error::Font(err) => err.fmt(f),
            Error::Render(err) => err.fmt(f),
            Error::ContextError(err) => err.fmt(f),
        }
    }
}

impl From<window::Error> for Error {
    fn from(val: window::Error) -> Self {
        Error::Window(val)
    }
}

impl From<crossfont::Error> for Error {
    fn from(val: crossfont::Error) -> Self {
        Error::Font(val)
    }
}

impl From<renderer::Error> for Error {
    fn from(val: renderer::Error) -> Self {
        Error::Render(val)
    }
}

impl From<glutin::ContextError> for Error {
    fn from(val: glutin::ContextError) -> Self {
        Error::ContextError(val)
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct DisplayUpdate {
    pub dirty: bool,

    dimensions: Option<PhysicalSize<u32>>,
    font: Option<Font>,
    cursor_dirty: bool,
}

impl DisplayUpdate {
    pub fn dimensions(&self) -> Option<PhysicalSize<u32>> {
        self.dimensions
    }

    pub fn font(&self) -> Option<&Font> {
        self.font.as_ref()
    }

    pub fn cursor_dirty(&self) -> bool {
        self.cursor_dirty
    }

    pub fn set_dimensions(&mut self, dimensions: PhysicalSize<u32>) {
        self.dimensions = Some(dimensions);
        self.dirty = true;
    }

    pub fn set_font(&mut self, font: Font) {
        self.font = Some(font);
        self.dirty = true;
    }

    pub fn set_cursor_dirty(&mut self) {
        self.cursor_dirty = true;
        self.dirty = true;
    }
}

/// The display wraps a window, font rasterizer, and GPU renderer.
pub struct Display {
    pub size_info: SizeInfo,
    pub window: Window,
    pub urls: Urls,

    /// Currently highlighted URL.
    pub highlighted_url: Option<Url>,

    #[cfg(not(any(target_os = "macos", windows)))]
    pub wayland_event_queue: Option<EventQueue>,

    renderer: QuadRenderer,
    glyph_cache: GlyphCache,
    meter: Meter,
    #[cfg(not(any(target_os = "macos", windows)))]
    is_x11: bool,
}

impl Display {
    pub fn new<E>(config: &Config, event_loop: &EventLoop<E>) -> Result<Display, Error> {
        // Guess DPR based on first monitor.
        let estimated_dpr =
            event_loop.available_monitors().next().map(|m| m.scale_factor()).unwrap_or(1.);

        // Guess the target window dimensions.
        let metrics = GlyphCache::static_metrics(config.ui_config.font.clone(), estimated_dpr)?;
        let (cell_width, cell_height) = compute_cell_size(config, &metrics);

        let dimensions = GlyphCache::calculate_dimensions(
            &config.ui_config.window,
            estimated_dpr,
            cell_width,
            cell_height,
        );

        debug!("Estimated DPR: {}", estimated_dpr);
        debug!("Estimated Cell Size: {} x {}", cell_width, cell_height);
        debug!("Estimated Dimensions: {:?}", dimensions);

        #[cfg(not(any(target_os = "macos", windows)))]
        let mut wayland_event_queue = None;

        // Initialize Wayland event queue, to handle Wayland callbacks.
        #[cfg(not(any(target_os = "macos", windows)))]
        if let Some(display) = event_loop.wayland_display() {
            let display = unsafe { WaylandDisplay::from_external_display(display as _) };
            wayland_event_queue = Some(display.create_event_queue());
        }

        // Create the window where Alacritty will be displayed.
        let size = dimensions.map(|(width, height)| PhysicalSize::new(width, height));

        // Spawn window.
        let mut window = Window::new(
            event_loop,
            &config,
            size,
            #[cfg(not(any(target_os = "macos", windows)))]
            wayland_event_queue.as_ref(),
        )?;

        let dpr = window.scale_factor();
        info!("Device pixel ratio: {}", dpr);

        // get window properties for initializing the other subsystems.
        let viewport_size = window.inner_size();

        // Create renderer.
        let mut renderer = QuadRenderer::new()?;

        let (glyph_cache, cell_width, cell_height) =
            Self::new_glyph_cache(dpr, &mut renderer, config)?;

        let padding = config.ui_config.window.padding;
        let mut padding_x = f32::from(padding.x) * dpr as f32;
        let mut padding_y = f32::from(padding.y) * dpr as f32;

        if let Some((width, height)) =
            GlyphCache::calculate_dimensions(&config.ui_config.window, dpr, cell_width, cell_height)
        {
            let PhysicalSize { width: w, height: h } = window.inner_size();
            if w == width && h == height {
                info!("Estimated DPR correctly, skipping resize");
            } else {
                window.set_inner_size(PhysicalSize::new(width, height));
            }
        } else if config.ui_config.window.dynamic_padding {
            // Make sure additional padding is spread evenly.
            padding_x = dynamic_padding(padding_x, viewport_size.width as f32, cell_width);
            padding_y = dynamic_padding(padding_y, viewport_size.height as f32, cell_height);
        }

        padding_x = padding_x.floor();
        padding_y = padding_y.floor();

        info!("Cell Size: {} x {}", cell_width, cell_height);
        info!("Padding: {} x {}", padding_x, padding_y);

        // Create new size with at least one column and row.
        let size_info = SizeInfo {
            dpr,
            width: (viewport_size.width as f32).max(cell_width + 2. * padding_x),
            height: (viewport_size.height as f32).max(cell_height + 2. * padding_y),
            cell_width,
            cell_height,
            padding_x,
            padding_y,
        };

        // Update OpenGL projection.
        renderer.resize(&size_info);

        // Clear screen.
        let background_color = config.colors.primary.background;
        renderer.with_api(&config.ui_config, config.cursor, &size_info, |api| {
            api.clear(background_color);
        });

        // Set subpixel anti-aliasing.
        #[cfg(target_os = "macos")]
        set_font_smoothing(config.ui_config.font.use_thin_strokes());

        #[cfg(not(any(target_os = "macos", windows)))]
        let is_x11 = event_loop.is_x11();

        // On Wayland we can safely ignore this call, since the window isn't visible until you
        // actually draw something into it and commit those changes.
        #[cfg(not(any(target_os = "macos", windows)))]
        if is_x11 {
            window.swap_buffers();
            renderer.with_api(&config.ui_config, config.cursor, &size_info, |api| {
                api.finish();
            });
        }

        window.set_visible(true);

        // Set window position.
        //
        // TODO: replace `set_position` with `with_position` once available.
        // Upstream issue: https://github.com/rust-windowing/winit/issues/806.
        if let Some(position) = config.ui_config.window.position {
            window.set_outer_position(PhysicalPosition::from((position.x, position.y)));
        }

        #[allow(clippy::single_match)]
        match config.ui_config.window.startup_mode {
            StartupMode::Fullscreen => window.set_fullscreen(true),
            #[cfg(target_os = "macos")]
            StartupMode::SimpleFullscreen => window.set_simple_fullscreen(true),
            #[cfg(not(any(target_os = "macos", windows)))]
            StartupMode::Maximized => window.set_maximized(true),
            _ => (),
        }

        Ok(Self {
            window,
            renderer,
            glyph_cache,
            meter: Meter::new(),
            size_info,
            urls: Urls::new(),
            highlighted_url: None,
            #[cfg(not(any(target_os = "macos", windows)))]
            is_x11,
            #[cfg(not(any(target_os = "macos", windows)))]
            wayland_event_queue,
        })
    }

    fn new_glyph_cache(
        dpr: f64,
        renderer: &mut QuadRenderer,
        config: &Config,
    ) -> Result<(GlyphCache, f32, f32), Error> {
        let font = config.ui_config.font.clone();
        let rasterizer = Rasterizer::new(dpr as f32, config.ui_config.font.use_thin_strokes())?;

        // Initialize glyph cache.
        let glyph_cache = {
            info!("Initializing glyph cache...");
            let init_start = Instant::now();

            let cache =
                renderer.with_loader(|mut api| GlyphCache::new(rasterizer, &font, &mut api))?;

            let stop = init_start.elapsed();
            let stop_f = stop.as_secs() as f64 + f64::from(stop.subsec_nanos()) / 1_000_000_000f64;
            info!("... finished initializing glyph cache in {}s", stop_f);

            cache
        };

        // Need font metrics to resize the window properly. This suggests to me the
        // font metrics should be computed before creating the window in the first
        // place so that a resize is not needed.
        let (cw, ch) = compute_cell_size(config, &glyph_cache.font_metrics());

        Ok((glyph_cache, cw, ch))
    }

    /// Update font size and cell dimensions.
    fn update_glyph_cache(&mut self, config: &Config, font: &Font) {
        let size_info = &mut self.size_info;
        let cache = &mut self.glyph_cache;

        self.renderer.with_loader(|mut api| {
            let _ = cache.update_font_size(font, size_info.dpr, &mut api);
        });

        // Update cell size.
        let (cell_width, cell_height) = compute_cell_size(config, &self.glyph_cache.font_metrics());
        size_info.cell_width = cell_width;
        size_info.cell_height = cell_height;
    }

    /// Clear glyph cache.
    fn clear_glyph_cache(&mut self) {
        let cache = &mut self.glyph_cache;
        self.renderer.with_loader(|mut api| {
            cache.clear_glyph_cache(&mut api);
        });
    }

    /// Process update events.
    pub fn handle_update<T>(
        &mut self,
        terminal: &mut Term<T>,
        pty_resize_handle: &mut dyn OnResize,
        message_buffer: &MessageBuffer,
        search_active: bool,
        config: &Config,
        update_pending: DisplayUpdate,
    ) where
        T: EventListener,
    {
        // Update font size and cell dimensions.
        if let Some(font) = update_pending.font() {
            self.update_glyph_cache(config, font);
        } else if update_pending.cursor_dirty() {
            self.clear_glyph_cache();
        }

        let cell_width = self.size_info.cell_width;
        let cell_height = self.size_info.cell_height;

        // Recalculate padding.
        let padding = config.ui_config.window.padding;
        let mut padding_x = f32::from(padding.x) * self.size_info.dpr as f32;
        let mut padding_y = f32::from(padding.y) * self.size_info.dpr as f32;

        // Update the window dimensions.
        if let Some(size) = update_pending.dimensions() {
            // Ensure we have at least one column and row.
            self.size_info.width = (size.width as f32).max(cell_width + 2. * padding_x);
            self.size_info.height = (size.height as f32).max(cell_height + 2. * padding_y);
        }

        // Distribute excess padding equally on all sides.
        if config.ui_config.window.dynamic_padding {
            padding_x = dynamic_padding(padding_x, self.size_info.width, cell_width);
            padding_y = dynamic_padding(padding_y, self.size_info.height, cell_height);
        }

        self.size_info.padding_x = padding_x.floor() as f32;
        self.size_info.padding_y = padding_y.floor() as f32;

        let mut pty_size = self.size_info;

        // Subtract message bar lines from pty size.
        if let Some(message) = message_buffer.message() {
            let lines = message.text(&self.size_info).len();
            pty_size.height -= pty_size.cell_height * lines as f32;
        }

        // Add an extra line for the current search regex.
        if search_active {
            pty_size.height -= pty_size.cell_height;
        }

        // Resize PTY.
        pty_resize_handle.on_resize(&pty_size);

        // Resize terminal.
        terminal.resize(&pty_size);

        // Resize renderer.
        let physical = PhysicalSize::new(self.size_info.width as u32, self.size_info.height as u32);
        self.window.resize(physical);
        self.renderer.resize(&self.size_info);
    }

    /// Draw the screen.
    ///
    /// A reference to Term whose state is being drawn must be provided.
    ///
    /// This call may block if vsync is enabled.
    pub fn draw<T>(
        &mut self,
        terminal: MutexGuard<'_, Term<T>>,
        message_buffer: &MessageBuffer,
        config: &Config,
        mouse: &Mouse,
        mods: ModifiersState,
        search_state: &SearchState,
    ) {
        let grid_cells: Vec<RenderableCell> = terminal.renderable_cells(config).collect();
        let visual_bell_intensity = terminal.visual_bell.intensity();
        let background_color = terminal.background_color();
        let cursor_point = terminal.grid().cursor.point;
        let metrics = self.glyph_cache.font_metrics();
        let glyph_cache = &mut self.glyph_cache;
        let size_info = self.size_info;

        let selection = !terminal.selection.as_ref().map(Selection::is_empty).unwrap_or(true);
        let mouse_mode = terminal.mode().intersects(TermMode::MOUSE_MODE)
            && !terminal.mode().contains(TermMode::VI);

        let vi_mode_cursor = if terminal.mode().contains(TermMode::VI) {
            Some(terminal.vi_mode_cursor)
        } else {
            None
        };

        // Drop terminal as early as possible to free lock.
        drop(terminal);

        self.renderer.with_api(&config.ui_config, config.cursor, &size_info, |api| {
            api.clear(background_color);
        });

        let mut lines = RenderLines::new();
        let mut urls = Urls::new();

        // Draw grid.
        {
            let _sampler = self.meter.sampler();

            self.renderer.with_api(&config.ui_config, config.cursor, &size_info, |mut api| {
                // Iterate over all non-empty cells in the grid.
                for cell in grid_cells {
                    // Update URL underlines.
                    urls.update(size_info.cols(), cell);

                    // Update underline/strikeout.
                    lines.update(cell);

                    // Draw the cell.
                    api.render_cell(cell, glyph_cache);
                }
            });
        }

        let mut rects = lines.rects(&metrics, &size_info);

        // Update visible URLs.
        self.urls = urls;
        if let Some(url) = self.urls.highlighted(config, mouse, mods, mouse_mode, selection) {
            rects.append(&mut url.rects(&metrics, &size_info));

            self.window.set_mouse_cursor(CursorIcon::Hand);

            self.highlighted_url = Some(url);
        } else if self.highlighted_url.is_some() {
            self.highlighted_url = None;

            if mouse_mode {
                self.window.set_mouse_cursor(CursorIcon::Default);
            } else {
                self.window.set_mouse_cursor(CursorIcon::Text);
            }
        }

        // Highlight URLs at the vi mode cursor position.
        if let Some(vi_mode_cursor) = vi_mode_cursor {
            if let Some(url) = self.urls.find_at(vi_mode_cursor.point) {
                rects.append(&mut url.rects(&metrics, &size_info));
            }
        }

        // Push visual bell after url/underline/strikeout rects.
        if visual_bell_intensity != 0. {
            let visual_bell_rect = RenderRect::new(
                0.,
                0.,
                size_info.width,
                size_info.height,
                config.bell().color,
                visual_bell_intensity as f32,
            );
            rects.push(visual_bell_rect);
        }

        let mut message_bar_lines = 0;
        if let Some(message) = message_buffer.message() {
            let text = message.text(&size_info);
            message_bar_lines = text.len();

            // Create a new rectangle for the background.
            let start_line = size_info.lines().0 - message_bar_lines;
            let y = size_info.cell_height.mul_add(start_line as f32, size_info.padding_y);

            let color = match message.ty() {
                MessageType::Error => config.colors.normal().red,
                MessageType::Warning => config.colors.normal().yellow,
            };

            let message_bar_rect =
                RenderRect::new(0., y, size_info.width, size_info.height - y, color, 1.);

            // Push message_bar in the end, so it'll be above all other content.
            rects.push(message_bar_rect);

            // Draw rectangles.
            self.renderer.draw_rects(&size_info, rects);

            // Relay messages to the user.
            let fg = config.colors.primary.background;
            for (i, message_text) in text.iter().rev().enumerate() {
                self.renderer.with_api(&config.ui_config, config.cursor, &size_info, |mut api| {
                    api.render_string(
                        glyph_cache,
                        Line(size_info.lines().saturating_sub(i + 1)),
                        &message_text,
                        fg,
                        None,
                    );
                });
            }
        } else {
            // Draw rectangles.
            self.renderer.draw_rects(&size_info, rects);
        }

        self.draw_render_timer(config, &size_info);

        // Handle search and IME positioning.
        let ime_position = match search_state.regex() {
            Some(regex) => {
                let search_label = match search_state.direction() {
                    Direction::Right => FORWARD_SEARCH_LABEL,
                    Direction::Left => BACKWARD_SEARCH_LABEL,
                };

                let search_text = Self::format_search(&size_info, regex, search_label);

                // Render the search bar.
                self.draw_search(config, &size_info, message_bar_lines, &search_text);

                // Compute IME position.
                Point::new(size_info.lines() - 1, Column(search_text.len() - 1))
            },
            None => cursor_point,
        };

        // Update IME position.
        self.window.update_ime_position(ime_position, &self.size_info);

        // Frame event should be requested before swaping buffers, since it requires surface
        // `commit`, which is done by swap buffers under the hood.
        #[cfg(not(any(target_os = "macos", windows)))]
        self.request_frame(&self.window);

        self.window.swap_buffers();

        #[cfg(not(any(target_os = "macos", windows)))]
        if self.is_x11 {
            // On X11 `swap_buffers` does not block for vsync. However the next OpenGl command
            // will block to synchronize (this is `glClear` in Alacritty), which causes a
            // permanent one frame delay.
            self.renderer.with_api(&config.ui_config, config.cursor, &size_info, |api| {
                api.finish();
            });
        }
    }

    /// Format search regex to account for the cursor and fullwidth characters.
    fn format_search(size_info: &SizeInfo, search_regex: &str, search_label: &str) -> String {
        // Add spacers for wide chars.
        let mut formatted_regex = String::with_capacity(search_regex.len());
        for c in search_regex.chars() {
            formatted_regex.push(c);
            if c.width() == Some(2) {
                formatted_regex.push(' ');
            }
        }

        // Add cursor to show whitespace.
        formatted_regex.push('_');

        // Truncate beginning of the search regex if it exceeds the viewport width.
        let num_cols = size_info.cols().0;
        let label_len = search_label.len();
        let regex_len = formatted_regex.len();
        let truncate_len = min((regex_len + label_len).saturating_sub(num_cols), regex_len);
        let truncated_regex = &formatted_regex[truncate_len..];

        // Add search label to the beginning of the search regex.
        let mut bar_text = format!("{}{}", search_label, truncated_regex);

        // Make sure the label alone doesn't exceed the viewport width.
        bar_text.truncate(num_cols);

        bar_text
    }

    /// Draw current search regex.
    fn draw_search(
        &mut self,
        config: &Config,
        size_info: &SizeInfo,
        message_bar_lines: usize,
        text: &str,
    ) {
        let glyph_cache = &mut self.glyph_cache;
        let num_cols = size_info.cols().0;

        // Assure text length is at least num_cols.
        let text = format!("{:<1$}", text, num_cols);

        let fg = config.colors.search_bar_foreground();
        let bg = config.colors.search_bar_background();
        let line = size_info.lines() - message_bar_lines - 1;
        self.renderer.with_api(&config.ui_config, config.cursor, &size_info, |mut api| {
            api.render_string(glyph_cache, line, &text, fg, Some(bg));
        });
    }

    /// Draw render timer.
    fn draw_render_timer(&mut self, config: &Config, size_info: &SizeInfo) {
        if !config.ui_config.debug.render_timer {
            return;
        }
        let glyph_cache = &mut self.glyph_cache;

        let timing = format!("{:.3} usec", self.meter.average());
        let fg = config.colors.primary.background;
        let bg = config.colors.normal().red;

        self.renderer.with_api(&config.ui_config, config.cursor, &size_info, |mut api| {
            api.render_string(glyph_cache, size_info.lines() - 2, &timing[..], fg, Some(bg));
        });
    }

    /// Requst a new frame for a window on Wayland.
    #[inline]
    #[cfg(not(any(target_os = "macos", windows)))]
    fn request_frame(&self, window: &Window) {
        let surface = match window.wayland_surface() {
            Some(surface) => surface,
            None => return,
        };

        let should_draw = self.window.should_draw.clone();

        // Mark that window was drawn.
        should_draw.store(false, Ordering::Relaxed);

        // Request a new frame.
        surface.frame().quick_assign(move |_, _, _| {
            should_draw.store(true, Ordering::Relaxed);
        });
    }
}

/// Calculate padding to spread it evenly around the terminal content.
#[inline]
fn dynamic_padding(padding: f32, dimension: f32, cell_dimension: f32) -> f32 {
    padding + ((dimension - 2. * padding) % cell_dimension) / 2.
}

/// Calculate the cell dimensions based on font metrics.
#[inline]
fn compute_cell_size(config: &Config, metrics: &crossfont::Metrics) -> (f32, f32) {
    let offset_x = f64::from(config.ui_config.font.offset.x);
    let offset_y = f64::from(config.ui_config.font.offset.y);
    (
        ((metrics.average_advance + offset_x) as f32).floor().max(1.),
        ((metrics.line_height + offset_y) as f32).floor().max(1.),
    )
}
