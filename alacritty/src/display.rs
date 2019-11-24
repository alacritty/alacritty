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
use futures::future::lazy;
use futures::sync::mpsc as futures_mpsc;
use futures::sync::oneshot;
use std::f64;
use std::fmt;
use std::time::Instant;
use std::time::UNIX_EPOCH;
use tokio::prelude::*;
use tokio::runtime::current_thread;

use glutin::dpi::{PhysicalPosition, PhysicalSize};
use glutin::event::ModifiersState;
use glutin::event_loop::EventLoop;
use glutin::window::CursorIcon;
use log::{debug, error, info};
use parking_lot::MutexGuard;

use font::{self, Rasterize};

use alacritty_terminal::config::{Font, StartupMode};
use alacritty_terminal::event::{Event, OnResize};
use alacritty_terminal::index::Line;
use alacritty_terminal::message_bar::MessageBuffer;
use alacritty_terminal::meter::Meter;
use alacritty_terminal::renderer::rects::{RenderLines, RenderRect};
use alacritty_terminal::renderer::{self, GlyphCache, QuadRenderer};
use alacritty_terminal::selection::Selection;
use alacritty_terminal::term::color::Rgb;
use alacritty_terminal::term::{RenderableCell, SizeInfo, Term, TermMode};

use crate::config::Config;
use crate::event::{DisplayUpdate, Mouse};
use crate::url::{Url, Urls};
use crate::window::{self, Window};

#[derive(Debug)]
pub enum Error {
    /// Error with window management
    Window(window::Error),

    /// Error dealing with fonts
    Font(font::Error),

    /// Error in renderer
    Render(renderer::Error),

    /// Error during buffer swap
    ContextError(glutin::ContextError),
}

impl std::error::Error for Error {
    fn cause(&self) -> Option<&dyn (std::error::Error)> {
        match *self {
            Error::Window(ref err) => Some(err),
            Error::Font(ref err) => Some(err),
            Error::Render(ref err) => Some(err),
            Error::ContextError(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Window(ref err) => err.description(),
            Error::Font(ref err) => err.description(),
            Error::Render(ref err) => err.description(),
            Error::ContextError(ref err) => err.description(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Window(ref err) => err.fmt(f),
            Error::Font(ref err) => err.fmt(f),
            Error::Render(ref err) => err.fmt(f),
            Error::ContextError(ref err) => err.fmt(f),
        }
    }
}

impl From<window::Error> for Error {
    fn from(val: window::Error) -> Error {
        Error::Window(val)
    }
}

impl From<font::Error> for Error {
    fn from(val: font::Error) -> Error {
        Error::Font(val)
    }
}

impl From<renderer::Error> for Error {
    fn from(val: renderer::Error) -> Error {
        Error::Render(val)
    }
}

impl From<glutin::ContextError> for Error {
    fn from(val: glutin::ContextError) -> Error {
        Error::ContextError(val)
    }
}

/// The display wraps a window, font rasterizer, and GPU renderer
pub struct Display {
    pub size_info: SizeInfo,
    pub window: Window,
    pub urls: Urls,

    /// Currently highlighted URL.
    pub highlighted_url: Option<Url>,

    renderer: QuadRenderer,
    glyph_cache: GlyphCache,
    meter: Meter,
    charts_last_drawn: u64,
}

impl Display {
    pub fn new(config: &Config, event_loop: &EventLoop<Event>) -> Result<Display, Error> {
        // Guess DPR based on first monitor
        let estimated_dpr =
            event_loop.available_monitors().next().map(|m| m.hidpi_factor()).unwrap_or(1.);

        // Guess the target window dimensions
        let metrics = GlyphCache::static_metrics(config.font.clone(), estimated_dpr)?;
        let (cell_width, cell_height) = compute_cell_size(config, &metrics);
        let dimensions =
            GlyphCache::calculate_dimensions(config, estimated_dpr, cell_width, cell_height);

        debug!("Estimated DPR: {}", estimated_dpr);
        debug!("Estimated Cell Size: {} x {}", cell_width, cell_height);
        debug!("Estimated Dimensions: {:?}", dimensions);

        // Create the window where Alacritty will be displayed
        let logical = dimensions.map(|d| PhysicalSize::new(d.0, d.1).to_logical(estimated_dpr));

        // Spawn window
        let mut window = Window::new(event_loop, &config, logical)?;

        let dpr = window.hidpi_factor();
        info!("Device pixel ratio: {}", dpr);

        // get window properties for initializing the other subsystems
        let mut viewport_size = window.inner_size().to_physical(dpr);

        // Create renderer
        let mut renderer = QuadRenderer::new()?;

        let (glyph_cache, cell_width, cell_height) =
            Self::new_glyph_cache(dpr, &mut renderer, config)?;

        let mut padding_x = f32::from(config.window.padding.x) * dpr as f32;
        let mut padding_y = f32::from(config.window.padding.y) * dpr as f32;

        if let Some((width, height)) =
            GlyphCache::calculate_dimensions(config, dpr, cell_width, cell_height)
        {
            let PhysicalSize { width: w, height: h } = window.inner_size().to_physical(dpr);
            if (w - width).abs() < f64::EPSILON && (h - height).abs() < f64::EPSILON {
                info!("Estimated DPR correctly, skipping resize");
            } else {
                viewport_size = PhysicalSize::new(width, height);
                window.set_inner_size(viewport_size.to_logical(dpr));
            }
        } else if config.window.dynamic_padding {
            // Make sure additional padding is spread evenly
            padding_x = dynamic_padding(padding_x, viewport_size.width as f32, cell_width);
            padding_y = dynamic_padding(padding_y, viewport_size.height as f32, cell_height);
        }

        padding_x = padding_x.floor();
        padding_y = padding_y.floor();

        info!("Cell Size: {} x {}", cell_width, cell_height);
        info!("Padding: {} x {}", padding_x, padding_y);

        // Create new size with at least one column and row
        let size_info = SizeInfo {
            dpr,
            width: (viewport_size.width as f32).max(cell_width + 2. * padding_x),
            height: (viewport_size.height as f32).max(cell_height + 2. * padding_y),
            cell_width,
            cell_height,
            padding_x,
            padding_y,
        };

        // Update OpenGL projection
        renderer.resize(&size_info);

        // Clear screen
        let background_color = config.colors.primary.background;
        renderer.with_api(&config, &size_info, |api| {
            api.clear(background_color);
        });

        // We should call `clear` when window is offscreen, so when `window.show()` happens it
        // would be with background color instead of uninitialized surface.
        window.swap_buffers();

        window.set_visible(true);

        // Set window position
        //
        // TODO: replace `set_position` with `with_position` once available
        // Upstream issue: https://github.com/tomaka/winit/issues/806
        if let Some(position) = config.window.position {
            let physical = PhysicalPosition::from((position.x, position.y));
            let logical = physical.to_logical(dpr);
            window.set_outer_position(logical);
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

        Ok(Display {
            window,
            renderer,
            glyph_cache,
            meter: Meter::new(),
            size_info,
            urls: Urls::new(),
            highlighted_url: None,
            charts_last_drawn: 0u64,
        })
    }

    fn new_glyph_cache(
        dpr: f64,
        renderer: &mut QuadRenderer,
        config: &Config,
    ) -> Result<(GlyphCache, f32, f32), Error> {
        let font = config.font.clone();
        let rasterizer = font::Rasterizer::new(dpr as f32, config.font.use_thin_strokes())?;

        // Initialize glyph cache
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

    /// Update font size and cell dimensions
    fn update_glyph_cache(&mut self, config: &Config, font: Font) {
        let size_info = &mut self.size_info;
        let cache = &mut self.glyph_cache;

        self.renderer.with_loader(|mut api| {
            let _ = cache.update_font_size(font, size_info.dpr, &mut api);
        });

        // Update cell size
        let (cell_width, cell_height) = compute_cell_size(config, &self.glyph_cache.font_metrics());
        size_info.cell_width = cell_width;
        size_info.cell_height = cell_height;
    }

    /// Process update events
    pub fn handle_update<T>(
        &mut self,
        terminal: &mut Term<T>,
        pty_resize_handle: &mut dyn OnResize,
        message_buffer: &MessageBuffer,
        config: &Config,
        update_pending: DisplayUpdate,
        charts_tx: futures_mpsc::Sender<alacritty_charts::async_utils::AsyncChartTask>,
        tokio_handle: current_thread::Handle,
    ) {
        // Update font size and cell dimensions
        if let Some(font) = update_pending.font {
            self.update_glyph_cache(config, font);
        }

        let cell_width = self.size_info.cell_width;
        let cell_height = self.size_info.cell_height;

        // Recalculate padding
        let mut padding_x = f32::from(config.window.padding.x) * self.size_info.dpr as f32;
        let mut padding_y = f32::from(config.window.padding.y) * self.size_info.dpr as f32;

        // Update the window dimensions
        if let Some(size) = update_pending.dimensions {
            // Ensure we have at least one column and row
            self.size_info.width = (size.width as f32).max(cell_width + 2. * padding_x);
            self.size_info.height = (size.height as f32).max(cell_height + 2. * padding_y);
        }

        // Distribute excess padding equally on all sides
        if config.window.dynamic_padding {
            padding_x = dynamic_padding(padding_x, self.size_info.width, cell_width);
            padding_y = dynamic_padding(padding_y, self.size_info.height, cell_height);
        }

        self.size_info.padding_x = padding_x.floor() as f32;
        self.size_info.padding_y = padding_y.floor() as f32;

        let mut pty_size = self.size_info;

        // Subtract message bar lines from pty size
        if let Some(message) = message_buffer.message() {
            let lines = message.text(&self.size_info).len();
            pty_size.height -= pty_size.cell_height * lines as f32;
        }

        // Resize PTY
        pty_resize_handle.on_resize(&pty_size);

        // Resize terminal
        terminal.resize(&pty_size);

        // Resize renderer
        let physical =
            PhysicalSize::new(f64::from(self.size_info.width), f64::from(self.size_info.height));
        self.renderer.resize(&self.size_info);
        self.window.resize(physical);
        let (height, width) = (self.size_info.height, self.size_info.width);
        let (chart_resize_tx, chart_resize_rx) = oneshot::channel();
        let send_display_size = charts_tx
            .send(alacritty_charts::async_utils::AsyncChartTask::ChangeDisplaySize(
                height,
                width,
                padding_y,
                padding_x,
                chart_resize_tx,
            ))
            .map_err(|e| error!("Sending ChangeDisplaySize Task: err={:?}", e))
            .and_then(move |_res| {
                debug!(
                    "Sent ChangeDisplaySize Task height: {}, width: {}, padding_y: {}, padding_x: \
                     {}",
                    height, width, padding_y, padding_x
                );
                Ok(())
            });
        tokio_handle
            .spawn(lazy(move || send_display_size))
            .expect("Unable to queue async task for send_display_size");
        match chart_resize_rx.map(|x| x).wait() {
            Ok(_) => {
                debug!("Got response from ChangeDisplaySize Task.");
            },
            Err(err) => {
                error!("Error response from SendMetricsOpenGLData Task: {:?}", err);
            },
        }
    }

    /// Draw the screen
    ///
    /// A reference to Term whose state is being drawn must be provided.
    ///
    /// This call may block if vsync is enabled
    pub fn draw<T>(
        &mut self,
        terminal: MutexGuard<'_, Term<T>>,
        message_buffer: &MessageBuffer,
        config: &Config,
        mouse: &Mouse,
        mods: ModifiersState,
        charts_tx: futures_mpsc::Sender<alacritty_charts::async_utils::AsyncChartTask>,
        tokio_handle: current_thread::Handle,
    ) {
        let grid_cells: Vec<RenderableCell> = terminal.renderable_cells(config).collect();
        let visual_bell_intensity = terminal.visual_bell.intensity();
        let background_color = terminal.background_color();
        let metrics = self.glyph_cache.font_metrics();
        let glyph_cache = &mut self.glyph_cache;
        let size_info = self.size_info;

        let selection = !terminal.selection().as_ref().map(Selection::is_empty).unwrap_or(true);
        let mouse_mode = terminal.mode().intersects(TermMode::MOUSE_MODE);

        // Update IME position
        #[cfg(not(windows))]
        self.window.update_ime_position(&terminal, &self.size_info);

        // Drop terminal as early as possible to free lock
        drop(terminal);

        self.renderer.with_api(&config, &size_info, |api| {
            api.clear(background_color);
        });

        let mut lines = RenderLines::new();
        let mut urls = Urls::new();

        // Draw grid
        {
            let _sampler = self.meter.sampler();

            self.renderer.with_api(&config, &size_info, |mut api| {
                // Iterate over all non-empty cells in the grid
                for cell in grid_cells {
                    // Update URL underlines
                    urls.update(size_info.cols().0, cell);

                    // Update underline/strikeout
                    lines.update(cell);

                    // Draw the cell
                    api.render_cell(cell, glyph_cache);
                }
            });
        }

        let mut rects = lines.rects(&metrics, &size_info);

        // Update visible URLs
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

        // Push visual bell after url/underline/strikeout rects
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

            // Create a new rectangle for the background
            let start_line = size_info.lines().0 - text.len();
            let y = size_info.cell_height.mul_add(start_line as f32, size_info.padding_y);
            let message_bar_rect =
                RenderRect::new(0., y, size_info.width, size_info.height - y, message.color(), 1.);

            // Push message_bar in the end, so it'll be above all other content
            rects.push(message_bar_rect);

            // Draw rectangles
            self.renderer.draw_rects(&size_info, rects);

            // Relay messages to the user
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
            // Draw rectangles
            self.renderer.draw_rects(&size_info, rects);
        }
        // Draw the charts
        for chart_idx in 0..config.charts.len() {
            debug!("draw: Drawing chart: {}", config.charts[chart_idx].name);
            for decoration_idx in 0..config.charts[chart_idx].decorations.len() {
                let alpha = config.charts[chart_idx].decorations[decoration_idx].alpha();
                self.renderer.draw_charts_line(
                    &size_info,
                    &alacritty_charts::async_utils::get_metric_opengl_vecs(
                        charts_tx.clone(),
                        chart_idx,
                        decoration_idx,
                        "decoration",
                        tokio_handle.clone(),
                    ),
                    Rgb {
                        r: config.charts[chart_idx].decorations[decoration_idx].color().r,
                        g: config.charts[chart_idx].decorations[decoration_idx].color().g,
                        b: config.charts[chart_idx].decorations[decoration_idx].color().b,
                    },
                    alpha,
                );
            }
            for series_idx in 0..config.charts[chart_idx].sources.len() {
                let alpha = config.charts[chart_idx].sources[series_idx].alpha();
                self.renderer.draw_charts_line(
                    &size_info,
                    &alacritty_charts::async_utils::get_metric_opengl_vecs(
                        charts_tx.clone(),
                        chart_idx,
                        series_idx,
                        "metric_data",
                        tokio_handle.clone(),
                    ),
                    Rgb {
                        r: config.charts[chart_idx].sources[series_idx].color().r,
                        g: config.charts[chart_idx].sources[series_idx].color().g,
                        b: config.charts[chart_idx].sources[series_idx].color().b,
                    },
                    alpha,
                );
            }
            let chart_last_drawn =
                std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        }

        // Draw render timer
        if config.render_timer() {
            let timing = format!("{:.3} usec", self.meter.average());
            let color = Rgb { r: 0xd5, g: 0x4e, b: 0x53 };
            self.renderer.with_api(&config, &size_info, |mut api| {
                api.render_string(&timing[..], size_info.lines() - 2, glyph_cache, Some(color));
            });
        }

        self.window.swap_buffers();
    }
}

/// Calculate padding to spread it evenly around the terminal content
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
