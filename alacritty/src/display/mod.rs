//! The display subsystem including window management, font rasterization, and
//! GPU drawing.

use std::convert::TryFrom;
use std::fmt::{self, Formatter};
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use std::sync::atomic::Ordering;
use std::{cmp, mem};

use glutin::dpi::PhysicalSize;
use glutin::event::ModifiersState;
use glutin::event_loop::EventLoopWindowTarget;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use glutin::platform::unix::EventLoopWindowTargetExtUnix;
use glutin::window::CursorIcon;
use glutin::Rect as DamageRect;
use log::{debug, info};
use parking_lot::MutexGuard;
use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use wayland_client::EventQueue;

use crossfont::{self, Rasterize, Rasterizer};

use alacritty_terminal::ansi::NamedColor;
use alacritty_terminal::config::MAX_SCROLLBACK_LINES;
use alacritty_terminal::event::{EventListener, OnResize, WindowSize};
use alacritty_terminal::grid::Dimensions as TermDimensions;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::{Selection, SelectionRange};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Rgb;
use alacritty_terminal::term::{Term, TermDamage, TermMode, MIN_COLUMNS, MIN_SCREEN_LINES};

use crate::config::font::Font;
#[cfg(not(windows))]
use crate::config::window::StartupMode;
use crate::config::window::{Dimensions, Identity};
use crate::config::UiConfig;
use crate::display::bell::VisualBell;
use crate::display::color::List;
use crate::display::content::RenderableContent;
use crate::display::cursor::IntoRects;
use crate::display::damage::RenderDamageIterator;
use crate::display::hint::{HintMatch, HintState};
use crate::display::meter::Meter;
use crate::display::window::Window;
use crate::event::{Mouse, SearchState};
use crate::message_bar::{MessageBuffer, MessageType};
use crate::renderer::rects::{RenderLines, RenderRect};
use crate::renderer::{self, GlyphCache, Renderer};

pub mod content;
pub mod cursor;
pub mod hint;
pub mod window;

mod bell;
mod color;
mod damage;
mod meter;

/// Maximum number of linewraps followed outside of the viewport during search highlighting.
pub const MAX_SEARCH_LINES: usize = 100;

/// Label for the forward terminal search bar.
const FORWARD_SEARCH_LABEL: &str = "Search: ";

/// Label for the backward terminal search bar.
const BACKWARD_SEARCH_LABEL: &str = "Backward Search: ";

/// Color which is used to highlight damaged rects when debugging.
const DAMAGE_RECT_COLOR: Rgb = Rgb { r: 255, g: 0, b: 255 };

#[derive(Debug)]
pub enum Error {
    /// Error with window management.
    Window(window::Error),

    /// Error dealing with fonts.
    Font(crossfont::Error),

    /// Error in renderer.
    Render(renderer::Error),

    /// Error during buffer swap.
    Context(glutin::ContextError),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Window(err) => err.source(),
            Error::Font(err) => err.source(),
            Error::Render(err) => err.source(),
            Error::Context(err) => err.source(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Window(err) => err.fmt(f),
            Error::Font(err) => err.fmt(f),
            Error::Render(err) => err.fmt(f),
            Error::Context(err) => err.fmt(f),
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
        Error::Context(val)
    }
}

/// Terminal size info.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq)]
pub struct SizeInfo<T = f32> {
    /// Terminal window width.
    width: T,

    /// Terminal window height.
    height: T,

    /// Width of individual cell.
    cell_width: T,

    /// Height of individual cell.
    cell_height: T,

    /// Horizontal window padding.
    padding_x: T,

    /// Vertical window padding.
    padding_y: T,

    /// Number of lines in the viewport.
    screen_lines: usize,

    /// Number of columns in the viewport.
    columns: usize,
}

impl From<SizeInfo<f32>> for SizeInfo<u32> {
    fn from(size_info: SizeInfo<f32>) -> Self {
        Self {
            width: size_info.width as u32,
            height: size_info.height as u32,
            cell_width: size_info.cell_width as u32,
            cell_height: size_info.cell_height as u32,
            padding_x: size_info.padding_x as u32,
            padding_y: size_info.padding_y as u32,
            screen_lines: size_info.screen_lines,
            columns: size_info.screen_lines,
        }
    }
}

impl From<SizeInfo<f32>> for WindowSize {
    fn from(size_info: SizeInfo<f32>) -> Self {
        Self {
            num_cols: size_info.columns() as u16,
            num_lines: size_info.screen_lines() as u16,
            cell_width: size_info.cell_width() as u16,
            cell_height: size_info.cell_width() as u16,
        }
    }
}

impl<T: Clone + Copy> SizeInfo<T> {
    #[inline]
    pub fn width(&self) -> T {
        self.width
    }

    #[inline]
    pub fn height(&self) -> T {
        self.height
    }

    #[inline]
    pub fn cell_width(&self) -> T {
        self.cell_width
    }

    #[inline]
    pub fn cell_height(&self) -> T {
        self.cell_height
    }

    #[inline]
    pub fn padding_x(&self) -> T {
        self.padding_x
    }

    #[inline]
    pub fn padding_y(&self) -> T {
        self.padding_y
    }
}

impl SizeInfo<f32> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: f32,
        height: f32,
        cell_width: f32,
        cell_height: f32,
        mut padding_x: f32,
        mut padding_y: f32,
        dynamic_padding: bool,
    ) -> SizeInfo {
        if dynamic_padding {
            padding_x = Self::dynamic_padding(padding_x.floor(), width, cell_width);
            padding_y = Self::dynamic_padding(padding_y.floor(), height, cell_height);
        }

        let lines = (height - 2. * padding_y) / cell_height;
        let screen_lines = cmp::max(lines as usize, MIN_SCREEN_LINES);

        let columns = (width - 2. * padding_x) / cell_width;
        let columns = cmp::max(columns as usize, MIN_COLUMNS);

        SizeInfo {
            width,
            height,
            cell_width,
            cell_height,
            padding_x: padding_x.floor(),
            padding_y: padding_y.floor(),
            screen_lines,
            columns,
        }
    }

    #[inline]
    pub fn reserve_lines(&mut self, count: usize) {
        self.screen_lines = cmp::max(self.screen_lines.saturating_sub(count), MIN_SCREEN_LINES);
    }

    /// Check if coordinates are inside the terminal grid.
    ///
    /// The padding, message bar or search are not counted as part of the grid.
    #[inline]
    pub fn contains_point(&self, x: usize, y: usize) -> bool {
        x <= (self.padding_x + self.columns as f32 * self.cell_width) as usize
            && x > self.padding_x as usize
            && y <= (self.padding_y + self.screen_lines as f32 * self.cell_height) as usize
            && y > self.padding_y as usize
    }

    /// Calculate padding to spread it evenly around the terminal content.
    #[inline]
    fn dynamic_padding(padding: f32, dimension: f32, cell_dimension: f32) -> f32 {
        padding + ((dimension - 2. * padding) % cell_dimension) / 2.
    }
}

impl TermDimensions for SizeInfo {
    #[inline]
    fn columns(&self) -> usize {
        self.columns
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    #[inline]
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct DisplayUpdate {
    pub dirty: bool,

    dimensions: Option<PhysicalSize<u32>>,
    cursor_dirty: bool,
    font: Option<Font>,
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

    /// Hint highlighted by the mouse.
    pub highlighted_hint: Option<HintMatch>,

    /// Hint highlighted by the vi mode cursor.
    pub vi_highlighted_hint: Option<HintMatch>,

    #[cfg(not(any(target_os = "macos", windows)))]
    pub is_x11: bool,

    /// UI cursor visibility for blinking.
    pub cursor_hidden: bool,

    pub visual_bell: VisualBell,

    /// Mapped RGB values for each terminal color.
    pub colors: List,

    /// State of the keyboard hints.
    pub hint_state: HintState,

    /// Unprocessed display updates.
    pub pending_update: DisplayUpdate,

    is_damage_supported: bool,
    debug_damage: bool,
    damage_rects: Vec<DamageRect>,
    renderer: Renderer,
    glyph_cache: GlyphCache,
    meter: Meter,
}

impl Display {
    pub fn new<E>(
        config: &UiConfig,
        event_loop: &EventLoopWindowTarget<E>,
        identity: &Identity,
        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        wayland_event_queue: Option<&EventQueue>,
    ) -> Result<Display, Error> {
        #[cfg(any(not(feature = "x11"), target_os = "macos", windows))]
        let is_x11 = false;
        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        let is_x11 = event_loop.is_x11();

        // Guess scale_factor based on first monitor. On Wayland the initial frame always renders at
        // a scale factor of 1.
        let estimated_scale_factor = if cfg!(any(target_os = "macos", windows)) || is_x11 {
            event_loop.available_monitors().next().map(|m| m.scale_factor()).unwrap_or(1.)
        } else {
            1.
        };

        // Guess the target window dimensions.
        debug!("Loading \"{}\" font", &config.font.normal().family);
        let font = &config.font;
        let rasterizer = Rasterizer::new(estimated_scale_factor as f32, font.use_thin_strokes)?;
        let mut glyph_cache = GlyphCache::new(rasterizer, font)?;
        let metrics = glyph_cache.font_metrics();
        let (cell_width, cell_height) = compute_cell_size(config, &metrics);

        // Guess the target window size if the user has specified the number of lines/columns.
        let dimensions = config.window.dimensions();
        let estimated_size = dimensions.map(|dimensions| {
            window_size(config, dimensions, cell_width, cell_height, estimated_scale_factor)
        });

        debug!("Estimated scaling factor: {}", estimated_scale_factor);
        debug!("Estimated window size: {:?}", estimated_size);
        debug!("Estimated cell size: {} x {}", cell_width, cell_height);

        // Spawn the Alacritty window.
        let window = Window::new(
            event_loop,
            config,
            identity,
            estimated_size,
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            wayland_event_queue,
        )?;

        // Create renderer.
        let mut renderer = Renderer::new()?;

        let scale_factor = window.scale_factor;
        info!("Display scale factor: {}", scale_factor);

        // If the scaling factor changed update the glyph cache and mark for resize.
        let should_resize = (estimated_scale_factor - window.scale_factor).abs() > f64::EPSILON;
        let (cell_width, cell_height) = if should_resize {
            Self::update_glyph_cache(&mut renderer, &mut glyph_cache, scale_factor, config, font)
        } else {
            (cell_width, cell_height)
        };

        // Load font common glyphs to accelerate rendering.
        debug!("Filling glyph cache with common glyphs");
        renderer.with_loader(|mut api| {
            glyph_cache.load_common_glyphs(&mut api);
        });

        if let Some(dimensions) = dimensions.filter(|_| should_resize) {
            // Resize the window again if the scale factor was not estimated correctly.
            let size =
                window_size(config, dimensions, cell_width, cell_height, window.scale_factor);
            window.set_inner_size(size);
        }

        let padding = config.window.padding(window.scale_factor);
        let viewport_size = window.inner_size();

        // Create new size with at least one column and row.
        let size_info = SizeInfo::new(
            viewport_size.width as f32,
            viewport_size.height as f32,
            cell_width,
            cell_height,
            padding.0,
            padding.1,
            config.window.dynamic_padding && dimensions.is_none(),
        );

        info!("Cell size: {} x {}", cell_width, cell_height);
        info!("Padding: {} x {}", size_info.padding_x(), size_info.padding_y());
        info!("Width: {}, Height: {}", size_info.width(), size_info.height());

        // Update OpenGL projection.
        renderer.resize(&size_info);

        // Clear screen.
        let background_color = config.colors.primary.background;
        renderer.clear(background_color, config.window_opacity());

        // Set subpixel anti-aliasing.
        #[cfg(target_os = "macos")]
        crossfont::set_font_smoothing(config.font.use_thin_strokes);

        // Disable shadows for transparent windows on macOS.
        #[cfg(target_os = "macos")]
        window.set_has_shadow(config.window_opacity() >= 1.0);

        // On Wayland we can safely ignore this call, since the window isn't visible until you
        // actually draw something into it and commit those changes.
        #[cfg(not(any(target_os = "macos", windows)))]
        if is_x11 {
            window.swap_buffers();
            renderer.finish();
        }

        window.set_visible(true);

        #[allow(clippy::single_match)]
        #[cfg(not(windows))]
        match config.window.startup_mode {
            #[cfg(target_os = "macos")]
            StartupMode::SimpleFullscreen => window.set_simple_fullscreen(true),
            #[cfg(not(target_os = "macos"))]
            StartupMode::Maximized if is_x11 => window.set_maximized(true),
            _ => (),
        }

        let hint_state = HintState::new(config.hints.alphabet());
        let is_damage_supported = window.swap_buffers_with_damage_supported();
        let debug_damage = config.debug.highlight_damage;
        let damage_rects = if is_damage_supported || debug_damage {
            Vec::with_capacity(size_info.screen_lines())
        } else {
            Vec::new()
        };

        Ok(Self {
            window,
            renderer,
            glyph_cache,
            hint_state,
            meter: Meter::new(),
            size_info,
            highlighted_hint: None,
            vi_highlighted_hint: None,
            #[cfg(not(any(target_os = "macos", windows)))]
            is_x11,
            cursor_hidden: false,
            visual_bell: VisualBell::from(&config.bell),
            colors: List::from(&config.colors),
            pending_update: Default::default(),
            is_damage_supported,
            debug_damage,
            damage_rects,
        })
    }

    /// Update font size and cell dimensions.
    ///
    /// This will return a tuple of the cell width and height.
    fn update_glyph_cache(
        renderer: &mut Renderer,
        glyph_cache: &mut GlyphCache,
        scale_factor: f64,
        config: &UiConfig,
        font: &Font,
    ) -> (f32, f32) {
        renderer.with_loader(|mut api| {
            let _ = glyph_cache.update_font_size(font, scale_factor, &mut api);
        });

        // Compute new cell sizes.
        compute_cell_size(config, &glyph_cache.font_metrics())
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
        config: &UiConfig,
    ) where
        T: EventListener,
    {
        let pending_update = mem::take(&mut self.pending_update);

        let (mut cell_width, mut cell_height) =
            (self.size_info.cell_width(), self.size_info.cell_height());

        // Ensure we're modifying the correct OpenGL context.
        self.window.make_current();

        // Update font size and cell dimensions.
        if let Some(font) = pending_update.font() {
            let scale_factor = self.window.scale_factor;
            let cell_dimensions = Self::update_glyph_cache(
                &mut self.renderer,
                &mut self.glyph_cache,
                scale_factor,
                config,
                font,
            );
            cell_width = cell_dimensions.0;
            cell_height = cell_dimensions.1;

            info!("Cell size: {} x {}", cell_width, cell_height);
        } else if pending_update.cursor_dirty() {
            self.clear_glyph_cache();
        }

        let (mut width, mut height) = (self.size_info.width(), self.size_info.height());
        if let Some(dimensions) = pending_update.dimensions() {
            width = dimensions.width as f32;
            height = dimensions.height as f32;
        }

        let padding = config.window.padding(self.window.scale_factor);

        self.size_info = SizeInfo::new(
            width,
            height,
            cell_width,
            cell_height,
            padding.0,
            padding.1,
            config.window.dynamic_padding,
        );

        // Update number of column/lines in the viewport.
        let message_bar_lines =
            message_buffer.message().map(|m| m.text(&self.size_info).len()).unwrap_or(0);
        let search_lines = if search_active { 1 } else { 0 };
        self.size_info.reserve_lines(message_bar_lines + search_lines);

        // Resize PTY.
        pty_resize_handle.on_resize(self.size_info.into());

        // Resize terminal.
        terminal.resize(self.size_info);

        // Resize renderer.
        let physical = PhysicalSize::new(self.size_info.width() as _, self.size_info.height() as _);
        self.window.resize(physical);
        self.renderer.resize(&self.size_info);

        if self.collect_damage() {
            let lines = self.size_info.screen_lines();
            if lines > self.damage_rects.len() {
                self.damage_rects.reserve(lines);
            } else {
                self.damage_rects.shrink_to(lines);
            }
        }

        info!("Padding: {} x {}", self.size_info.padding_x(), self.size_info.padding_y());
        info!("Width: {}, Height: {}", self.size_info.width(), self.size_info.height());

        // Damage the entire screen after processing update.
        self.fully_damage();
    }

    /// Damage the entire window.
    fn fully_damage(&mut self) {
        let screen_rect = DamageRect {
            x: 0,
            y: 0,
            width: self.size_info.width() as u32,
            height: self.size_info.height() as u32,
        };

        self.damage_rects.push(screen_rect);
    }

    fn update_damage<T: EventListener>(
        &mut self,
        terminal: &mut MutexGuard<'_, Term<T>>,
        selection_range: Option<SelectionRange>,
        search_state: &SearchState,
    ) {
        let requires_full_damage = self.visual_bell.intensity() != 0.
            || self.hint_state.active()
            || search_state.regex().is_some();
        if requires_full_damage {
            terminal.mark_fully_damaged();
        }

        self.damage_highlighted_hints(terminal);
        match terminal.damage(selection_range) {
            TermDamage::Full => self.fully_damage(),
            TermDamage::Partial(damaged_lines) => {
                let damaged_rects = RenderDamageIterator::new(damaged_lines, self.size_info.into());
                for damaged_rect in damaged_rects {
                    self.damage_rects.push(damaged_rect);
                }
            },
        }
        terminal.reset_damage();

        // Ensure that the content requiring full damage is cleaned up again on the next frame.
        if requires_full_damage {
            terminal.mark_fully_damaged();
        }

        // Damage highlighted hints for the next frame as well, so we'll clear them.
        self.damage_highlighted_hints(terminal);
    }

    /// Draw the screen.
    ///
    /// A reference to Term whose state is being drawn must be provided.
    ///
    /// This call may block if vsync is enabled.
    pub fn draw<T: EventListener>(
        &mut self,
        mut terminal: MutexGuard<'_, Term<T>>,
        message_buffer: &MessageBuffer,
        config: &UiConfig,
        search_state: &SearchState,
    ) {
        // Collect renderable content before the terminal is dropped.
        let mut content = RenderableContent::new(config, self, &terminal, search_state);
        let mut grid_cells = Vec::new();
        for cell in &mut content {
            grid_cells.push(cell);
        }
        let selection_range = content.selection_range();
        let background_color = content.color(NamedColor::Background as usize);
        let display_offset = content.display_offset();
        let cursor = content.cursor();

        let cursor_point = terminal.grid().cursor.point;
        let total_lines = terminal.grid().total_lines();
        let metrics = self.glyph_cache.font_metrics();
        let size_info = self.size_info;

        let vi_mode = terminal.mode().contains(TermMode::VI);
        let vi_mode_cursor = if vi_mode { Some(terminal.vi_mode_cursor) } else { None };

        if self.collect_damage() {
            self.update_damage(&mut terminal, selection_range, search_state);
        }

        // Drop terminal as early as possible to free lock.
        drop(terminal);

        // Make sure this window's OpenGL context is active.
        self.window.make_current();

        self.renderer.clear(background_color, config.window_opacity());
        let mut lines = RenderLines::new();

        // Draw grid.
        {
            let _sampler = self.meter.sampler();

            // Ensure macOS hasn't reset our viewport.
            #[cfg(target_os = "macos")]
            self.renderer.set_viewport(&size_info);

            let glyph_cache = &mut self.glyph_cache;
            let highlighted_hint = &self.highlighted_hint;
            let vi_highlighted_hint = &self.vi_highlighted_hint;
            self.renderer.draw_cells(
                &size_info,
                glyph_cache,
                grid_cells.into_iter().map(|mut cell| {
                    // Underline hints hovered by mouse or vi mode cursor.
                    let point = viewport_to_point(display_offset, cell.point);
                    if highlighted_hint.as_ref().map_or(false, |h| h.bounds.contains(&point))
                        || vi_highlighted_hint.as_ref().map_or(false, |h| h.bounds.contains(&point))
                    {
                        cell.flags.insert(Flags::UNDERLINE);
                    }

                    // Update underline/strikeout.
                    lines.update(&cell);

                    cell
                }),
            );
        }

        let mut rects = lines.rects(&metrics, &size_info);

        if let Some(vi_mode_cursor) = vi_mode_cursor {
            // Indicate vi mode by showing the cursor's position in the top right corner.
            let vi_point = vi_mode_cursor.point;
            let line = (-vi_point.line.0 + size_info.bottommost_line().0) as usize;
            let obstructed_column = Some(vi_point)
                .filter(|point| point.line == -(display_offset as i32))
                .map(|point| point.column);
            self.draw_line_indicator(config, &size_info, total_lines, obstructed_column, line);
        } else if search_state.regex().is_some() {
            // Show current display offset in vi-less search to indicate match position.
            self.draw_line_indicator(config, &size_info, total_lines, None, display_offset);
        }

        // Draw cursor.
        for rect in cursor.rects(&size_info, config.terminal_config.cursor.thickness()) {
            rects.push(rect);
        }

        // Push visual bell after url/underline/strikeout rects.
        let visual_bell_intensity = self.visual_bell.intensity();
        if visual_bell_intensity != 0. {
            let visual_bell_rect = RenderRect::new(
                0.,
                0.,
                size_info.width(),
                size_info.height(),
                config.bell.color,
                visual_bell_intensity as f32,
            );
            rects.push(visual_bell_rect);
        }

        if self.debug_damage {
            self.highlight_damage(&mut rects);
        }

        if let Some(message) = message_buffer.message() {
            let search_offset = if search_state.regex().is_some() { 1 } else { 0 };
            let text = message.text(&size_info);

            // Create a new rectangle for the background.
            let start_line = size_info.screen_lines() + search_offset;
            let y = size_info.cell_height().mul_add(start_line as f32, size_info.padding_y());

            let bg = match message.ty() {
                MessageType::Error => config.colors.normal.red,
                MessageType::Warning => config.colors.normal.yellow,
            };

            let message_bar_rect =
                RenderRect::new(0., y, size_info.width(), size_info.height() - y, bg, 1.);

            // Push message_bar in the end, so it'll be above all other content.
            rects.push(message_bar_rect);

            // Draw rectangles.
            self.renderer.draw_rects(&size_info, &metrics, rects);

            // Relay messages to the user.
            let glyph_cache = &mut self.glyph_cache;
            let fg = config.colors.primary.background;
            for (i, message_text) in text.iter().enumerate() {
                let point = Point::new(start_line + i, Column(0));
                self.renderer.draw_string(point, fg, bg, message_text, &size_info, glyph_cache);
            }
        } else {
            // Draw rectangles.
            self.renderer.draw_rects(&size_info, &metrics, rects);
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
                self.draw_search(config, &size_info, &search_text);

                // Compute IME position.
                let line = Line(size_info.screen_lines() as i32 + 1);
                Point::new(line, Column(search_text.chars().count() - 1))
            },
            None => cursor_point,
        };

        // Update IME position.
        self.window.update_ime_position(ime_position, &self.size_info);

        // Frame event should be requested before swaping buffers, since it requires surface
        // `commit`, which is done by swap buffers under the hood.
        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        self.request_frame(&self.window);

        // Clearing debug highlights from the previous frame requires full redraw.
        if self.is_damage_supported && !self.debug_damage {
            self.window.swap_buffers_with_damage(&self.damage_rects);
        } else {
            self.window.swap_buffers();
        }

        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        if self.is_x11 {
            // On X11 `swap_buffers` does not block for vsync. However the next OpenGl command
            // will block to synchronize (this is `glClear` in Alacritty), which causes a
            // permanent one frame delay.
            self.renderer.finish();
        }

        self.damage_rects.clear();
    }

    /// Update to a new configuration.
    pub fn update_config(&mut self, config: &UiConfig) {
        self.debug_damage = config.debug.highlight_damage;
        self.visual_bell.update_config(&config.bell);
        self.colors = List::from(&config.colors);
    }

    /// Update the mouse/vi mode cursor hint highlighting.
    ///
    /// This will return whether the highlighted hints changed.
    pub fn update_highlighted_hints<T>(
        &mut self,
        term: &Term<T>,
        config: &UiConfig,
        mouse: &Mouse,
        modifiers: ModifiersState,
    ) -> bool {
        // Update vi mode cursor hint.
        let vi_highlighted_hint = if term.mode().contains(TermMode::VI) {
            let mods = ModifiersState::all();
            let point = term.vi_mode_cursor.point;
            hint::highlighted_at(term, config, point, mods)
        } else {
            None
        };
        let mut dirty = vi_highlighted_hint != self.vi_highlighted_hint;
        self.vi_highlighted_hint = vi_highlighted_hint;

        // Abort if mouse highlighting conditions are not met.
        if !mouse.inside_text_area || !term.selection.as_ref().map_or(true, Selection::is_empty) {
            dirty |= self.highlighted_hint.is_some();
            self.highlighted_hint = None;
            return dirty;
        }

        // Find highlighted hint at mouse position.
        let point = mouse.point(&self.size_info, term.grid().display_offset());
        let highlighted_hint = hint::highlighted_at(term, config, point, modifiers);

        // Update cursor shape.
        if highlighted_hint.is_some() {
            self.window.set_mouse_cursor(CursorIcon::Hand);
        } else if self.highlighted_hint.is_some() {
            if term.mode().intersects(TermMode::MOUSE_MODE) && !term.mode().contains(TermMode::VI) {
                self.window.set_mouse_cursor(CursorIcon::Default);
            } else {
                self.window.set_mouse_cursor(CursorIcon::Text);
            }
        }

        dirty |= self.highlighted_hint != highlighted_hint;
        self.highlighted_hint = highlighted_hint;

        dirty
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
        let num_cols = size_info.columns();
        let label_len = search_label.chars().count();
        let regex_len = formatted_regex.chars().count();
        let truncate_len = cmp::min((regex_len + label_len).saturating_sub(num_cols), regex_len);
        let index = formatted_regex.char_indices().nth(truncate_len).map(|(i, _c)| i).unwrap_or(0);
        let truncated_regex = &formatted_regex[index..];

        // Add search label to the beginning of the search regex.
        let mut bar_text = format!("{}{}", search_label, truncated_regex);

        // Make sure the label alone doesn't exceed the viewport width.
        bar_text.truncate(num_cols);

        bar_text
    }

    /// Draw current search regex.
    fn draw_search(&mut self, config: &UiConfig, size_info: &SizeInfo, text: &str) {
        let glyph_cache = &mut self.glyph_cache;
        let num_cols = size_info.columns();

        // Assure text length is at least num_cols.
        let text = format!("{:<1$}", text, num_cols);

        let point = Point::new(size_info.screen_lines(), Column(0));
        let fg = config.colors.search_bar_foreground();
        let bg = config.colors.search_bar_background();

        self.renderer.draw_string(point, fg, bg, &text, size_info, glyph_cache);
    }

    /// Draw render timer.
    fn draw_render_timer(&mut self, config: &UiConfig, size_info: &SizeInfo) {
        if !config.debug.render_timer {
            return;
        }

        let timing = format!("{:.3} usec", self.meter.average());
        let point = Point::new(size_info.screen_lines().saturating_sub(2), Column(0));
        let fg = config.colors.primary.background;
        let bg = config.colors.normal.red;

        // Damage the entire line.
        self.damage_from_point(point, self.size_info.columns() as u32);

        let glyph_cache = &mut self.glyph_cache;
        self.renderer.draw_string(point, fg, bg, &timing, size_info, glyph_cache);
    }

    /// Draw an indicator for the position of a line in history.
    fn draw_line_indicator(
        &mut self,
        config: &UiConfig,
        size_info: &SizeInfo,
        total_lines: usize,
        obstructed_column: Option<Column>,
        line: usize,
    ) {
        const fn num_digits(mut number: u32) -> usize {
            let mut res = 0;
            loop {
                number /= 10;
                res += 1;
                if number == 0 {
                    break res;
                }
            }
        }

        let text = format!("[{}/{}]", line, total_lines - 1);
        let column = Column(size_info.columns().saturating_sub(text.len()));
        let point = Point::new(0, column);

        // Damage the maximum possible length of the format text, which could be achieved when
        // using `MAX_SCROLLBACK_LINES` as current and total lines adding a `3` for formatting.
        const MAX_SIZE: usize = 2 * num_digits(MAX_SCROLLBACK_LINES) + 3;
        let damage_point = Point::new(0, Column(size_info.columns().saturating_sub(MAX_SIZE)));
        self.damage_from_point(damage_point, MAX_SIZE as u32);

        let colors = &config.colors;
        let fg = colors.line_indicator.foreground.unwrap_or(colors.primary.background);
        let bg = colors.line_indicator.background.unwrap_or(colors.primary.foreground);

        // Do not render anything if it would obscure the vi mode cursor.
        if obstructed_column.map_or(true, |obstructed_column| obstructed_column < column) {
            let glyph_cache = &mut self.glyph_cache;
            self.renderer.draw_string(point, fg, bg, &text, size_info, glyph_cache);
        }
    }

    /// Damage `len` starting from a `point`.
    #[inline]
    fn damage_from_point(&mut self, point: Point<usize>, len: u32) {
        if !self.collect_damage() {
            return;
        }

        let size_info: SizeInfo<u32> = self.size_info.into();
        let x = size_info.padding_x() + point.column.0 as u32 * size_info.cell_width();
        let y_top = size_info.height() - size_info.padding_y();
        let y = y_top - (point.line as u32 + 1) * size_info.cell_height();
        let width = len as u32 * size_info.cell_width();
        self.damage_rects.push(DamageRect { x, y, width, height: size_info.cell_height() })
    }

    /// Damage currently highlighted `Display` hints.
    #[inline]
    fn damage_highlighted_hints<T: EventListener>(&self, terminal: &mut Term<T>) {
        let display_offset = terminal.grid().display_offset();
        for hint in self.highlighted_hint.iter().chain(&self.vi_highlighted_hint) {
            for point in (hint.bounds.start().line.0..=hint.bounds.end().line.0).flat_map(|line| {
                point_to_viewport(display_offset, Point::new(Line(line), Column(0)))
            }) {
                terminal.damage_line(point.line, 0, terminal.columns() - 1);
            }
        }
    }

    /// Returns `true` if damage information should be collected, `false` otherwise.
    #[inline]
    fn collect_damage(&self) -> bool {
        self.is_damage_supported || self.debug_damage
    }

    /// Highlight damaged rects.
    ///
    /// This function is for debug purposes only.
    fn highlight_damage(&self, render_rects: &mut Vec<RenderRect>) {
        for damage_rect in &self.damage_rects {
            let x = damage_rect.x as f32;
            let height = damage_rect.height as f32;
            let width = damage_rect.width as f32;
            let y = self.size_info.height() - damage_rect.y as f32 - height;
            let render_rect = RenderRect::new(x, y, width, height, DAMAGE_RECT_COLOR, 0.5);

            render_rects.push(render_rect);
        }
    }

    /// Requst a new frame for a window on Wayland.
    #[inline]
    #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
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

impl Drop for Display {
    fn drop(&mut self) {
        // Switch OpenGL context before dropping, otherwise objects (like programs) from other
        // contexts might be deleted.
        self.window.make_current()
    }
}

/// Convert a terminal point to a viewport relative point.
#[inline]
pub fn point_to_viewport(display_offset: usize, point: Point) -> Option<Point<usize>> {
    let viewport_line = point.line.0 + display_offset as i32;
    usize::try_from(viewport_line).ok().map(|line| Point::new(line, point.column))
}

/// Convert a viewport relative point to a terminal point.
#[inline]
pub fn viewport_to_point(display_offset: usize, point: Point<usize>) -> Point {
    let line = Line(point.line as i32) - display_offset;
    Point::new(line, point.column)
}

/// Calculate the cell dimensions based on font metrics.
///
/// This will return a tuple of the cell width and height.
#[inline]
fn compute_cell_size(config: &UiConfig, metrics: &crossfont::Metrics) -> (f32, f32) {
    let offset_x = f64::from(config.font.offset.x);
    let offset_y = f64::from(config.font.offset.y);
    (
        (metrics.average_advance + offset_x).floor().max(1.) as f32,
        (metrics.line_height + offset_y).floor().max(1.) as f32,
    )
}

/// Calculate the size of the window given padding, terminal dimensions and cell size.
fn window_size(
    config: &UiConfig,
    dimensions: Dimensions,
    cell_width: f32,
    cell_height: f32,
    scale_factor: f64,
) -> PhysicalSize<u32> {
    let padding = config.window.padding(scale_factor);

    let grid_width = cell_width * dimensions.columns.0.max(MIN_COLUMNS) as f32;
    let grid_height = cell_height * dimensions.lines.max(MIN_SCREEN_LINES) as f32;

    let width = (padding.0).mul_add(2., grid_width).floor();
    let height = (padding.1).mul_add(2., grid_height).floor();

    PhysicalSize::new(width as u32, height as u32)
}
