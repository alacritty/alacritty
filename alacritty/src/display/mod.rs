//! The display subsystem including window management, font rasterization, and
//! GPU drawing.

use std::cmp;
use std::fmt::{self, Formatter};
use std::mem::{self, ManuallyDrop};
use std::num::NonZeroU32;
use std::ops::Deref;
use std::time::{Duration, Instant};

use glutin::config::GetGlConfig;
use glutin::context::{NotCurrentContext, PossiblyCurrentContext};
use glutin::display::GetGlDisplay;
use glutin::error::ErrorKind;
use glutin::prelude::*;
use glutin::surface::{Surface, SwapInterval, WindowSurface};

use log::{debug, info};
use parking_lot::MutexGuard;
use serde::{Deserialize, Serialize};
use winit::dpi::PhysicalSize;
use winit::keyboard::ModifiersState;
use winit::raw_window_handle::RawWindowHandle;
use winit::window::CursorIcon;

use crossfont::{Rasterize, Rasterizer, Size as FontSize};
use unicode_width::UnicodeWidthChar;

use alacritty_terminal::event::{EventListener, OnResize, WindowSize};
use alacritty_terminal::grid::Dimensions as TermDimensions;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::Selection;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{
    self, LineDamageBounds, Term, TermDamage, TermMode, MIN_COLUMNS, MIN_SCREEN_LINES,
};
use alacritty_terminal::vte::ansi::{CursorShape, NamedColor};

use crate::config::debug::RendererPreference;
use crate::config::font::Font;
use crate::config::tabs::{TabBarEdge, TabBarStyle, TabFontStyle, TabPowerlineStyle};
use crate::config::window::Dimensions;
#[cfg(not(windows))]
use crate::config::window::StartupMode;
use crate::config::UiConfig;
use crate::display::bell::VisualBell;
use crate::display::color::{List, Rgb};
use crate::display::content::{RenderableContent, RenderableCursor};
use crate::display::cursor::IntoRects;
use crate::display::damage::{damage_y_to_viewport_y, DamageTracker};
use crate::display::hint::{HintMatch, HintState};
use crate::display::meter::Meter;
use crate::display::window::Window;
use crate::event::{Event, EventType, Mouse, SearchState};
use crate::message_bar::{MessageBuffer, MessageType};
use crate::renderer::rects::{RenderLine, RenderLines, RenderRect};
use crate::renderer::{self, platform, GlyphCache, Renderer};
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::string::{ShortenDirection, StrShortener};

pub mod color;
pub mod content;
pub mod cursor;
pub mod hint;
pub mod window;

mod bell;
mod damage;
mod meter;

/// Label for the forward terminal search bar.
const FORWARD_SEARCH_LABEL: &str = "Search: ";

/// Label for the backward terminal search bar.
const BACKWARD_SEARCH_LABEL: &str = "Backward Search: ";

/// The character used to shorten the visible text like uri preview or search regex.
const SHORTENER: char = '…';

/// Color which is used to highlight damaged rects when debugging.
const DAMAGE_RECT_COLOR: Rgb = Rgb::new(255, 0, 255);

#[derive(Debug)]
pub enum Error {
    /// Error with window management.
    Window(window::Error),

    /// Error dealing with fonts.
    Font(crossfont::Error),

    /// Error in renderer.
    Render(renderer::Error),

    /// Error during context operations.
    Context(glutin::error::Error),
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

impl From<glutin::error::Error> for Error {
    fn from(val: glutin::error::Error) -> Self {
        Error::Context(val)
    }
}

/// Terminal size info.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
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

    /// Bottom window padding.
    padding_bottom_y: T,

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
            padding_bottom_y: size_info.padding_bottom_y as u32,
            screen_lines: size_info.screen_lines,
            columns: size_info.columns,
        }
    }
}

impl From<SizeInfo<f32>> for WindowSize {
    fn from(size_info: SizeInfo<f32>) -> Self {
        Self {
            num_cols: size_info.columns() as u16,
            num_lines: size_info.screen_lines() as u16,
            cell_width: size_info.cell_width() as u16,
            cell_height: size_info.cell_height() as u16,
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

    #[inline]
    pub fn padding_bottom_y(&self) -> T {
        self.padding_bottom_y
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
            padding_bottom_y: padding_y.floor(),
            screen_lines,
            columns,
        }
    }

    #[inline]
    pub fn reserve_lines(&mut self, count: usize) {
        self.screen_lines = cmp::max(self.screen_lines.saturating_sub(count), MIN_SCREEN_LINES);
    }

    #[inline]
    pub fn add_top_padding(&mut self, padding: f32) {
        self.padding_y += padding;
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

#[derive(Default, Clone, Debug, PartialEq, Eq)]
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
    pub window: Window,

    pub size_info: SizeInfo,

    /// Hint highlighted by the mouse.
    pub highlighted_hint: Option<HintMatch>,
    /// Frames since hint highlight was created.
    highlighted_hint_age: usize,

    /// Hint highlighted by the vi mode cursor.
    pub vi_highlighted_hint: Option<HintMatch>,
    /// Frames since hint highlight was created.
    vi_highlighted_hint_age: usize,

    pub raw_window_handle: RawWindowHandle,

    /// UI cursor visibility for blinking.
    pub cursor_hidden: bool,

    pub visual_bell: VisualBell,

    /// Mapped RGB values for each terminal color.
    pub colors: List,

    /// State of the keyboard hints.
    pub hint_state: HintState,

    /// Unprocessed display updates.
    pub pending_update: DisplayUpdate,

    /// The renderer update that takes place only once before the actual rendering.
    pub pending_renderer_update: Option<RendererUpdate>,

    /// The ime on the given display.
    pub ime: Ime,

    /// The state of the timer for frame scheduling.
    pub frame_timer: FrameTimer,

    /// Damage tracker for the given display.
    pub damage_tracker: DamageTracker,

    /// Font size used by the window.
    pub font_size: FontSize,

    // Mouse point position when highlighting hints.
    hint_mouse_point: Option<Point>,
    tab_hit_boxes: Vec<TabHitBox>,

    renderer: ManuallyDrop<Renderer>,
    renderer_preference: Option<RendererPreference>,

    surface: ManuallyDrop<Surface<WindowSurface>>,

    context: ManuallyDrop<PossiblyCurrentContext>,

    glyph_cache: GlyphCache,
    meter: Meter,
}

#[derive(Clone, Copy, Debug)]
struct TabHitBox {
    index: usize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Display {
    pub fn new(
        window: Window,
        gl_context: NotCurrentContext,
        config: &UiConfig,
        _tabbed: bool,
    ) -> Result<Display, Error> {
        let raw_window_handle = window.raw_window_handle();

        let scale_factor = window.scale_factor as f32;
        let rasterizer = Rasterizer::new()?;

        let font_size = config.font.size().scale(scale_factor);
        debug!("Loading \"{}\" font", &config.font.normal().family);
        let font = config.font.clone().with_size(font_size);
        let mut glyph_cache = GlyphCache::new(rasterizer, &font)?;

        let metrics = glyph_cache.font_metrics();
        let (cell_width, cell_height) = compute_cell_size(config, &metrics);

        // Resize the window to account for the user configured size.
        if let Some(dimensions) = config.window.dimensions() {
            let size = window_size(config, dimensions, cell_width, cell_height, scale_factor);
            window.request_inner_size(size);
        }

        // Create the GL surface to draw into.
        let surface = platform::create_gl_surface(
            &gl_context,
            window.inner_size(),
            window.raw_window_handle(),
        )?;

        // Make the context current.
        let context = gl_context.make_current(&surface)?;

        // Create renderer.
        let mut renderer = Renderer::new(&context, config.debug.renderer)?;

        // Load font common glyphs to accelerate rendering.
        debug!("Filling glyph cache with common glyphs");
        renderer.with_loader(|mut api| {
            glyph_cache.reset_glyph_cache(&mut api);
        });

        let padding = config.window.padding(window.scale_factor as f32);
        let viewport_size = window.inner_size();

        // Create new size with at least one column and row.
        let size_info = SizeInfo::new(
            viewport_size.width as f32,
            viewport_size.height as f32,
            cell_width,
            cell_height,
            padding.0,
            padding.1,
            config.window.dynamic_padding && config.window.dimensions().is_none(),
        );

        info!("Cell size: {cell_width} x {cell_height}");
        info!("Padding: {} x {}", size_info.padding_x(), size_info.padding_y());
        info!("Width: {}, Height: {}", size_info.width(), size_info.height());

        // Update OpenGL projection.
        renderer.resize(&size_info);

        // Clear screen.
        let background_color = config.colors.primary.background;
        renderer.clear(background_color, config.window_opacity());

        // Disable shadows for transparent windows on macOS.
        #[cfg(target_os = "macos")]
        window.set_has_shadow(config.window_opacity() >= 1.0);

        let is_wayland = matches!(raw_window_handle, RawWindowHandle::Wayland(_));

        // On Wayland we can safely ignore this call, since the window isn't visible until you
        // actually draw something into it and commit those changes.
        if !is_wayland {
            surface.swap_buffers(&context).expect("failed to swap buffers.");
            renderer.finish();
        }

        // Set resize increments for the newly created window.
        if config.window.resize_increments {
            window.set_resize_increments(PhysicalSize::new(cell_width, cell_height));
        }

        window.set_visible(true);

        // Always focus new windows, even if no Alacritty window is currently focused.
        #[cfg(target_os = "macos")]
        window.focus_window();

        #[allow(clippy::single_match)]
        #[cfg(not(windows))]
        if !_tabbed {
            match config.window.startup_mode {
                #[cfg(target_os = "macos")]
                StartupMode::SimpleFullscreen => window.set_simple_fullscreen(true),
                StartupMode::Maximized if !is_wayland => window.set_maximized(true),
                _ => (),
            }
        }

        let hint_state = HintState::new(config.hints.alphabet());

        let mut damage_tracker = DamageTracker::new(size_info.screen_lines(), size_info.columns());
        damage_tracker.debug = config.debug.highlight_damage;

        // Disable vsync.
        if let Err(err) = surface.set_swap_interval(&context, SwapInterval::DontWait) {
            info!("Failed to disable vsync: {err}");
        }

        Ok(Self {
            context: ManuallyDrop::new(context),
            visual_bell: VisualBell::from(&config.bell),
            renderer: ManuallyDrop::new(renderer),
            renderer_preference: config.debug.renderer,
            surface: ManuallyDrop::new(surface),
            colors: List::from(&config.colors),
            frame_timer: FrameTimer::new(),
            raw_window_handle,
            damage_tracker,
            glyph_cache,
            hint_state,
            size_info,
            font_size,
            window,
            pending_renderer_update: Default::default(),
            vi_highlighted_hint_age: Default::default(),
            highlighted_hint_age: Default::default(),
            vi_highlighted_hint: Default::default(),
            highlighted_hint: Default::default(),
            hint_mouse_point: Default::default(),
            tab_hit_boxes: Default::default(),
            pending_update: Default::default(),
            cursor_hidden: Default::default(),
            meter: Default::default(),
            ime: Default::default(),
        })
    }

    #[inline]
    pub fn gl_context(&self) -> &PossiblyCurrentContext {
        &self.context
    }

    pub fn make_not_current(&mut self) {
        if self.context.is_current() {
            self.context.make_not_current_in_place().expect("failed to disable context");
        }
    }

    pub fn make_current(&mut self) {
        let is_current = self.context.is_current();

        // Attempt to make the context current if it's not.
        let context_loss = if is_current {
            self.renderer.was_context_reset()
        } else {
            match self.context.make_current(&self.surface) {
                Err(err) if err.error_kind() == ErrorKind::ContextLost => {
                    info!("Context lost for window {:?}", self.window.id());
                    true
                },
                _ => false,
            }
        };

        if !context_loss {
            return;
        }

        let gl_display = self.context.display();
        let gl_config = self.context.config();
        let raw_window_handle = Some(self.window.raw_window_handle());
        let context = platform::create_gl_context(&gl_display, &gl_config, raw_window_handle)
            .expect("failed to recreate context.");

        // Drop the old context and renderer.
        unsafe {
            ManuallyDrop::drop(&mut self.renderer);
            ManuallyDrop::drop(&mut self.context);
        }

        // Activate new context.
        let context = context.treat_as_possibly_current();
        self.context = ManuallyDrop::new(context);
        self.context.make_current(&self.surface).expect("failed to reativate context after reset.");

        // Recreate renderer.
        let renderer = Renderer::new(&self.context, self.renderer_preference)
            .expect("failed to recreate renderer after reset");
        self.renderer = ManuallyDrop::new(renderer);

        // Resize the renderer.
        self.renderer.resize(&self.size_info);

        self.reset_glyph_cache();
        self.damage_tracker.frame().mark_fully_damaged();

        debug!("Recovered window {:?} from gpu reset", self.window.id());
    }

    fn swap_buffers(&self) {
        #[allow(clippy::single_match)]
        let res = match (self.surface.deref(), &self.context.deref()) {
            #[cfg(not(any(target_os = "macos", windows)))]
            (Surface::Egl(surface), PossiblyCurrentContext::Egl(context))
                if matches!(self.raw_window_handle, RawWindowHandle::Wayland(_))
                    && !self.damage_tracker.debug =>
            {
                let damage = self.damage_tracker.shape_frame_damage(self.size_info.into());
                surface.swap_buffers_with_damage(context, &damage)
            },
            (surface, context) => surface.swap_buffers(context),
        };
        if let Err(err) = res {
            debug!("error calling swap_buffers: {err}");
        }
    }

    /// Update font size and cell dimensions.
    ///
    /// This will return a tuple of the cell width and height.
    fn update_font_size(
        glyph_cache: &mut GlyphCache,
        config: &UiConfig,
        font: &Font,
    ) -> (f32, f32) {
        let _ = glyph_cache.update_font_size(font);

        // Compute new cell sizes.
        compute_cell_size(config, &glyph_cache.font_metrics())
    }

    /// Reset glyph cache.
    fn reset_glyph_cache(&mut self) {
        let cache = &mut self.glyph_cache;
        self.renderer.with_loader(|mut api| {
            cache.reset_glyph_cache(&mut api);
        });
    }

    // XXX: this function must not call to any `OpenGL` related tasks. Renderer updates are
    // performed in [`Self::process_renderer_update`] right before drawing.
    //
    /// Process update events.
    pub fn handle_update<T>(
        &mut self,
        terminal: &mut Term<T>,
        pty_resize_handle: &mut dyn OnResize,
        message_buffer: &MessageBuffer,
        search_state: &mut SearchState,
        config: &UiConfig,
        tab_bar_lines: usize,
        top_tab_bar_lines: usize,
        tab_title_editor_lines: usize,
    ) where
        T: EventListener,
    {
        let pending_update = mem::take(&mut self.pending_update);

        let (mut cell_width, mut cell_height) =
            (self.size_info.cell_width(), self.size_info.cell_height());

        if pending_update.font().is_some() || pending_update.cursor_dirty() {
            let renderer_update = self.pending_renderer_update.get_or_insert(Default::default());
            renderer_update.clear_font_cache = true
        }

        // Update font size and cell dimensions.
        if let Some(font) = pending_update.font() {
            let cell_dimensions = Self::update_font_size(&mut self.glyph_cache, config, font);
            cell_width = cell_dimensions.0;
            cell_height = cell_dimensions.1;

            info!("Cell size: {cell_width} x {cell_height}");

            // Mark entire terminal as damaged since glyph size could change without cell size
            // changes.
            self.damage_tracker.frame().mark_fully_damaged();
        }

        let (mut width, mut height) = (self.size_info.width(), self.size_info.height());
        if let Some(dimensions) = pending_update.dimensions() {
            width = dimensions.width as f32;
            height = dimensions.height as f32;
        }

        let padding = config.window.padding(self.window.scale_factor as f32);

        let mut new_size = SizeInfo::new(
            width,
            height,
            cell_width,
            cell_height,
            padding.0,
            padding.1,
            config.window.dynamic_padding,
        );

        // Update number of column/lines in the viewport.
        let search_active = search_state.history_index.is_some();
        let message_bar_lines = message_buffer.message().map_or(0, |m| m.text(&new_size).len());
        let search_lines = usize::from(search_active);
        new_size.reserve_lines(
            message_bar_lines + search_lines + tab_bar_lines + tab_title_editor_lines,
        );
        new_size.add_top_padding(top_tab_bar_lines as f32 * new_size.cell_height());

        // Update resize increments.
        if config.window.resize_increments {
            self.window.set_resize_increments(PhysicalSize::new(cell_width, cell_height));
        }

        // Resize when terminal when its dimensions have changed.
        if self.size_info.screen_lines() != new_size.screen_lines
            || self.size_info.columns() != new_size.columns()
        {
            // Resize PTY.
            pty_resize_handle.on_resize(new_size.into());

            // Resize terminal.
            terminal.resize(new_size);

            // Resize damage tracking.
            self.damage_tracker.resize(new_size.screen_lines(), new_size.columns());
        }

        // Check if dimensions have changed.
        if new_size != self.size_info {
            // Queue renderer update.
            let renderer_update = self.pending_renderer_update.get_or_insert(Default::default());
            renderer_update.resize = true;

            // Clear focused search match.
            search_state.clear_focused_match();
        }
        self.size_info = new_size;
    }

    // NOTE: Renderer updates are split off, since platforms like Wayland require resize and other
    // OpenGL operations to be performed right before rendering. Otherwise they could lock the
    // back buffer and render with the previous state. This also solves flickering during resizes.
    //
    /// Update the state of the renderer.
    pub fn process_renderer_update(&mut self) {
        let renderer_update = match self.pending_renderer_update.take() {
            Some(renderer_update) => renderer_update,
            _ => return,
        };

        // Resize renderer.
        if renderer_update.resize {
            let width = NonZeroU32::new(self.size_info.width() as u32).unwrap();
            let height = NonZeroU32::new(self.size_info.height() as u32).unwrap();
            self.surface.resize(&self.context, width, height);
        }

        // Ensure we're modifying the correct OpenGL context.
        self.make_current();

        if renderer_update.clear_font_cache {
            self.reset_glyph_cache();
        }

        self.renderer.resize(&self.size_info);

        info!("Padding: {} x {}", self.size_info.padding_x(), self.size_info.padding_y());
        info!("Width: {}, Height: {}", self.size_info.width(), self.size_info.height());
    }

    /// Draw the screen.
    ///
    /// A reference to Term whose state is being drawn must be provided.
    ///
    /// This call may block if vsync is enabled.
    pub fn draw<T: EventListener>(
        &mut self,
        mut terminal: MutexGuard<'_, Term<T>>,
        scheduler: &mut Scheduler,
        message_buffer: &MessageBuffer,
        config: &UiConfig,
        search_state: &mut SearchState,
        tab_titles: &[(String, bool)],
        tab_title_editor: Option<&str>,
    ) {
        // Collect renderable content before the terminal is dropped.
        let mut content = RenderableContent::new(config, self, &terminal, search_state);
        let mut grid_cells = Vec::new();
        for cell in &mut content {
            grid_cells.push(cell);
        }
        let selection_range = content.selection_range();
        let foreground_color = content.color(NamedColor::Foreground as usize);
        let background_color = content.color(NamedColor::Background as usize);
        let display_offset = content.display_offset();
        let cursor = content.cursor();

        let cursor_point = terminal.grid().cursor.point;
        let total_lines = terminal.grid().total_lines();
        let metrics = self.glyph_cache.font_metrics();
        let size_info = self.size_info;

        let vi_mode = terminal.mode().contains(TermMode::VI);
        let vi_cursor_point = if vi_mode { Some(terminal.vi_mode_cursor.point) } else { None };

        // Add damage from the terminal.
        match terminal.damage() {
            TermDamage::Full => self.damage_tracker.frame().mark_fully_damaged(),
            TermDamage::Partial(damaged_lines) => {
                for damage in damaged_lines {
                    self.damage_tracker.frame().damage_line(damage);
                }
            },
        }
        terminal.reset_damage();

        // Drop terminal as early as possible to free lock.
        drop(terminal);

        // Invalidate highlighted hints if grid has changed.
        self.validate_hint_highlights(display_offset);

        // Add damage from alacritty's UI elements overlapping terminal.

        let requires_full_damage = self.visual_bell.intensity() != 0.
            || self.hint_state.active()
            || search_state.regex().is_some();
        if requires_full_damage {
            self.damage_tracker.frame().mark_fully_damaged();
            self.damage_tracker.next_frame().mark_fully_damaged();
        }

        let vi_cursor_viewport_point =
            vi_cursor_point.and_then(|cursor| term::point_to_viewport(display_offset, cursor));
        self.damage_tracker.damage_vi_cursor(vi_cursor_viewport_point);
        self.damage_tracker.damage_selection(selection_range, display_offset);

        // Make sure this window's OpenGL context is active.
        self.make_current();

        self.renderer.clear(background_color, config.window_opacity());
        let mut lines = RenderLines::new();

        // Optimize loop hint comparator.
        let has_highlighted_hint =
            self.highlighted_hint.is_some() || self.vi_highlighted_hint.is_some();

        // Draw grid.
        {
            let _sampler = self.meter.sampler();

            // Ensure macOS hasn't reset our viewport.
            #[cfg(target_os = "macos")]
            self.renderer.set_viewport(&size_info);

            let glyph_cache = &mut self.glyph_cache;
            let highlighted_hint = &self.highlighted_hint;
            let vi_highlighted_hint = &self.vi_highlighted_hint;
            let damage_tracker = &mut self.damage_tracker;

            let cells = grid_cells.into_iter().map(|mut cell| {
                // Underline hints hovered by mouse or vi mode cursor.
                if has_highlighted_hint {
                    let point = term::viewport_to_point(display_offset, cell.point);
                    let hyperlink = cell.extra.as_ref().and_then(|extra| extra.hyperlink.as_ref());

                    let should_highlight = |hint: &Option<HintMatch>| {
                        hint.as_ref().is_some_and(|hint| hint.should_highlight(point, hyperlink))
                    };
                    if should_highlight(highlighted_hint) || should_highlight(vi_highlighted_hint) {
                        damage_tracker.frame().damage_point(cell.point);
                        cell.flags.insert(Flags::UNDERLINE);
                    }
                }

                // Update underline/strikeout.
                lines.update(&cell);

                cell
            });
            self.renderer.draw_cells(&size_info, glyph_cache, cells);
        }

        let mut rects = lines.rects(&metrics, &size_info);

        if let Some(vi_cursor_point) = vi_cursor_point {
            // Indicate vi mode by showing the cursor's position in the top right corner.
            let line = (-vi_cursor_point.line.0 + size_info.bottommost_line().0) as usize;
            let obstructed_column = Some(vi_cursor_point)
                .filter(|point| point.line == -(display_offset as i32))
                .map(|point| point.column);
            self.draw_line_indicator(config, total_lines, obstructed_column, line);
        } else if search_state.regex().is_some() {
            // Show current display offset in vi-less search to indicate match position.
            self.draw_line_indicator(config, total_lines, None, display_offset);
        };

        // Draw cursor.
        rects.extend(cursor.rects(&size_info, config.cursor.thickness()));

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

        // Handle IME positioning and search bar rendering.
        let ime_position = match search_state.regex() {
            Some(regex) => {
                let search_label = match search_state.direction() {
                    Direction::Right => FORWARD_SEARCH_LABEL,
                    Direction::Left => BACKWARD_SEARCH_LABEL,
                };

                let search_text = Self::format_search(regex, search_label, size_info.columns());

                // Render the search bar.
                self.draw_search(config, &search_text);

                // Draw search bar cursor.
                let line = size_info.screen_lines();
                let column = Column(search_text.chars().count() - 1);

                // Add cursor to search bar if IME is not active.
                if self.ime.preedit().is_none() {
                    let fg = config.colors.footer_bar_foreground();
                    let shape = CursorShape::Underline;
                    let cursor_width = NonZeroU32::new(1).unwrap();
                    let cursor =
                        RenderableCursor::new(Point::new(line, column), shape, fg, cursor_width);
                    rects.extend(cursor.rects(&size_info, config.cursor.thickness()));
                }

                Some(Point::new(line, column))
            },
            None => {
                let num_lines = self.size_info.screen_lines();
                match vi_cursor_viewport_point {
                    None => term::point_to_viewport(display_offset, cursor_point)
                        .filter(|point| point.line < num_lines),
                    point => point,
                }
            },
        };

        // Handle IME.
        if self.ime.is_enabled() {
            if let Some(point) = ime_position {
                let (fg, bg) = if search_state.regex().is_some() {
                    (config.colors.footer_bar_foreground(), config.colors.footer_bar_background())
                } else {
                    (foreground_color, background_color)
                };

                self.draw_ime_preview(point, fg, bg, &mut rects, config);
            }
        }

        let tab_title_editor_offset = usize::from(tab_title_editor.is_some());

        if let Some(message) = message_buffer.message() {
            let search_offset =
                usize::from(search_state.regex().is_some()) + tab_title_editor_offset;
            let text = message.text(&size_info);

            // Create a new rectangle for the background.
            let start_line = size_info.screen_lines() + search_offset;
            let y = size_info.cell_height().mul_add(start_line as f32, size_info.padding_y());

            let bg = match message.ty() {
                MessageType::Error => config.colors.normal.red,
                MessageType::Warning => config.colors.normal.yellow,
            };

            let x = 0;
            let width = size_info.width() as i32;
            let height = (size_info.height() - y) as i32;
            let message_bar_rect =
                RenderRect::new(x as f32, y, width as f32, height as f32, bg, 1.);

            // Push message_bar in the end, so it'll be above all other content.
            rects.push(message_bar_rect);

            // Always damage message bar, since it could have messages of the same size in it.
            self.damage_tracker.frame().add_viewport_rect(&size_info, x, y as i32, width, height);

            // Draw rectangles.
            self.renderer.draw_rects(&size_info, &metrics, rects);

            // Relay messages to the user.
            let glyph_cache = &mut self.glyph_cache;
            let fg = config.colors.primary.background;
            for (i, message_text) in text.iter().enumerate() {
                let point = Point::new(start_line + i, Column(0));
                self.renderer.draw_string(
                    point,
                    fg,
                    bg,
                    message_text.chars(),
                    &size_info,
                    glyph_cache,
                );
            }
        } else {
            // Draw rectangles.
            self.renderer.draw_rects(&size_info, &metrics, rects);
        }

        if let Some(tab_title_editor) = tab_title_editor {
            let line = size_info.screen_lines() + usize::from(search_state.regex().is_some());
            self.draw_footer_text(
                config,
                &format_search_prompt("Tab title: ", tab_title_editor, size_info.columns()),
                line,
            );
            let y = size_info.cell_height().mul_add(line as f32, size_info.padding_y()) as i32;
            let width = size_info.width() as i32;
            let height = size_info.cell_height() as i32;
            self.damage_tracker.frame().add_viewport_rect(&size_info, 0, y, width, height);
            self.damage_tracker.next_frame().add_viewport_rect(&size_info, 0, y, width, height);
        }

        self.tab_hit_boxes.clear();
        if config.tabs.display_tab_bar(tab_titles.len()) {
            let line = match config.tabs.tab_bar_edge {
                TabBarEdge::Top => 0,
                TabBarEdge::Bottom => {
                    let search_lines =
                        usize::from(search_state.regex().is_some()) + tab_title_editor_offset;
                    let message_lines =
                        message_buffer.message().map_or(0, |m| m.text(&size_info).len());
                    size_info.screen_lines() + search_lines + message_lines
                },
            };
            self.draw_tab_bar(config, tab_titles, line);
        }

        self.draw_render_timer(config);

        // Draw hyperlink uri preview.
        if has_highlighted_hint {
            let cursor_point = vi_cursor_point.or(Some(cursor_point));
            self.draw_hyperlink_preview(config, cursor_point, display_offset);
        }

        // Notify winit that we're about to present.
        self.window.pre_present_notify();

        // Highlight damage for debugging.
        if self.damage_tracker.debug {
            let damage = self.damage_tracker.shape_frame_damage(self.size_info.into());
            let mut rects = Vec::with_capacity(damage.len());
            self.highlight_damage(&mut rects);
            self.renderer.draw_rects(&self.size_info, &metrics, rects);
        }

        // Clearing debug highlights from the previous frame requires full redraw.
        self.swap_buffers();

        if matches!(self.raw_window_handle, RawWindowHandle::Xcb(_) | RawWindowHandle::Xlib(_)) {
            // On X11 `swap_buffers` does not block for vsync. However the next OpenGl command
            // will block to synchronize (this is `glClear` in Alacritty), which causes a
            // permanent one frame delay.
            self.renderer.finish();
        }

        // XXX: Request the new frame after swapping buffers, so the
        // time to finish OpenGL operations is accounted for in the timeout.
        if !matches!(self.raw_window_handle, RawWindowHandle::Wayland(_)) {
            self.request_frame(scheduler);
        }

        self.damage_tracker.swap_damage();
    }

    /// Update to a new configuration.
    pub fn update_config(&mut self, config: &UiConfig) {
        self.damage_tracker.debug = config.debug.highlight_damage;
        self.visual_bell.update_config(&config.bell);
        self.colors = List::from(&config.colors);
    }

    pub fn tab_at_position(&self, x: usize, y: usize) -> Option<usize> {
        self.tab_hit_boxes.iter().find_map(|hit_box| {
            let inside_x = (hit_box.x..hit_box.x + hit_box.width).contains(&(x as i32));
            let inside_y = (hit_box.y..hit_box.y + hit_box.height).contains(&(y as i32));
            (inside_x && inside_y).then_some(hit_box.index)
        })
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
        self.vi_highlighted_hint_age = 0;

        // Force full redraw if the vi mode highlight was cleared.
        if dirty {
            self.damage_tracker.frame().mark_fully_damaged();
        }

        // Abort if mouse highlighting conditions are not met.
        if !self.window.mouse_visible()
            || !mouse.inside_text_area
            || !term.selection.as_ref().is_none_or(Selection::is_empty)
        {
            if self.highlighted_hint.take().is_some() {
                self.damage_tracker.frame().mark_fully_damaged();
                dirty = true;
            }
            return dirty;
        }

        // Find highlighted hint at mouse position.
        let point = mouse.point(&self.size_info, term.grid().display_offset());
        let highlighted_hint = hint::highlighted_at(term, config, point, modifiers);

        // Update cursor shape.
        if highlighted_hint.is_some() {
            // If mouse changed the line, we should update the hyperlink preview, since the
            // highlighted hint could be disrupted by the old preview.
            dirty = self.hint_mouse_point.is_some_and(|p| p.line != point.line);
            self.hint_mouse_point = Some(point);
            self.window.set_mouse_cursor(CursorIcon::Pointer);
        } else if self.highlighted_hint.is_some() {
            self.hint_mouse_point = None;
            if term.mode().intersects(TermMode::MOUSE_MODE) && !term.mode().contains(TermMode::VI) {
                self.window.set_mouse_cursor(CursorIcon::Default);
            } else {
                self.window.set_mouse_cursor(CursorIcon::Text);
            }
        }

        let mouse_highlight_dirty = self.highlighted_hint != highlighted_hint;
        dirty |= mouse_highlight_dirty;
        self.highlighted_hint = highlighted_hint;
        self.highlighted_hint_age = 0;

        // Force full redraw if the mouse cursor highlight was changed.
        if mouse_highlight_dirty {
            self.damage_tracker.frame().mark_fully_damaged();
        }

        dirty
    }

    #[inline(never)]
    fn draw_ime_preview(
        &mut self,
        point: Point<usize>,
        fg: Rgb,
        bg: Rgb,
        rects: &mut Vec<RenderRect>,
        config: &UiConfig,
    ) {
        let preedit = match self.ime.preedit() {
            Some(preedit) => preedit,
            None => {
                // In case we don't have preedit, just set the popup point.
                self.window.update_ime_position(point, &self.size_info);
                return;
            },
        };

        let num_cols = self.size_info.columns();

        // Get the visible preedit.
        let visible_text: String = match (preedit.cursor_byte_offset, preedit.cursor_end_offset) {
            (Some(byte_offset), Some(end_offset)) if end_offset.0 > num_cols => StrShortener::new(
                &preedit.text[byte_offset.0..],
                num_cols,
                ShortenDirection::Right,
                Some(SHORTENER),
            ),
            _ => {
                StrShortener::new(&preedit.text, num_cols, ShortenDirection::Left, Some(SHORTENER))
            },
        }
        .collect();

        let visible_len = visible_text.chars().count();

        let end = cmp::min(point.column.0 + visible_len, num_cols);
        let start = end.saturating_sub(visible_len);

        let start = Point::new(point.line, Column(start));
        let end = Point::new(point.line, Column(end - 1));

        let glyph_cache = &mut self.glyph_cache;
        let metrics = glyph_cache.font_metrics();

        self.renderer.draw_string(
            start,
            fg,
            bg,
            visible_text.chars(),
            &self.size_info,
            glyph_cache,
        );

        // Damage preedit inside the terminal viewport.
        if point.line < self.size_info.screen_lines() {
            let damage = LineDamageBounds::new(start.line, 0, num_cols);
            self.damage_tracker.frame().damage_line(damage);
            self.damage_tracker.next_frame().damage_line(damage);
        }

        // Add underline for preedit text.
        let underline = RenderLine { start, end, color: fg };
        rects.extend(underline.rects(Flags::UNDERLINE, &metrics, &self.size_info));

        let ime_popup_point = match preedit.cursor_end_offset {
            Some(cursor_end_offset) => {
                // Use hollow block when multiple characters are changed at once.
                let (shape, width) = if let Some(width) =
                    NonZeroU32::new((cursor_end_offset.0 - cursor_end_offset.1) as u32)
                {
                    (CursorShape::HollowBlock, width)
                } else {
                    (CursorShape::Beam, NonZeroU32::new(1).unwrap())
                };

                let cursor_column = Column(
                    (end.column.0 as isize - cursor_end_offset.0 as isize + 1).max(0) as usize,
                );
                let cursor_point = Point::new(point.line, cursor_column);
                let cursor = RenderableCursor::new(cursor_point, shape, fg, width);
                rects.extend(cursor.rects(&self.size_info, config.cursor.thickness()));
                cursor_point
            },
            _ => end,
        };

        self.window.update_ime_position(ime_popup_point, &self.size_info);
    }

    /// Format search regex to account for the cursor and fullwidth characters.
    fn format_search(search_regex: &str, search_label: &str, max_width: usize) -> String {
        let label_len = search_label.len();

        // Skip `search_regex` formatting if only label is visible.
        if label_len > max_width {
            return search_label[..max_width].to_owned();
        }

        // The search string consists of `search_label` + `search_regex` + `cursor`.
        let mut bar_text = String::from(search_label);
        bar_text.extend(StrShortener::new(
            search_regex,
            max_width.wrapping_sub(label_len + 1),
            ShortenDirection::Left,
            Some(SHORTENER),
        ));

        // Add place for cursor.
        bar_text.push(' ');

        bar_text
    }

    /// Draw preview for the currently highlighted `Hyperlink`.
    #[inline(never)]
    fn draw_hyperlink_preview(
        &mut self,
        config: &UiConfig,
        cursor_point: Option<Point>,
        display_offset: usize,
    ) {
        let num_cols = self.size_info.columns();
        let uris: Vec<_> = self
            .highlighted_hint
            .iter()
            .chain(&self.vi_highlighted_hint)
            .filter_map(|hint| hint.hyperlink().map(|hyperlink| hyperlink.uri()))
            .map(|uri| StrShortener::new(uri, num_cols, ShortenDirection::Right, Some(SHORTENER)))
            .collect();

        if uris.is_empty() {
            return;
        }

        // The maximum amount of protected lines including the ones we'll show preview on.
        let max_protected_lines = uris.len() * 2;

        // Lines we shouldn't show preview on, because it'll obscure the highlighted hint.
        let mut protected_lines = Vec::with_capacity(max_protected_lines);
        if self.size_info.screen_lines() > max_protected_lines {
            // Prefer to show preview even when it'll likely obscure the highlighted hint, when
            // there's no place left for it.
            protected_lines.push(self.hint_mouse_point.map(|point| point.line));
            protected_lines.push(cursor_point.map(|point| point.line));
        }

        // Find the line in viewport we can draw preview on without obscuring protected lines.
        let viewport_bottom = self.size_info.bottommost_line() - Line(display_offset as i32);
        let viewport_top = viewport_bottom - (self.size_info.screen_lines() - 1);
        let uri_lines = (viewport_top.0..=viewport_bottom.0)
            .rev()
            .map(|line| Some(Line(line)))
            .filter_map(|line| {
                if protected_lines.contains(&line) {
                    None
                } else {
                    protected_lines.push(line);
                    line
                }
            })
            .take(uris.len())
            .flat_map(|line| term::point_to_viewport(display_offset, Point::new(line, Column(0))));

        let fg = config.colors.footer_bar_foreground();
        let bg = config.colors.footer_bar_background();
        for (uri, point) in uris.into_iter().zip(uri_lines) {
            // Damage the uri preview.
            let damage = LineDamageBounds::new(point.line, point.column.0, num_cols);
            self.damage_tracker.frame().damage_line(damage);

            // Damage the uri preview for the next frame as well.
            self.damage_tracker.next_frame().damage_line(damage);

            self.renderer.draw_string(point, fg, bg, uri, &self.size_info, &mut self.glyph_cache);
        }
    }

    /// Draw current search regex.
    #[inline(never)]
    fn draw_search(&mut self, config: &UiConfig, text: &str) {
        self.draw_footer_text(config, text, self.size_info.screen_lines());
    }

    #[inline(never)]
    fn draw_footer_text(&mut self, config: &UiConfig, text: &str, line: usize) {
        // Assure text length is at least num_cols.
        let num_cols = self.size_info.columns();
        let text = format!("{text:<num_cols$}");

        let point = Point::new(line, Column(0));

        let fg = config.colors.footer_bar_foreground();
        let bg = config.colors.footer_bar_background();

        self.renderer.draw_string(
            point,
            fg,
            bg,
            text.chars(),
            &self.size_info,
            &mut self.glyph_cache,
        );
    }

    fn draw_string_with_flags(
        &mut self,
        point: Point<usize>,
        fg: Rgb,
        bg: Rgb,
        bg_alpha: f32,
        text: &str,
        size_info: &SizeInfo,
        flags: Flags,
    ) -> usize {
        let mut cells = Vec::with_capacity(text.chars().count());
        let mut column = point.column.0;

        for character in text.chars() {
            let width = character.width().unwrap_or(1);
            let cell_flags = if width == 2 { flags | Flags::WIDE_CHAR } else { flags };

            cells.push(crate::display::content::RenderableCell {
                point: Point::new(point.line, Column(column)),
                character,
                extra: None,
                flags: cell_flags,
                bg_alpha,
                fg,
                bg,
                underline: fg,
            });

            column += width;
        }

        self.renderer.draw_cells(size_info, &mut self.glyph_cache, cells.into_iter());
        column - point.column.0
    }

    #[inline(never)]
    fn draw_tab_bar(&mut self, config: &UiConfig, tab_titles: &[(String, bool)], line: usize) {
        let mut size_info = self.size_info;
        if config.tabs.tab_bar_edge == TabBarEdge::Top {
            size_info.padding_y -= size_info.cell_height();
        }

        self.renderer.set_viewport(&size_info);

        let num_cols = self.size_info.columns();
        let default_bar_bg = config.colors.primary.background * 0.8;
        let bar_bg = config.tabs.tab_bar_background.unwrap_or(default_bar_bg);
        let translucent_alpha = tab_bar_background_alpha(config.window_opacity());
        let rendered_bar_bg = if config.window_opacity() < 1.0 {
            darken_rgb(bar_bg, 0.5)
        } else {
            bar_bg
        };
        let visible_bar_bg =
            blend_rgb(config.colors.primary.background, rendered_bar_bg, translucent_alpha);
        let active_fg =
            config.tabs.active_tab_foreground.unwrap_or(config.colors.footer_bar_foreground());
        let active_bg =
            config.tabs.active_tab_background.unwrap_or(config.colors.footer_bar_background());
        let inactive_fg =
            config.tabs.inactive_tab_foreground.unwrap_or(config.colors.primary.foreground);
        let default_inactive_bg = if config.window_opacity() < 1.0 {
            darken_rgb(bar_bg, 0.82)
        } else {
            bar_bg
        };
        let inactive_bg = config.tabs.inactive_tab_background.unwrap_or(default_inactive_bg);

        let y = size_info.cell_height().mul_add(line as f32, size_info.padding_y()) as i32;
        let width = size_info.width() as i32;
        let height = size_info.cell_height() as i32;
        self.damage_tracker.frame().add_viewport_rect(&size_info, 0, y, width, height);
        self.damage_tracker.next_frame().add_viewport_rect(&size_info, 0, y, width, height);

        let metrics = self.glyph_cache.font_metrics();
        let mut rects = vec![RenderRect::new(
            0.,
            y as f32,
            width as f32,
            height as f32,
            rendered_bar_bg,
            translucent_alpha,
        )];

        if config.tabs.tab_bar_style == TabBarStyle::Slant {
            let mut column = 0usize;

            for (index, (title, active)) in tab_titles.iter().enumerate() {
                if column >= num_cols {
                    break;
                }

                let tab_bg = if *active {
                    active_bg
                } else {
                    inactive_bg
                };
                let rendered_tab_bg = if *active {
                    tab_bg
                } else {
                    blend_rgb(config.colors.primary.background, tab_bg, translucent_alpha)
                };
                let next_bg = tab_titles
                    .get(index + 1)
                    .map(|(_, active)| {
                        if *active {
                            active_bg
                        } else {
                            blend_rgb(config.colors.primary.background, inactive_bg, translucent_alpha)
                        }
                    })
                    .unwrap_or(visible_bar_bg);
                let needs_separator = rendered_tab_bg != next_bg;
                let reserve = usize::from(needs_separator);
                let visible: String = StrShortener::new(
                    &format!(" {title} "),
                    num_cols.saturating_sub(column + reserve),
                    ShortenDirection::Right,
                    Some(SHORTENER),
                )
                .collect();
                let width_cells: usize =
                    visible.chars().map(|character| character.width().unwrap_or(1)).sum();

                if width_cells == 0 {
                    break;
                }

                let body_x = size_info.padding_x() + size_info.cell_width() * column as f32;
                let body_width = size_info.cell_width() * width_cells as f32;
                rects.push(RenderRect::new(
                    body_x,
                    y as f32,
                    body_width,
                    height as f32,
                    rendered_tab_bg,
                    1.0,
                ));

                if needs_separator {
                    let separator_x = body_x + body_width;
                    add_slanted_separator_rects(
                        &mut rects,
                        separator_x,
                        y as f32,
                        size_info.cell_width(),
                        height as f32,
                        rendered_tab_bg,
                        next_bg,
                    );
                }

                self.tab_hit_boxes.push(TabHitBox {
                    index,
                    x: body_x as i32,
                    y,
                    width: (size_info.cell_width() * (width_cells + reserve) as f32) as i32,
                    height,
                });

                column += width_cells + reserve;
            }

            self.renderer.draw_rects(&size_info, &metrics, rects);

            let mut column = 0usize;
            for (index, (title, active)) in tab_titles.iter().enumerate() {
                if column >= num_cols {
                    break;
                }

                let (tab_fg, tab_bg, font_style) = if *active {
                    (active_fg, active_bg, config.tabs.active_tab_font_style)
                } else {
                    (inactive_fg, inactive_bg, config.tabs.inactive_tab_font_style)
                };
                let rendered_tab_bg = if *active {
                    tab_bg
                } else {
                    blend_rgb(config.colors.primary.background, tab_bg, translucent_alpha)
                };
                let next_bg = tab_titles
                    .get(index + 1)
                    .map(|(_, active)| {
                        if *active {
                            active_bg
                        } else {
                            blend_rgb(config.colors.primary.background, inactive_bg, translucent_alpha)
                        }
                    })
                    .unwrap_or(visible_bar_bg);
                let reserve = usize::from(rendered_tab_bg != next_bg);
                let visible: String = StrShortener::new(
                    &format!(" {title} "),
                    num_cols.saturating_sub(column + reserve),
                    ShortenDirection::Right,
                    Some(SHORTENER),
                )
                .collect();
                let width_cells: usize =
                    visible.chars().map(|character| character.width().unwrap_or(1)).sum();

                if width_cells == 0 {
                    break;
                }

                self.draw_string_with_flags(
                    Point::new(line, Column(column)),
                    tab_fg,
                    rendered_tab_bg,
                    0.0,
                    &visible,
                    &size_info,
                    tab_font_flags(font_style),
                );
                column += width_cells + reserve;
            }

            self.renderer.set_viewport(&self.size_info);
            return;
        }

        self.renderer.draw_rects(&size_info, &metrics, rects);
        let mut column = 0usize;
        for (index, (title, active)) in tab_titles.iter().enumerate() {
            if column >= num_cols {
                break;
            }

            let title = if config.tabs.tab_title_max_length == 0 {
                title.clone()
            } else {
                StrShortener::new(
                    title,
                    config.tabs.tab_title_max_length,
                    ShortenDirection::Right,
                    Some(SHORTENER),
                )
                .collect()
            };
            let label = format!(" {title} ");
            let reserve = match config.tabs.tab_bar_style {
                TabBarStyle::Powerline => 1,
                TabBarStyle::Hidden => 0,
                TabBarStyle::Separator | TabBarStyle::Fade => {
                    if index + 1 < tab_titles.len() {
                        config.tabs.tab_separator.chars().count()
                    } else {
                        0
                    }
                },
                TabBarStyle::Slant => 0,
            };
            let visible: String = StrShortener::new(
                &label,
                num_cols.saturating_sub(column + reserve),
                ShortenDirection::Right,
                Some(SHORTENER),
            )
            .collect();

            let (tab_fg, tab_bg, font_style) = if *active {
                (active_fg, active_bg, config.tabs.active_tab_font_style)
            } else {
                (inactive_fg, inactive_bg, config.tabs.inactive_tab_font_style)
            };
            let width_cells: usize =
                visible.chars().map(|character| character.width().unwrap_or(1)).sum();
            let hit_box_column = column;
            let bg_alpha = if *active {
                1.0
            } else if tab_bg == bar_bg {
                0.0
            } else {
                translucent_alpha
            };

            column += self.draw_string_with_flags(
                Point::new(line, Column(column)),
                tab_fg,
                tab_bg,
                bg_alpha,
                &visible,
                &size_info,
                tab_font_flags(font_style),
            );

            let separator_width = match config.tabs.tab_bar_style {
                TabBarStyle::Powerline => {
                    let next_bg = tab_titles
                        .get(index + 1)
                        .map(|(_, active)| if *active { active_bg } else { inactive_bg })
                        .unwrap_or(bar_bg);
                    let separator = if index + 1 < tab_titles.len() {
                        powerline_separator(config.tabs.tab_powerline_style)
                    } else {
                        trailing_powerline_separator(config.tabs.tab_powerline_style)
                    }
                    .to_string();
                    self.draw_string_with_flags(
                        Point::new(line, Column(column)),
                        tab_bg,
                        next_bg,
                        if next_bg == bar_bg { translucent_alpha } else { 1.0 },
                        &separator,
                        &size_info,
                        Flags::empty(),
                    )
                },
                TabBarStyle::Separator | TabBarStyle::Fade if index + 1 < tab_titles.len() => self
                    .draw_string_with_flags(
                        Point::new(line, Column(column)),
                        inactive_fg,
                        bar_bg,
                        translucent_alpha,
                        &config.tabs.tab_separator,
                        &size_info,
                        Flags::empty(),
                    ),
                _ => 0,
            };
            column += separator_width;

            self.tab_hit_boxes.push(TabHitBox {
                index,
                x: (size_info.padding_x() + size_info.cell_width() * hit_box_column as f32) as i32,
                y,
                width: (size_info.cell_width() * (width_cells + separator_width) as f32) as i32,
                height,
            });
        }

        self.renderer.set_viewport(&self.size_info);
    }

    /// Draw render timer.
    #[inline(never)]
    fn draw_render_timer(&mut self, config: &UiConfig) {
        if !config.debug.render_timer {
            return;
        }

        let timing = format!("{:.3} usec", self.meter.average());
        let point = Point::new(self.size_info.screen_lines().saturating_sub(2), Column(0));
        let fg = config.colors.primary.background;
        let bg = config.colors.normal.red;

        // Damage render timer for current and next frame.
        let damage = LineDamageBounds::new(point.line, point.column.0, timing.len());
        self.damage_tracker.frame().damage_line(damage);
        self.damage_tracker.next_frame().damage_line(damage);

        let glyph_cache = &mut self.glyph_cache;
        self.renderer.draw_string(point, fg, bg, timing.chars(), &self.size_info, glyph_cache);
    }

    /// Draw an indicator for the position of a line in history.
    #[inline(never)]
    fn draw_line_indicator(
        &mut self,
        config: &UiConfig,
        total_lines: usize,
        obstructed_column: Option<Column>,
        line: usize,
    ) {
        let columns = self.size_info.columns();
        let text = format!("[{}/{}]", line, total_lines - 1);
        let column = Column(self.size_info.columns().saturating_sub(text.len()));
        let point = Point::new(0, column);

        // Damage the line indicator for current and next frame.
        let damage = LineDamageBounds::new(point.line, point.column.0, columns - 1);
        self.damage_tracker.frame().damage_line(damage);
        self.damage_tracker.next_frame().damage_line(damage);

        let colors = &config.colors;
        let fg = colors.line_indicator.foreground.unwrap_or(colors.primary.background);
        let bg = colors.line_indicator.background.unwrap_or(colors.primary.foreground);

        // Do not render anything if it would obscure the vi mode cursor.
        if obstructed_column.is_none_or(|obstructed_column| obstructed_column < column) {
            let glyph_cache = &mut self.glyph_cache;
            self.renderer.draw_string(point, fg, bg, text.chars(), &self.size_info, glyph_cache);
        }
    }

    /// Highlight damaged rects.
    ///
    /// This function is for debug purposes only.
    fn highlight_damage(&self, render_rects: &mut Vec<RenderRect>) {
        for damage_rect in &self.damage_tracker.shape_frame_damage(self.size_info.into()) {
            let x = damage_rect.x as f32;
            let height = damage_rect.height as f32;
            let width = damage_rect.width as f32;
            let y = damage_y_to_viewport_y(&self.size_info, damage_rect) as f32;
            let render_rect = RenderRect::new(x, y, width, height, DAMAGE_RECT_COLOR, 0.5);

            render_rects.push(render_rect);
        }
    }

    /// Check whether a hint highlight needs to be cleared.
    fn validate_hint_highlights(&mut self, display_offset: usize) {
        let frame = self.damage_tracker.frame();
        let hints = [
            (&mut self.highlighted_hint, &mut self.highlighted_hint_age, true),
            (&mut self.vi_highlighted_hint, &mut self.vi_highlighted_hint_age, false),
        ];

        let num_lines = self.size_info.screen_lines();
        for (hint, hint_age, reset_mouse) in hints {
            let (start, end) = match hint {
                Some(hint) => (*hint.bounds().start(), *hint.bounds().end()),
                None => continue,
            };

            // Ignore hints that were created this frame.
            *hint_age += 1;
            if *hint_age == 1 {
                continue;
            }

            // Convert hint bounds to viewport coordinates.
            let start = term::point_to_viewport(display_offset, start)
                .filter(|point| point.line < num_lines)
                .unwrap_or_default();
            let end = term::point_to_viewport(display_offset, end)
                .filter(|point| point.line < num_lines)
                .unwrap_or_else(|| Point::new(num_lines - 1, self.size_info.last_column()));

            // Clear invalidated hints.
            if frame.intersects(start, end) {
                if reset_mouse {
                    self.window.set_mouse_cursor(CursorIcon::Default);
                }
                frame.mark_fully_damaged();
                *hint = None;
            }
        }
    }

    /// Request a new frame for a window on Wayland.
    fn request_frame(&mut self, scheduler: &mut Scheduler) {
        // Mark that we've used a frame.
        self.window.has_frame = false;

        // Get the display vblank interval.
        let monitor_vblank_interval = 1_000_000.
            / self
                .window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .unwrap_or(60_000) as f64;

        // Now convert it to micro seconds.
        let monitor_vblank_interval =
            Duration::from_micros((1000. * monitor_vblank_interval) as u64);

        let swap_timeout = self.frame_timer.compute_timeout(monitor_vblank_interval);

        let window_id = self.window.id();
        let timer_id = TimerId::new(Topic::Frame, window_id);
        let event = Event::new(EventType::Frame, window_id);

        scheduler.schedule(event, swap_timeout, false, timer_id);
    }
}

fn tab_font_flags(style: TabFontStyle) -> Flags {
    match style {
        TabFontStyle::Normal => Flags::empty(),
        TabFontStyle::Bold => Flags::BOLD,
        TabFontStyle::Italic => Flags::ITALIC,
        TabFontStyle::BoldItalic => Flags::BOLD_ITALIC,
    }
}

fn powerline_separator(style: TabPowerlineStyle) -> char {
    match style {
        TabPowerlineStyle::Angled => '\u{e0b0}',
        TabPowerlineStyle::Slanted => '\u{e0bc}',
        TabPowerlineStyle::Round => '\u{e0b4}',
    }
}

fn trailing_powerline_separator(style: TabPowerlineStyle) -> char {
    match style {
        TabPowerlineStyle::Angled => '\u{e0b0}',
        TabPowerlineStyle::Slanted => '\u{e0be}',
        TabPowerlineStyle::Round => '\u{e0b4}',
    }
}

fn tab_bar_background_alpha(window_opacity: f32) -> f32 {
    if window_opacity >= 1.0 {
        1.0
    } else {
        (window_opacity * 0.35).clamp(0.08, 0.35)
    }
}

fn blend_rgb(lhs: Rgb, rhs: Rgb, factor: f32) -> Rgb {
    let factor = factor.clamp(0.0, 1.0);
    let inv = 1.0 - factor;
    let (lr, lg, lb) = lhs.as_tuple();
    let (rr, rg, rb) = rhs.as_tuple();

    Rgb::new(
        (lr as f32 * inv + rr as f32 * factor).round() as u8,
        (lg as f32 * inv + rg as f32 * factor).round() as u8,
        (lb as f32 * inv + rb as f32 * factor).round() as u8,
    )
}

fn darken_rgb(color: Rgb, factor: f32) -> Rgb {
    let factor = factor.clamp(0.0, 1.0);
    let (r, g, b) = color.as_tuple();
    Rgb::new(
        (r as f32 * factor).round() as u8,
        (g as f32 * factor).round() as u8,
        (b as f32 * factor).round() as u8,
    )
}

fn add_slanted_separator_rects(
    rects: &mut Vec<RenderRect>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    left_color: Rgb,
    right_color: Rgb,
) {
    let strip_count = ((height * 2.0).round() as i32).max(1);
    let strip_height = height / strip_count as f32;

    for strip in 0..strip_count {
        let progress = (strip as f32 + 0.5) / strip_count as f32;
        let boundary = x + (1.0 - progress) * width;
        add_antialiased_strip(rects, x, y + strip as f32 * strip_height, boundary, strip_height, left_color);
        add_antialiased_strip(
            rects,
            boundary,
            y + strip as f32 * strip_height,
            x + width,
            strip_height,
            right_color,
        );
    }
}

fn add_antialiased_strip(
    rects: &mut Vec<RenderRect>,
    left: f32,
    y: f32,
    right: f32,
    height: f32,
    color: Rgb,
) {
    if right <= left || height <= 0.0 {
        return;
    }

    let left_floor = left.floor();
    let right_floor = right.floor();
    let left_partial = left.fract() > f32::EPSILON;
    let right_partial = right.fract() > f32::EPSILON;

    if left_floor == right_floor {
        rects.push(RenderRect::new(left_floor, y, 1.0, height, color, (right - left).clamp(0.0, 1.0)));
        return;
    }

    if left_partial {
        rects.push(RenderRect::new(
            left_floor,
            y,
            1.0,
            height,
            color,
            (left_floor + 1.0 - left).clamp(0.0, 1.0),
        ));
    }

    let full_start = if left_partial { left_floor + 1.0 } else { left_floor };
    let full_end = right_floor;
    if full_end > full_start {
        rects.push(RenderRect::new(full_start, y, full_end - full_start, height, color, 1.0));
    }

    if right_partial {
        rects.push(RenderRect::new(right_floor, y, 1.0, height, color, right.fract()));
    }
}

fn format_search_prompt(label: &str, value: &str, max_width: usize) -> String {
    let text = format!("{label}{value}_");
    format!("{text:<max_width$}")
}

impl Drop for Display {
    fn drop(&mut self) {
        // Switch OpenGL context before dropping, otherwise objects (like programs) from other
        // contexts might be deleted when dropping renderer.
        self.make_current();
        unsafe {
            ManuallyDrop::drop(&mut self.renderer);
            ManuallyDrop::drop(&mut self.context);
            ManuallyDrop::drop(&mut self.surface);
        }
    }
}

/// Input method state.
#[derive(Debug, Default)]
pub struct Ime {
    /// Whether the IME is enabled.
    enabled: bool,

    /// Current IME preedit.
    preedit: Option<Preedit>,
}

impl Ime {
    #[inline]
    pub fn set_enabled(&mut self, is_enabled: bool) {
        if is_enabled {
            self.enabled = is_enabled
        } else {
            // Clear state when disabling IME.
            *self = Default::default();
        }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[inline]
    pub fn set_preedit(&mut self, preedit: Option<Preedit>) {
        self.preedit = preedit;
    }

    #[inline]
    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    /// The preedit text.
    text: String,

    /// Byte offset for cursor start into the preedit text.
    ///
    /// `None` means that the cursor is invisible.
    cursor_byte_offset: Option<(usize, usize)>,

    /// The cursor offset from the end of the start of the preedit in char width.
    cursor_end_offset: Option<(usize, usize)>,
}

impl Preedit {
    pub fn new(text: String, cursor_byte_offset: Option<(usize, usize)>) -> Self {
        let cursor_end_offset = if let Some(byte_offset) = cursor_byte_offset {
            // Convert byte offset into char offset.
            let start_to_end_offset =
                text[byte_offset.0..].chars().fold(0, |acc, ch| acc + ch.width().unwrap_or(1));
            let end_to_end_offset =
                text[byte_offset.1..].chars().fold(0, |acc, ch| acc + ch.width().unwrap_or(1));

            Some((start_to_end_offset, end_to_end_offset))
        } else {
            None
        };

        Self { text, cursor_byte_offset, cursor_end_offset }
    }
}

/// Pending renderer updates.
///
/// All renderer updates are cached to be applied just before rendering, to avoid platform-specific
/// rendering issues.
#[derive(Debug, Default, Copy, Clone)]
pub struct RendererUpdate {
    /// Should resize the window.
    resize: bool,

    /// Clear font caches.
    clear_font_cache: bool,
}

/// The frame timer state.
pub struct FrameTimer {
    /// Base timestamp used to compute sync points.
    base: Instant,

    /// The last timestamp we synced to.
    last_synced_timestamp: Instant,

    /// The refresh rate we've used to compute sync timestamps.
    refresh_interval: Duration,
}

impl FrameTimer {
    pub fn new() -> Self {
        let now = Instant::now();
        Self { base: now, last_synced_timestamp: now, refresh_interval: Duration::ZERO }
    }

    /// Compute the delay that we should use to achieve the target frame
    /// rate.
    pub fn compute_timeout(&mut self, refresh_interval: Duration) -> Duration {
        let now = Instant::now();

        // Handle refresh rate change.
        if self.refresh_interval != refresh_interval {
            self.base = now;
            self.last_synced_timestamp = now;
            self.refresh_interval = refresh_interval;
            return refresh_interval;
        }

        let next_frame = self.last_synced_timestamp + self.refresh_interval;

        if next_frame < now {
            // Redraw immediately if we haven't drawn in over `refresh_interval` microseconds.
            let elapsed_micros = (now - self.base).as_micros() as u64;
            let refresh_micros = self.refresh_interval.as_micros() as u64;
            self.last_synced_timestamp =
                now - Duration::from_micros(elapsed_micros % refresh_micros);
            Duration::ZERO
        } else {
            // Redraw on the next `refresh_interval` clock tick.
            self.last_synced_timestamp = next_frame;
            next_frame - now
        }
    }
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
    scale_factor: f32,
) -> PhysicalSize<u32> {
    let padding = config.window.padding(scale_factor);

    let grid_width = cell_width * dimensions.columns.max(MIN_COLUMNS) as f32;
    let grid_height = cell_height * dimensions.lines.max(MIN_SCREEN_LINES) as f32;

    let width = (padding.0).mul_add(2., grid_width).floor();
    let height = (padding.1).mul_add(2., grid_height).floor();

    PhysicalSize::new(width as u32, height as u32)
}
