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
use std::f64;

use parking_lot::{MutexGuard};

use {PhysicalSize, Rgb};
use cli;
use config::Config;
use font::{self, Rasterize};
use meter::Meter;
use renderer::{self, GlyphCache, QuadRenderer};
use term::{Term, SizeInfo};
use sync::FairMutex;

#[derive(Debug)]
pub enum Error {
    /// Error dealing with fonts
    Font(font::Error),

    /// Error in renderer
    Render(renderer::Error),
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::Font(ref err) => Some(err),
            Error::Render(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Font(ref err) => err.description(),
            Error::Render(ref err) => err.description(),
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::Font(ref err) => err.fmt(f),
            Error::Render(ref err) => err.fmt(f),
        }
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
    renderer: QuadRenderer,
    glyph_cache: GlyphCache,
    render_timer: bool,
    rx: mpsc::Receiver<PhysicalSize>,
    tx: mpsc::Sender<PhysicalSize>,
    meter: Meter,
    font_size: font::Size,
    size_info: SizeInfo,
}

/// Types that are interested in when the display is resized
pub trait OnResize {
    fn on_resize(&mut self, size: &SizeInfo);
}

impl Display {
    pub fn update_config(&mut self, config: &Config) {
        self.render_timer = config.render_timer();
    }

    /// Get size info about the display
    pub fn size(&self) -> &SizeInfo {
        &self.size_info
    }

    pub fn new(config: &Config, options: &cli::Options, dpr: f64) -> Result<Display, Error> {
        // Extract some properties from config
        let render_timer = config.render_timer();

        // Create renderer
        let mut renderer = QuadRenderer::new(config, PhysicalSize::new(0.0, 0.0), dpr)?;

        let (glyph_cache, cell_width, cell_height) =
            Self::new_glyph_cache(dpr, &mut renderer, config)?;


        let dimensions = options.dimensions()
            .unwrap_or_else(|| config.dimensions());

        let width = cell_width as u32 * dimensions.columns_u32();
        let height = cell_height as u32 * dimensions.lines_u32();

        // Resize window to specified dimensions unless one or both dimensions are 0
        let size = PhysicalSize::new(
            f64::from(width + 2 * (f64::from(config.padding().x) * dpr) as u32),
            f64::from(height + 2 * (f64::from(config.padding().y) * dpr) as u32) as f64,
        );

        renderer.resize(size, dpr);

        info!("Cell Size: ({} x {})", cell_width, cell_height);

        let size_info = SizeInfo {
            dpr,
            width: size.width as f32,
            height: size.height as f32,
            cell_width: cell_width as f32,
            cell_height: cell_height as f32,
            padding_x: (f64::from(config.padding().x) * dpr).floor() as f32,
            padding_y: (f64::from(config.padding().y) * dpr).floor() as f32,
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
            renderer,
            glyph_cache,
            render_timer,
            tx,
            rx,
            meter: Meter::new(),
            font_size: font::Size::new(0.),
            size_info,
        })
    }

    fn new_glyph_cache(dpr: f64, renderer: &mut QuadRenderer, config: &Config)
        -> Result<(GlyphCache, f32, f32), Error>
    {
        let font = config.font().clone();
        let rasterizer = font::Rasterizer::new(dpr as f32, config.use_thin_strokes())?;

        // Initialize glyph cache
        let glyph_cache = {
            info!("Initializing glyph cache");
            let init_start = ::std::time::Instant::now();

            let cache = renderer.with_loader(|mut api| {
                GlyphCache::new(rasterizer, &font, &mut api)
            })?;

            let stop = init_start.elapsed();
            let stop_f = stop.as_secs() as f64 + f64::from(stop.subsec_nanos()) / 1_000_000_000f64;
            info!("Finished initializing glyph cache in {}", stop_f);

            cache
        };

        // Need font metrics to resize the window properly. This suggests to me the
        // font metrics should be computed before creating the window in the first
        // place so that a resize is not needed.
        let metrics = glyph_cache.font_metrics();
        let cell_width = metrics.average_advance as f32 + f32::from(font.offset().x);
        let cell_height = metrics.line_height as f32 + f32::from(font.offset().y);

        // Prevent invalid cell sizes
        if cell_width < 1. || cell_height < 1. {
            panic!("font offset is too small");
        }

        Ok((glyph_cache, cell_width.floor(), cell_height.floor()))
    }

    pub fn update_glyph_cache(&mut self, config: &Config) {
        let dpr = self.size_info.dpr;
        let cache = &mut self.glyph_cache;
        let size = self.font_size;
        self.renderer.with_loader(|mut api| {
            let _ = cache.update_font_size(config.font(), size, dpr, &mut api);
        });

        let metrics = cache.font_metrics();
        self.size_info.cell_width = ((metrics.average_advance + f64::from(config.font().offset().x)) as f32).floor();
        self.size_info.cell_height = ((metrics.line_height + f64::from(config.font().offset().y)) as f32).floor();
    }

    #[inline]
    pub fn resize_channel(&self) -> mpsc::Sender<PhysicalSize> {
        self.tx.clone()
    }

    /// Process pending resize events
    pub fn handle_resize(
        &mut self,
        terminal: &mut MutexGuard<Term>,
        config: &Config,
        items: &mut [&mut OnResize],
        dpr: f64,
    ) {
        // Resize events new_size and are handled outside the poll_events
        // iterator. This has the effect of coalescing multiple resize
        // events into one.
        let mut new_size = None;

        // Take most recent resize event, if any
        while let Ok(size) = self.rx.try_recv() {
            new_size = Some(size);
        }

        // Font size/DPI factor modification detected
        if terminal.font_size != self.font_size || (dpr - self.size_info.dpr).abs() > f64::EPSILON {
            if new_size == None {
                // Force a resize to refresh things
                new_size = Some(PhysicalSize::new(
                    f64::from(self.size_info.width) / self.size_info.dpr * dpr,
                    f64::from(self.size_info.height) / self.size_info.dpr * dpr,
                ));
            }

            self.font_size = terminal.font_size;
            self.size_info.dpr = dpr;
            self.size_info.padding_x = (f64::from(config.padding().x) * dpr).floor() as f32;
            self.size_info.padding_y = (f64::from(config.padding().y) * dpr).floor() as f32;
            self.update_glyph_cache(config);
        }

        // Receive any resize events; only call gl::Viewport on last
        // available
        if let Some(psize) = new_size.take() {
            self.size_info.width = psize.width as f32;
            self.size_info.height = psize.height as f32;
            self.size_info.dpr = dpr;

            let size = &self.size_info;
            terminal.resize(size);

            for item in items {
                item.on_resize(size)
            }

            self.renderer.resize(psize, dpr);
        }
    }

    /// Draw the screen
    ///
    /// A reference to Term whose state is being drawn must be provided.
    ///
    /// This call may block if vsync is enabled
    pub fn draw(&mut self, terminal: &FairMutex<Term>, config: &Config, window_focused: bool) {
        let terminal_locked = terminal.lock();
        let size_info = *terminal_locked.size_info();
        let visual_bell_intensity = terminal_locked.visual_bell.intensity();
        let background_color = terminal_locked.background_color();

        // Clear when terminal mutex isn't held. Mesa for
        // some reason takes a long time to call glClear(). The driver descends
        // into xcb_connect_to_fd() which ends up calling __poll_nocancel()
        // which blocks for a while.
        //
        // By keeping this outside of the critical region, the Mesa bug is
        // worked around to some extent. Since this doesn't actually address the
        // issue of glClear being slow, less time is available for input
        // handling and rendering.
        drop(terminal_locked);

        self.renderer.with_api(config, &size_info, visual_bell_intensity, |api| {
            api.clear(background_color);
        });

        let mut terminal = terminal.lock();

        // Clear dirty flag
        terminal.dirty = !terminal.visual_bell.completed();

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
                    // Draw the grid
                    api.render_cells(
                        terminal.renderable_cells(config, window_focused),
                        glyph_cache,
                    );
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
    }

    /// Adjust the IME editor position according to the new location of the cursor
    pub fn current_ime_position(&mut self, terminal: &Term) -> (i32, i32) {
        use index::{Column, Line, Point};
        use term::SizeInfo;
        let Point{line: Line(row), col: Column(col)} = terminal.cursor().point;
        let SizeInfo{cell_width: cw,
                    cell_height: ch,
                    padding_x: px,
                    padding_y: py, ..} = *terminal.size_info();
        let nspot_y = (py + (row + 1) as f32 * ch) as i32;
        let nspot_x = (px + col as f32 * cw) as i32;
        (nspot_x, nspot_y)
    }
}
