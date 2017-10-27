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
use std::sync::mpsc;

use parking_lot::{MutexGuard};

use Rgb;
use cli;
use config::Config;
use font::{self, Rasterize};
use meter::Meter;
use renderer::{self, GlyphCache, QuadRenderer};
use selection::Selection;
use term::{Term, SizeInfo};

use window::{self, Size, Pixels, Window, SetInnerSize};

#[derive(Debug)]
pub enum Error {
    /// Error with window management
    Window(window::Error),

    /// Error dealing with fonts
    Font(font::Error),

    /// Error in renderer
    Render(renderer::Error),
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::Window(ref err) => Some(err),
            Error::Font(ref err) => Some(err),
            Error::Render(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Window(ref err) => err.description(),
            Error::Font(ref err) => err.description(),
            Error::Render(ref err) => err.description(),
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::Window(ref err) => err.fmt(f),
            Error::Font(ref err) => err.fmt(f),
            Error::Render(ref err) => err.fmt(f),
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

/// The display wraps a window, font rasterizer, and GPU renderer
pub struct Display {
    window: Window,
    renderer: QuadRenderer,
    glyph_cache: GlyphCache,
    render_timer: bool,
    rx: mpsc::Receiver<(u32, u32)>,
    tx: mpsc::Sender<(u32, u32)>,
    meter: Meter,
    font_size_modifier: i8,
    size_info: SizeInfo,
    last_background_color: Rgb,
}

/// Can wakeup the render loop from other threads
pub struct Notifier(window::Proxy);

/// Types that are interested in when the display is resized
pub trait OnResize {
    fn on_resize(&mut self, size: &SizeInfo);
}

impl Notifier {
    pub fn notify(&self) {
        self.0.wakeup_event_loop();
    }
}

impl Display {
    pub fn notifier(&self) -> Notifier {
        Notifier(self.window.create_window_proxy())
    }

    pub fn update_config(&mut self, config: &Config) {
        self.render_timer = config.render_timer();
    }

    /// Get size info about the display
    pub fn size(&self) -> &SizeInfo {
        &self.size_info
    }

    pub fn new(
        config: &Config,
        options: &cli::Options,
    ) -> Result<Display, Error> {
        // Extract some properties from config
        let render_timer = config.render_timer();

        // Create the window where Alacritty will be displayed
        let mut window = Window::new(&options.title)?;

        // get window properties for initializing the other subsytems
        let size = window.inner_size_pixels()
            .expect("glutin returns window size");
        let dpr = window.hidpi_factor();

        info!("device_pixel_ratio: {}", dpr);

        // Create renderer
        let mut renderer = QuadRenderer::new(&config, size)?;

        let (glyph_cache, cell_width, cell_height) =
            Self::new_glyph_cache(&window, &mut renderer, config, 0)?;

        // Resize window to specified dimensions
        let dimensions = options.dimensions()
            .unwrap_or_else(|| config.dimensions());
        let width = cell_width as u32 * dimensions.columns_u32();
        let height = cell_height as u32 * dimensions.lines_u32();
        let size = Size { width: Pixels(width), height: Pixels(height) };
        info!("set_inner_size: {}", size);

        let viewport_size = Size {
            width: Pixels(width + 2 * config.padding().x as u32),
            height: Pixels(height + 2 * config.padding().y as u32),
        };
        window.set_inner_size(&viewport_size);
        renderer.resize(viewport_size.width.0 as _, viewport_size.height.0 as _);
        info!("Cell Size: ({} x {})", cell_width, cell_height);

        let size_info = SizeInfo {
            width: viewport_size.width.0 as f32,
            height: viewport_size.height.0 as f32,
            cell_width: cell_width as f32,
            cell_height: cell_height as f32,
            padding_x: config.padding().x.floor(),
            padding_y: config.padding().y.floor(),
        };

        // Channel for resize events
        //
        // macOS has a callback for getting resize events, the channel is used
        // to queue resize events until the next draw call. Unfortunately, it
        // seems that the event loop is blocked until the window is done
        // resizing. If any drawing were to happen during a resize, it would
        // need to be in the callback.
        let (tx, rx) = mpsc::channel();

        // Clear screen
        let background_color = config.colors().primary.background;
        renderer.with_api(config, &size_info, 0. /* visual bell intensity */, |api| {
            api.clear(background_color);
        });

        Ok(Display {
            window: window,
            renderer: renderer,
            glyph_cache: glyph_cache,
            render_timer: render_timer,
            tx: tx,
            rx: rx,
            meter: Meter::new(),
            font_size_modifier: 0,
            size_info: size_info,
            last_background_color: background_color,
        })
    }

    fn new_glyph_cache(window : &Window, renderer : &mut QuadRenderer,
                       config: &Config, font_size_delta: i8)
        -> Result<(GlyphCache, f32, f32), Error>
    {
        let font = config.font().clone().with_size_delta(font_size_delta as f32);
        let dpr = window.hidpi_factor();
        let rasterizer = font::Rasterizer::new(
                font.options.clone(),
                dpr,
                config.use_thin_strokes()
            )?;

        // Initialize glyph cache
        let glyph_cache = {
            info!("Initializing glyph cache");
            let init_start = ::std::time::Instant::now();

            let cache = renderer.with_loader(|mut api| {
                GlyphCache::new(rasterizer, &font, &mut api)
            })?;

            let stop = init_start.elapsed();
            let stop_f = stop.as_secs() as f64 + stop.subsec_nanos() as f64 / 1_000_000_000f64;
            info!("Finished initializing glyph cache in {}", stop_f);

            cache
        };

        // Need font metrics to resize the window properly. This suggests to me the
        // font metrics should be computed before creating the window in the first
        // place so that a resize is not needed.
        let metrics = glyph_cache.font_metrics();
        let cell_width = (metrics.average_advance + font.offset().x as f64) as u32;
        let cell_height = (metrics.line_height + font.offset().y as f64) as u32;

        return Ok((glyph_cache, cell_width as f32, cell_height as f32));
    }

    pub fn update_glyph_cache(&mut self, config: &Config, font_size_delta: i8) {
        let cache = &mut self.glyph_cache;
        self.renderer.with_loader(|mut api| {
            let _ = cache.update_font_size(config.font(), font_size_delta, &mut api);
        });

        let metrics = cache.font_metrics();
        self.size_info.cell_width = ((metrics.average_advance + config.font().offset().x as f64) as f32).floor();
        self.size_info.cell_height = ((metrics.line_height + config.font().offset().y as f64) as f32).floor();
    }

    #[inline]
    pub fn resize_channel(&self) -> mpsc::Sender<(u32, u32)> {
        self.tx.clone()
    }

    pub fn window(&mut self) -> &mut Window {
        &mut self.window
    }

    /// Process pending resize events
    pub fn handle_resize(
        &mut self,
        terminal: &mut MutexGuard<Term>,
        config: &Config,
        items: &mut [&mut OnResize]
    ) {
        // Resize events new_size and are handled outside the poll_events
        // iterator. This has the effect of coalescing multiple resize
        // events into one.
        let mut new_size = None;

        // Take most recent resize event, if any
        while let Ok(sz) = self.rx.try_recv() {
            new_size = Some(sz);
        }

        if terminal.font_size_modifier != self.font_size_modifier {
            // Font size modification detected

            self.font_size_modifier = terminal.font_size_modifier;
            self.update_glyph_cache(config, terminal.font_size_modifier);

            if new_size == None {
                // Force a resize to refresh things
                new_size = Some((self.size_info.width as u32,
                                 self.size_info.height as u32));
            }
        }

        // Receive any resize events; only call gl::Viewport on last
        // available
        if let Some((w, h)) = new_size.take() {
            self.size_info.width = w as f32;
            self.size_info.height = h as f32;

            let size = &self.size_info;
            terminal.resize(size);

            for item in items {
                item.on_resize(size)
            }

            self.window.resize(w, h);
            self.renderer.resize(w as i32, h as i32);
        }

    }

    /// Draw the screen
    ///
    /// A reference to Term whose state is being drawn must be provided.
    ///
    /// This call may block if vsync is enabled
    pub fn draw(&mut self, mut terminal: MutexGuard<Term>, config: &Config, selection: Option<&Selection>) {
        // Clear dirty flag
        terminal.dirty = !terminal.visual_bell.completed();

        if let Some(title) = terminal.get_next_title() {
            self.window.set_title(&title);
        }

        if let Some(is_urgent) = terminal.next_is_urgent.take() {
            // We don't need to set the urgent flag if we already have the
            // user's attention.
            if !is_urgent || !self.window.is_focused {
                self.window.set_urgent(is_urgent);
            }
        }

        let size_info = *terminal.size_info();
        let visual_bell_intensity = terminal.visual_bell.intensity();

        let background_color = terminal.background_color();
        let background_color_changed = background_color != self.last_background_color;
        self.last_background_color = background_color;

        {
            let glyph_cache = &mut self.glyph_cache;

            // Draw grid
            {
                let _sampler = self.meter.sampler();

                // Make a copy of size_info since the closure passed here
                // borrows terminal mutably
                //
                // TODO I wonder if the renderable cells iter could avoid the
                // mutable borrow
                self.renderer.with_api(config, &size_info, visual_bell_intensity, |mut api| {
                    // Clear screen to update whole background with new color
                    if background_color_changed {
                        api.clear(background_color);
                    }

                    // Draw the grid
                    api.render_cells(terminal.renderable_cells(config, selection), glyph_cache);
                });
            }

            // Draw render timer
            if self.render_timer {
                let timing = format!("{:.3} usec", self.meter.average());
                let color = Rgb { r: 0xd5, g: 0x4e, b: 0x53 };
                self.renderer.with_api(config, &size_info, visual_bell_intensity, |mut api| {
                    api.render_string(&timing[..], glyph_cache, color);
                });
            }
        }

        // Unlock the terminal mutex; following call to swap_buffers() may block
        drop(terminal);
        self.window
            .swap_buffers()
            .expect("swap buffers");

        // Clear after swap_buffers when terminal mutex isn't held. Mesa for
        // some reason takes a long time to call glClear(). The driver descends
        // into xcb_connect_to_fd() which ends up calling __poll_nocancel()
        // which blocks for a while.
        //
        // By keeping this outside of the critical region, the Mesa bug is
        // worked around to some extent. Since this doesn't actually address the
        // issue of glClear being slow, less time is available for input
        // handling and rendering.
        self.renderer.with_api(config, &size_info, visual_bell_intensity, |api| {
            api.clear(background_color);
        });
    }

    pub fn get_window_id(&self) -> Option<usize> {
        self.window.get_window_id()
    }

    /// Adjust the XIM editor position according to the new location of the cursor
    pub fn update_ime_position(&mut self, terminal: &Term) {
        use index::{Point, Line, Column};
        use term::SizeInfo;
        let Point{line: Line(row), col: Column(col)} = terminal.cursor().point;
        let SizeInfo{cell_width: cw,
                    cell_height: ch,
                    padding_x: px,
                    padding_y: py, ..} = *terminal.size_info();
        let nspot_y = (py + (row + 1) as f32 * ch) as i16;
        let nspot_x = (px + col as f32 * cw) as i16;
        self.window().send_xim_spot(nspot_x, nspot_y);
    }
}
