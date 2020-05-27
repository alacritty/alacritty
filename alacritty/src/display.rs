// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The display subsystem including window management, font rasterization, and
//! GPU drawing.
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
#[cfg(not(any(target_os = "macos", windows)))]
use wayland_client::{Display as WaylandDisplay, EventQueue};

#[cfg(target_os = "macos")]
use font::set_font_smoothing;
use font::{self, Rasterize};

use alacritty_terminal::config::{Font, StartupMode};
use alacritty_terminal::event::{Event, OnResize};
use alacritty_terminal::index::Line;
use alacritty_terminal::message_bar::MessageBuffer;
use alacritty_terminal::meter::Meter;
use alacritty_terminal::selection::Selection;
use alacritty_terminal::term::color::Rgb;
use alacritty_terminal::term::{RenderableCell, SizeInfo, Term, TermMode};

use crate::config::Config;
use crate::event::{DisplayUpdate, Mouse};
use crate::renderer::rects::{RenderLines, RenderRect};
use crate::renderer::{self, GlyphCache, QuadRenderer};
use crate::url::{Url, Urls};
use crate::window::{self, Window};

#[derive(Debug)]
pub enum Error {
    /// Error with window management.
    Window(window::Error),

    /// Error dealing with fonts.
    Font(font::Error),

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

impl From<font::Error> for Error {
    fn from(val: font::Error) -> Self {
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
    pub fn new(config: &Config, event_loop: &EventLoop<Event>) -> Result<Display, Error> {
        // Guess DPR based on first monitor.
        let estimated_dpr =
            event_loop.available_monitors().next().map(|m| m.scale_factor()).unwrap_or(1.);

        // Guess the target window dimensions.
        let metrics = GlyphCache::static_metrics(config.font.clone(), estimated_dpr)?;
        let (cell_width, cell_height) = compute_cell_size(config, &metrics);
        let dimensions =
            GlyphCache::calculate_dimensions(config, estimated_dpr, cell_width, cell_height);

        debug!("Estimated DPR: {}", estimated_dpr);
        debug!("Estimated Cell Size: {} x {}", cell_width, cell_height);
        debug!("Estimated Dimensions: {:?}", dimensions);

        #[cfg(not(any(target_os = "macos", windows)))]
        let mut wayland_event_queue = None;

        // Initialize Wayland event queue, to handle Wayland callbacks.
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            if let Some(display) = event_loop.wayland_display() {
                let display = unsafe { WaylandDisplay::from_external_display(display as _) };
                wayland_event_queue = Some(display.create_event_queue());
            }
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

        let mut padding_x = f32::from(config.window.padding.x) * dpr as f32;
        let mut padding_y = f32::from(config.window.padding.y) * dpr as f32;

        if let Some((width, height)) =
            GlyphCache::calculate_dimensions(config, dpr, cell_width, cell_height)
        {
            let PhysicalSize { width: w, height: h } = window.inner_size();
            if w == width && h == height {
                info!("Estimated DPR correctly, skipping resize");
            } else {
                window.set_inner_size(PhysicalSize::new(width, height));
            }
        } else if config.window.dynamic_padding {
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
        renderer.with_api(&config, &size_info, |api| {
            api.clear(background_color);
        });

        // Set subpixel anti-aliasing.
        #[cfg(target_os = "macos")]
        set_font_smoothing(config.font.use_thin_strokes());

        #[cfg(not(any(target_os = "macos", windows)))]
        let is_x11 = event_loop.is_x11();

        #[cfg(not(any(target_os = "macos", windows)))]
        {
            // On Wayland we can safely ignore this call, since the window isn't visible until you
            // actually draw something into it and commit those changes.
            if is_x11 {
                window.swap_buffers();
                renderer.with_api(&config, &size_info, |api| {
                    api.finish();
                });
            }
        }

        window.set_visible(true);

        // Set window position.
        //
        // TODO: replace `set_position` with `with_position` once available.
        // Upstream issue: https://github.com/rust-windowing/winit/issues/806.
        if let Some(position) = config.window.position {
            window.set_outer_position(PhysicalPosition::from((position.x, position.y)));
        }

        #[allow(clippy::single_match)]
        match config.window.startup_mode() {
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
        let font = config.font.clone();
        let rasterizer = font::Rasterizer::new(dpr as f32, config.font.use_thin_strokes())?;

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
    fn update_glyph_cache(&mut self, config: &Config, font: Font) {
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
        config: &Config,
        update_pending: DisplayUpdate,
    ) {
        // Update font size and cell dimensions.
        if let Some(font) = update_pending.font {
            self.update_glyph_cache(config, font);
        } else if update_pending.cursor {
            self.clear_glyph_cache();
        }

        let cell_width = self.size_info.cell_width;
        let cell_height = self.size_info.cell_height;

        // Recalculate padding.
        let mut padding_x = f32::from(config.window.padding.x) * self.size_info.dpr as f32;
        let mut padding_y = f32::from(config.window.padding.y) * self.size_info.dpr as f32;

        // Update the window dimensions.
        if let Some(size) = update_pending.dimensions {
            // Ensure we have at least one column and row.
            self.size_info.width = (size.width as f32).max(cell_width + 2. * padding_x);
            self.size_info.height = (size.height as f32).max(cell_height + 2. * padding_y);
        }

        // Distribute excess padding equally on all sides.
        if config.window.dynamic_padding {
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
    ) {
        let grid_cells: Vec<RenderableCell> = terminal.renderable_cells(config).collect();
        let visual_bell_intensity = terminal.visual_bell.intensity();
        let background_color = terminal.background_color();
        let metrics = self.glyph_cache.font_metrics();
        let glyph_cache = &mut self.glyph_cache;
        let size_info = self.size_info;

        let selection = !terminal.selection().as_ref().map(Selection::is_empty).unwrap_or(true);
        let mouse_mode = terminal.mode().intersects(TermMode::MOUSE_MODE)
            && !terminal.mode().contains(TermMode::VI);

        let vi_mode_cursor = if terminal.mode().contains(TermMode::VI) {
            Some(terminal.vi_mode_cursor)
        } else {
            None
        };

        // Update IME position.
        #[cfg(not(windows))]
        self.window.update_ime_position(&terminal, &self.size_info);

        // Drop terminal as early as possible to free lock.
        drop(terminal);

        self.renderer.with_api(&config, &size_info, |api| {
            api.clear(background_color);
        });

        let mut lines = RenderLines::new();
        let mut urls = Urls::new();

        // Draw grid.
        {
            let _sampler = self.meter.sampler();

            self.renderer.with_api(&config, &size_info, |mut api| {
                // Iterate over all non-empty cells in the grid.
                for cell in grid_cells {
                    // Update URL underlines.
                    urls.update(size_info.cols().0, cell);

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
                config.visual_bell.color,
                visual_bell_intensity as f32,
            );
            rects.push(visual_bell_rect);
        }

        if let Some(message) = message_buffer.message() {
            let text = message.text(&size_info);

            // Create a new rectangle for the background.
            let start_line = size_info.lines().0 - text.len();
            let y = size_info.cell_height.mul_add(start_line as f32, size_info.padding_y);
            let message_bar_rect =
                RenderRect::new(0., y, size_info.width, size_info.height - y, message.color(), 1.);

            // Push message_bar in the end, so it'll be above all other content.
            rects.push(message_bar_rect);

            // Draw rectangles.
            self.renderer.draw_rects(&size_info, rects);

            // Relay messages to the user.
            let mut offset = 1;
            for message_text in text.iter().rev() {
                self.renderer.with_api(&config, &size_info, |mut api| {
                    api.render_string(
                        &message_text,
                        Line(size_info.lines().saturating_sub(offset)),
                        glyph_cache,
                        None,
                    );
                });
                offset += 1;
            }
        } else {
            // Draw rectangles.
            self.renderer.draw_rects(&size_info, rects);
        }

        // Draw render timer.
        if config.render_timer() {
            let timing = format!("{:.3} usec", self.meter.average());
            let color = Rgb { r: 0xd5, g: 0x4e, b: 0x53 };
            self.renderer.with_api(&config, &size_info, |mut api| {
                api.render_string(&timing[..], size_info.lines() - 2, glyph_cache, Some(color));
            });
        }

        // Frame event should be requested before swaping buffers, since it requires surface
        // `commit`, which is done by swap buffers under the hood.
        #[cfg(not(any(target_os = "macos", windows)))]
        self.request_frame(&self.window);

        self.window.swap_buffers();

        #[cfg(not(any(target_os = "macos", windows)))]
        {
            if self.is_x11 {
                // On X11 `swap_buffers` does not block for vsync. However the next OpenGl command
                // will block to synchronize (this is `glClear` in Alacritty), which causes a
                // permanent one frame delay.
                self.renderer.with_api(&config, &size_info, |api| {
                    api.finish();
                });
            }
        }
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
fn compute_cell_size(config: &Config, metrics: &font::Metrics) -> (f32, f32) {
    let offset_x = f64::from(config.font.offset.x);
    let offset_y = f64::from(config.font.offset.y);
    (
        ((metrics.average_advance + offset_x) as f32).floor().max(1.),
        ((metrics.line_height + offset_y) as f32).floor().max(1.),
    )
}
