//! Native window chrome: a terminal-styled tab bar, project sidebar and right-click context menu
//! drawn with Alacritty's own GL renderer (solid rects + cell-aligned text), replacing the
//! previous egui-based chrome.
//!
//! The chrome is laid out on a window-origin cell grid (the same `cell_width`/`cell_height` as the
//! terminal): the tab bar occupies the top row, the sidebar the leftmost columns. Each frame
//! [`Chrome::layout`] produces a list of [`RenderRect`]s (backgrounds, highlights, borders) in
//! absolute window pixels and a list of [`RenderableCell`]s (labels) in window-cell coordinates,
//! plus the hot regions used to hit-test mouse input.

use unicode_width::UnicodeWidthChar;

use alacritty_terminal::index::{Column, Point};
use alacritty_terminal::term::cell::Flags;

use crate::display::color::Rgb;
use crate::display::content::RenderableCell;
use crate::renderer::rects::RenderRect;

use super::TabBarInfo;

/// Scale of the chrome font relative to the terminal font. The chrome is rendered from a
/// dedicated, larger glyph cache so it stays legible regardless of the terminal font size.
pub const CHROME_SCALE: f32 = 1.5;

/// Columns (in chrome cells) reserved for the project sidebar when visible.
const SIDEBAR_COLS: usize = 14;
/// Maximum display width (in chrome cells) of a tab label before it is truncated.
const MAX_TAB_LABEL: usize = 16;
/// Width (in chrome cells) of the right-click context menu.
const MENU_COLS: usize = 10;

// Spacing, expressed as multiples of the chrome cell so it scales with the font.
/// Tab bar height = chrome cell height × this.
const BAR_H_MULT: f32 = 1.85;
/// Sidebar / menu row height = chrome cell height × this.
const ROW_H_MULT: f32 = 1.65;
/// Horizontal inset before text = chrome cell width × this.
const PAD_X_MULT: f32 = 0.8;
/// Gap between adjacent tabs = chrome cell width × this.
const TAB_GAP_MULT: f32 = 1.1;

#[inline]
fn c(r: u8, g: u8, b: u8) -> Rgb {
    Rgb::new(r, g, b)
}

// zinc palette (shadcn-inspired), matching the previous egui theme.
fn bar_bg() -> Rgb {
    c(0x0c, 0x0c, 0x0e)
}
fn menu_bg() -> Rgb {
    c(0x09, 0x09, 0x0b)
}
fn border() -> Rgb {
    c(0x27, 0x27, 0x2a)
}
/// Selected item background.
fn accent() -> Rgb {
    c(0x3f, 0x3f, 0x46)
}
/// Hovered item background.
fn hover_bg() -> Rgb {
    c(0x27, 0x27, 0x2a)
}
/// Primary foreground.
fn fg() -> Rgb {
    c(0xfa, 0xfa, 0xfa)
}
/// Muted foreground (headers, idle affordances).
fn dim() -> Rgb {
    c(0xa1, 0xa1, 0xaa)
}

/// A clickable region of the chrome, in window pixels.
#[derive(Clone, Copy)]
struct PixelRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl PixelRect {
    fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// An actionable target in the chrome, produced by hit-testing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    SelectTab(u64),
    CloseTab(u64),
    CreateTab,
    SelectProject(usize),
    CloseProject(usize),
    CreateProject,
    /// Resume the active project's Claude session at this index into `TabBarInfo::project_sessions`.
    OpenClaudeSession(usize),
    Copy,
    Paste,
}

/// Draw lists produced for a single frame of chrome.
#[derive(Default)]
pub struct ChromeDraw {
    /// Backgrounds, highlights and borders, in absolute window pixels.
    pub rects: Vec<RenderRect>,
    /// Label glyphs, positioned on the window-origin chrome cell grid.
    pub cells: Vec<RenderableCell>,
    /// Pixel height reserved at the top for the tab bar (0 when hidden).
    pub bar_height: f32,
    /// Pixel width reserved at the left for the sidebar (0 when hidden).
    pub sidebar_width: f32,
}

/// Native chrome state: visibility, the open context menu, the last mouse position and the hot
/// regions from the most recent layout.
pub struct Chrome {
    pub sidebar_visible: bool,
    /// Window-pixel position the right-click context menu was opened at, if any.
    pub context_menu: Option<(f32, f32)>,
    /// Last observed mouse position in window pixels.
    mouse: (f32, f32),
    /// Hot regions from the most recent [`Self::layout`], topmost last.
    hits: Vec<(PixelRect, Hit)>,
    /// Currently hovered hot region, used to paint a hover highlight.
    hover: Option<Hit>,
    /// Sidebar width (px) from the most recent layout, for region tests.
    sidebar_w: f32,
    /// Tab bar height (px) from the most recent layout, for region tests.
    bar_h: f32,
}

impl Chrome {
    pub fn new() -> Self {
        Self {
            sidebar_visible: true,
            context_menu: None,
            mouse: (0., 0.),
            hits: Vec::new(),
            hover: None,
            sidebar_w: 0.,
            bar_h: 0.,
        }
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }

    /// The last mouse position recorded via [`Self::set_mouse`], in window pixels.
    pub fn last_mouse(&self) -> (f32, f32) {
        self.mouse
    }

    /// Record the latest mouse position and recompute the hovered region. Returns whether the
    /// hovered region changed (i.e. a redraw is needed to update the highlight).
    pub fn set_mouse(&mut self, x: f32, y: f32) -> bool {
        self.mouse = (x, y);
        let hover = self.hit(x, y);
        let changed = hover != self.hover;
        self.hover = hover;
        changed
    }

    /// Whether `(x, y)` (window pixels) lies over any chrome surface (sidebar or tab bar).
    pub fn in_region(&self, x: f32, y: f32) -> bool {
        (self.sidebar_w > 0. && x < self.sidebar_w) || (self.bar_h > 0. && y < self.bar_h)
    }

    /// Hit-test the last recorded mouse position.
    pub fn hit_mouse(&self) -> Option<Hit> {
        self.hit(self.mouse.0, self.mouse.1)
    }

    fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        self.hits.iter().rev().find(|(r, _)| r.contains(x, y)).map(|(_, h)| *h)
    }

    /// The tab bar is always shown for a non-empty project, so a single tab still exposes the
    /// active tab and the new-tab affordance.
    fn show_tab_bar(tab_count: usize) -> bool {
        tab_count >= 1
    }

    /// Build the chrome draw lists and refresh the hot regions. `cw`/`ch` are the chrome cell
    /// dimensions (pixels); `win_w`/`win_h` the window size (pixels). Text and rects are emitted in
    /// absolute window pixels (cells carry pixel positions, rendered with a 1×1 cell projection).
    pub fn layout(
        &mut self,
        info: &TabBarInfo,
        cw: f32,
        ch: f32,
        win_w: f32,
        win_h: f32,
    ) -> ChromeDraw {
        let mut draw = ChromeDraw::default();
        self.hits.clear();

        let pad_x = cw * PAD_X_MULT;
        let row_h = ch * ROW_H_MULT;

        let sidebar_w = if self.sidebar_visible { SIDEBAR_COLS as f32 * cw } else { 0. };
        let show_tabs = Self::show_tab_bar(info.titles.len());
        let bar_h = if show_tabs { ch * BAR_H_MULT } else { 0. };

        self.sidebar_w = sidebar_w;
        self.bar_h = bar_h;
        draw.sidebar_width = sidebar_w;
        draw.bar_height = bar_h;

        if self.sidebar_visible {
            self.layout_sidebar(info, &mut draw, cw, ch, win_h, sidebar_w, pad_x, row_h);
        }
        if show_tabs {
            self.layout_tab_bar(info, &mut draw, cw, ch, win_w, sidebar_w, pad_x, bar_h);
        }
        if self.context_menu.is_some() {
            self.layout_context_menu(&mut draw, cw, ch, win_w, win_h, pad_x, row_h);
        }

        draw
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_sidebar(
        &mut self,
        info: &TabBarInfo,
        draw: &mut ChromeDraw,
        cw: f32,
        ch: f32,
        win_h: f32,
        sidebar_w: f32,
        pad_x: f32,
        row_h: f32,
    ) {
        // Background strip and right border.
        draw.rects.push(rect(0., 0., sidebar_w, win_h, bar_bg()));
        draw.rects.push(rect(sidebar_w - 1., 0., 1., win_h, border()));

        // Header.
        push_text_px(&mut draw.cells, pad_x, baseline(0., row_h, ch), "项目", dim(), cw);
        draw.rects.push(rect(0., row_h, sidebar_w, 1., border()));

        // A delete affordance is shown per project once there is more than one (the window always
        // keeps at least one project). It reserves room on the right so labels don't run under it.
        let deletable = info.project_names.len() >= 2;
        let label_budget = if deletable { 3. } else { 2. };
        let max_label = ((sidebar_w - label_budget * pad_x - if deletable { cw } else { 0. }) / cw)
            .floor()
            .max(1.) as usize;

        // Project list, one row each below the header.
        let mut y = row_h;
        for (i, name) in info.project_names.iter().enumerate() {
            let label = if name.is_empty() { "~" } else { name.as_str() };
            let label = truncate(label, max_label);
            let region = PixelRect { x: 0., y, w: sidebar_w, h: row_h };

            // The row is "hovered" when the mouse is over its name or its delete button.
            let row_hovered =
                matches!(self.hover, Some(Hit::SelectProject(h) | Hit::CloseProject(h)) if h == i);

            let selected = i == info.active_project;
            if selected {
                draw.rects.push(rect(0., y, sidebar_w, row_h, accent()));
            } else if row_hovered {
                draw.rects.push(rect(0., y, sidebar_w, row_h, hover_bg()));
            }
            push_text_px(&mut draw.cells, pad_x, baseline(y, row_h, ch), &label, fg(), cw);
            self.hits.push((region, Hit::SelectProject(i)));

            // Delete button: only painted while the row is hovered, but its hit region is always
            // registered (you can only click it while hovering). Pushed after the row so it wins
            // hit-testing on the right edge.
            if deletable {
                let close_x = sidebar_w - cw - pad_x * 0.5;
                let close_region =
                    PixelRect { x: close_x - pad_x * 0.5, y, w: cw + pad_x, h: row_h };
                if row_hovered {
                    if self.hover == Some(Hit::CloseProject(i)) {
                        draw.rects.push(rect(close_region.x, y, close_region.w, row_h, accent()));
                    }
                    push_text_px(&mut draw.cells, close_x, baseline(y, row_h, ch), "×", dim(), cw);
                }
                self.hits.push((close_region, Hit::CloseProject(i)));
            }
            y += row_h;

            // Indented Claude session sub-rows under the active project only.
            if selected {
                let sub_h = row_h * 0.82;
                let sub_pad = pad_x + cw;
                let max_sub = ((sidebar_w - sub_pad - pad_x) / cw).floor().max(1.) as usize;
                for (j, session) in info.project_sessions.iter().enumerate() {
                    let label = truncate(&session.label, max_sub);
                    let region = PixelRect { x: 0., y, w: sidebar_w, h: sub_h };
                    if self.hover == Some(Hit::OpenClaudeSession(j)) {
                        draw.rects.push(rect(0., y, sidebar_w, sub_h, hover_bg()));
                    }
                    push_text_px(&mut draw.cells, sub_pad, baseline(y, sub_h, ch), &label, dim(), cw);
                    self.hits.push((region, Hit::OpenClaudeSession(j)));
                    y += sub_h;
                }
            }
        }

        // New-project affordance, separated from the list above.
        draw.rects.push(rect(0., y, sidebar_w, 1., border()));
        let region = PixelRect { x: 0., y, w: sidebar_w, h: row_h };
        if self.hover == Some(Hit::CreateProject) {
            draw.rects.push(rect(0., y, sidebar_w, row_h, hover_bg()));
        }
        push_text_px(&mut draw.cells, pad_x, baseline(y, row_h, ch), "+ 新建项目", dim(), cw);
        self.hits.push((region, Hit::CreateProject));
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_tab_bar(
        &mut self,
        info: &TabBarInfo,
        draw: &mut ChromeDraw,
        cw: f32,
        ch: f32,
        win_w: f32,
        sidebar_w: f32,
        pad_x: f32,
        bar_h: f32,
    ) {
        let bar_x = sidebar_w;

        // Background strip (to the right of the sidebar) and bottom border.
        draw.rects.push(rect(bar_x, 0., win_w - bar_x, bar_h, bar_bg()));
        draw.rects.push(rect(bar_x, bar_h - 1., win_w - bar_x, 1., border()));

        let base = baseline(0., bar_h, ch);
        let tab_gap = cw * TAB_GAP_MULT;
        // Highlight blocks sit inside the bar with a vertical margin so they read as pills.
        let hl_margin = ((bar_h - ch) * 0.3).max(2.);
        let hl_h = bar_h - 2. * hl_margin;

        let mut x = bar_x + pad_x;
        for (i, title) in info.titles.iter().enumerate() {
            let id = info.ids[i];
            let name = if title.is_empty() { "shell" } else { title.as_str() };
            let label = truncate(&format!("{}:{}", i + 1, name), MAX_TAB_LABEL);
            let label_w = str_width(&label) as f32 * cw;
            let seg_w = label_w + pad_x + cw; // label + gap + close glyph

            // Stop drawing once segments overflow the window (no horizontal scroll).
            if x + seg_w + pad_x > win_w {
                break;
            }

            let selected = i == info.active;
            if selected {
                draw.rects.push(rect(x - pad_x * 0.5, hl_margin, seg_w + pad_x, hl_h, accent()));
            } else if self.hover == Some(Hit::SelectTab(id)) {
                draw.rects.push(rect(x - pad_x * 0.5, hl_margin, seg_w + pad_x, hl_h, hover_bg()));
            }

            // Label.
            let label_region = PixelRect { x, y: 0., w: label_w + pad_x * 0.5, h: bar_h };
            push_text_px(&mut draw.cells, x, base, &label, if selected { fg() } else { dim() }, cw);
            self.hits.push((label_region, Hit::SelectTab(id)));

            // Close button.
            let close_x = x + label_w + pad_x;
            let close_region =
                PixelRect { x: close_x - pad_x * 0.5, y: 0., w: cw + pad_x * 0.5, h: bar_h };
            if self.hover == Some(Hit::CloseTab(id)) {
                draw.rects.push(rect(close_x - 3., hl_margin, cw + 6., hl_h, hover_bg()));
            }
            push_text_px(&mut draw.cells, close_x, base, "×", dim(), cw);
            self.hits.push((close_region, Hit::CloseTab(id)));

            x += seg_w + tab_gap;
        }

        // New-tab affordance.
        if x + cw + pad_x <= win_w {
            let region = PixelRect { x: x - pad_x * 0.5, y: 0., w: cw + pad_x, h: bar_h };
            if self.hover == Some(Hit::CreateTab) {
                draw.rects.push(rect(x - pad_x * 0.5, hl_margin, cw + pad_x, hl_h, hover_bg()));
            }
            push_text_px(&mut draw.cells, x, base, "+", fg(), cw);
            self.hits.push((region, Hit::CreateTab));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_context_menu(
        &mut self,
        draw: &mut ChromeDraw,
        cw: f32,
        ch: f32,
        win_w: f32,
        win_h: f32,
        pad_x: f32,
        row_h: f32,
    ) {
        let Some((mx, my)) = self.context_menu else { return };

        const ITEMS: [(&str, Hit); 2] = [("Copy", Hit::Copy), ("Paste", Hit::Paste)];

        let w = MENU_COLS as f32 * cw;
        let h = ITEMS.len() as f32 * row_h;
        let x = mx.min((win_w - w).max(0.));
        let y = my.min((win_h - h).max(0.));

        // Popover surface with a hairline border.
        draw.rects.push(rect(x, y, w, h, menu_bg()));
        push_border(&mut draw.rects, x, y, w, h, border());

        for (k, (label, hit)) in ITEMS.iter().enumerate() {
            let item_y = y + k as f32 * row_h;
            let region = PixelRect { x, y: item_y, w, h: row_h };
            if self.hover == Some(*hit) {
                draw.rects.push(rect(x, item_y, w, row_h, hover_bg()));
            }
            push_text_px(&mut draw.cells, x + pad_x, baseline(item_y, row_h, ch), label, fg(), cw);
            self.hits.push((region, *hit));
        }
    }
}

/// Solid window-pixel rectangle.
fn rect(x: f32, y: f32, w: f32, h: f32, color: Rgb) -> RenderRect {
    RenderRect::new(x, y, w, h, color, 1.)
}

/// Hairline border around `(x, y, w, h)` as four 1px rects.
fn push_border(rects: &mut Vec<RenderRect>, x: f32, y: f32, w: f32, h: f32, color: Rgb) {
    rects.push(rect(x, y, w, 1., color));
    rects.push(rect(x, y + h - 1., w, 1., color));
    rects.push(rect(x, y, 1., h, color));
    rects.push(rect(x + w - 1., y, 1., h, color));
}

/// Text baseline (window pixels) that vertically centres a `ch`-tall line within a row of height
/// `row_h` starting at `row_top`. The renderer places a glyph's baseline at the bottom of its cell,
/// so this is the bottom of the centred line box.
fn baseline(row_top: f32, row_h: f32, ch: f32) -> f32 {
    row_top + (row_h + ch) * 0.5
}

/// Emit `text` as glyphs at absolute window pixels, starting at `(x, baseline)` and advancing by
/// `advance` pixels per cell (2× for wide glyphs). Cells carry pixel positions, so the chrome pass
/// must render them with a 1×1-pixel cell projection. Returns the x pixel after the last glyph.
fn push_text_px(
    cells: &mut Vec<RenderableCell>,
    x: f32,
    baseline: f32,
    text: &str,
    fg: Rgb,
    advance: f32,
) -> f32 {
    // The shader puts the baseline one cell below the cell origin (cell height is 1px here).
    let row = (baseline.round() as usize).saturating_sub(1);
    let mut x = x;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if width == 0 {
            continue;
        }
        let flags = if width == 2 { Flags::WIDE_CHAR } else { Flags::empty() };
        cells.push(RenderableCell {
            point: Point::new(row, Column(x.round().max(0.) as usize)),
            character: ch,
            fg,
            bg: Rgb::new(0, 0, 0),
            bg_alpha: 0.,
            underline: fg,
            flags,
            extra: None,
        });
        x += advance * width as f32;
    }
    x
}

/// Display width of `text` in cells.
fn str_width(text: &str) -> usize {
    text.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// Truncate `text` to at most `max` display cells, appending an ellipsis when shortened.
fn truncate(text: &str, max: usize) -> String {
    if str_width(text) <= max {
        return text.to_owned();
    }
    if max == 0 {
        return String::new();
    }
    let mut width = 0;
    let mut out = String::new();
    for ch in text.chars() {
        let cw = ch.width().unwrap_or(0);
        if width + cw > max.saturating_sub(1) {
            break;
        }
        width += cw;
        out.push(ch);
    }
    out.push('…');
    out
}
