use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::Color as AnsiColor;
use egui::{
    Color32, FontFamily, FontId, PointerButton, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2,
};

use crate::clipboard::{self, Target};
use crate::colors::{background, foreground, resolve, rgb_to_color32};
use crate::config::Config;
use crate::fonts::{BOLD_FAMILY, BOLD_ITALIC_FAMILY, ITALIC_FAMILY};
use crate::input::event_to_bytes;
use crate::session::{EventProxy, Session, TermSize};

pub fn show(ui: &mut Ui, session: &mut Session, config: &Config, allow_focus: bool) -> Response {
    let font_id = FontId::monospace(config.font.egui_size());
    let (cell_w_pt, cell_h_pt) = ui.ctx().fonts(|f| {
        let w = f.glyph_width(&font_id, 'M');
        let h = f.row_height(&font_id);
        (w, h)
    });
    // Floor cell size to whole device pixels — matches alacritty's
    // `compute_cell_size`.  Without this, fractional cell widths combined
    // with egui's AA fringe leave visible seams between adjacent cells.
    let ppp = ui.ctx().pixels_per_point();
    let cell_w = (cell_w_pt * ppp).floor().max(1.0) / ppp;
    let cell_h = (cell_h_pt * ppp).floor().max(1.0) / ppp;

    let pad_x = config.window.padding_x;
    let pad_y = config.window.padding_y;
    let avail = ui.available_size();
    let inner_w = (avail.x - 2.0 * pad_x).max(cell_w);
    let inner_h = (avail.y - 2.0 * pad_y).max(cell_h);
    let cols = (inner_w / cell_w).floor().max(1.0) as usize;
    let rows = (inner_h / cell_h).floor().max(1.0) as usize;
    session.resize(TermSize::new(cols, rows), (cell_w, cell_h));

    if pad_x > 0.0 || pad_y > 0.0 {
        ui.add_space(pad_y);
    }
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(
            cols as f32 * cell_w + 2.0 * pad_x,
            rows as f32 * cell_h + (if pad_y > 0.0 { 0.0 } else { 0.0 }),
        ),
        Sense::click_and_drag(),
    );
    // Snap the grid origin so column/row boundaries stay on integer pixels.
    let snap = |v: f32| (v * ppp).round() / ppp;
    let rect = Rect::from_min_size(
        Pos2::new(snap(rect.min.x + pad_x), snap(rect.min.y)),
        Vec2::new(cols as f32 * cell_w, rows as f32 * cell_h),
    );

    if allow_focus && !response.has_focus() {
        response.request_focus();
    }

    let painter = ui.painter_at(rect);

    handle_selection(ui, &response, session, config, rect, cell_w, cell_h, cols, rows);
    paint_grid(&painter, rect, session, config, &font_id, cell_w, cell_h);

    if allow_focus && response.has_focus() {
        let consumed: Vec<Vec<u8>> =
            ui.input(|i| i.events.iter().filter_map(event_to_bytes).collect());
        if !consumed.is_empty() {
            // Typing should drop any active selection — matches alacritty's UX.
            session.term.lock().selection = None;
        }
        for bytes in consumed {
            session.write(bytes);
        }
    }

    response
}

fn handle_selection(
    ui: &Ui,
    response: &Response,
    session: &Session,
    config: &Config,
    rect: Rect,
    cell_w: f32,
    cell_h: f32,
    cols: usize,
    rows: usize,
) {
    let primary = PointerButton::Primary;
    let secondary = PointerButton::Secondary;
    let middle = PointerButton::Middle;
    let modifiers = ui.input(|i| i.modifiers);

    // Middle-click pastes the PRIMARY (selection) buffer — alacritty's default.
    if response.clicked_by(middle) {
        if let Some(text) = clipboard::read(Target::Primary) {
            session.write(text.into_bytes());
        }
        return;
    }

    // Right-click extends the active selection's far edge to the click point,
    // matching alacritty's `ExpandSelection` mouse action.
    if response.clicked_by(secondary) {
        if let Some(pos) = click_position(ui, response) {
            let mut term = session.term.lock();
            if term.selection.is_some() {
                let display_offset = term.grid().display_offset() as i32;
                let (point, side) =
                    cell_at_pos(pos, rect, cell_w, cell_h, cols, rows, display_offset);
                if let Some(sel) = term.selection.as_mut() {
                    sel.update(point, side);
                }
                copy_active_selection(&term, config);
            }
        }
        return;
    }

    // Triple/double clicks set Lines/Semantic immediately and copy on the same release.
    if response.triple_clicked_by(primary) {
        if let Some(pos) = click_position(ui, response) {
            start_selection_at(
                session,
                config,
                rect,
                cell_w,
                cell_h,
                cols,
                rows,
                pos,
                SelectionType::Lines,
            );
        }
        return;
    }
    if response.double_clicked_by(primary) {
        if let Some(pos) = click_position(ui, response) {
            start_selection_at(
                session,
                config,
                rect,
                cell_w,
                cell_h,
                cols,
                rows,
                pos,
                SelectionType::Semantic,
            );
        }
        return;
    }

    if response.drag_started_by(primary) {
        if let Some(pos) = response.interact_pointer_pos() {
            let ty = if modifiers.ctrl { SelectionType::Block } else { SelectionType::Simple };
            let mut term = session.term.lock();
            let display_offset = term.grid().display_offset() as i32;
            let (point, side) = cell_at_pos(pos, rect, cell_w, cell_h, cols, rows, display_offset);
            term.selection = Some(Selection::new(ty, point, side));
        }
    } else if response.dragged_by(primary) {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut term = session.term.lock();
            let display_offset = term.grid().display_offset() as i32;
            let (point, side) = cell_at_pos(pos, rect, cell_w, cell_h, cols, rows, display_offset);
            if let Some(sel) = term.selection.as_mut() {
                sel.update(point, side);
            }
        }
    } else if response.drag_stopped_by(primary) {
        copy_active_selection(&session.term.lock(), config);
    } else if response.clicked_by(primary) {
        // Bare click outside an existing drag clears the selection, matching alacritty.
        session.term.lock().selection = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn start_selection_at(
    session: &Session,
    config: &Config,
    rect: Rect,
    cell_w: f32,
    cell_h: f32,
    cols: usize,
    rows: usize,
    pos: Pos2,
    ty: SelectionType,
) {
    let mut term = session.term.lock();
    let display_offset = term.grid().display_offset() as i32;
    let (point, side) = cell_at_pos(pos, rect, cell_w, cell_h, cols, rows, display_offset);
    term.selection = Some(Selection::new(ty, point, side));
    copy_active_selection(&term, config);
}

/// Pointer position to use for click handlers.  Triple/double click are
/// reported only on release, by which point `interact_pointer_pos` has already
/// dropped the press location, so fall back to the last hover position.
fn click_position(ui: &Ui, response: &Response) -> Option<Pos2> {
    response.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.hover_pos()))
}

fn cell_at_pos(
    pos: Pos2,
    rect: Rect,
    cell_w: f32,
    cell_h: f32,
    cols: usize,
    rows: usize,
    display_offset: i32,
) -> (Point, Side) {
    let local_x = pos.x - rect.min.x;
    let local_y = pos.y - rect.min.y;
    let col_f = local_x / cell_w;
    let row_f = local_y / cell_h;
    let col = (col_f.floor() as i32).clamp(0, cols as i32 - 1) as usize;
    let row = (row_f.floor() as i32).clamp(0, rows as i32 - 1) as usize;
    let frac = col_f - col_f.floor();
    let side = if frac < 0.5 { Side::Left } else { Side::Right };
    (Point::new(Line(row as i32 - display_offset), Column(col)), side)
}

fn copy_active_selection(term: &Term<EventProxy>, config: &Config) {
    let Some(text) = term.selection_to_string() else {
        return;
    };
    if text.is_empty() {
        return;
    }
    // alacritty default: drag-select feeds the PRIMARY (middle-click) buffer.
    // The regular clipboard is mirrored only when `selection.save_to_clipboard`
    // is on.
    clipboard::write(Target::Primary, &text);
    if config.selection.save_to_clipboard {
        clipboard::write(Target::Clipboard, &text);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Style {
    fg: AnsiColor,
    bg: AnsiColor,
    flags: Flags,
}

impl Style {
    fn from_cell(cell: &Cell) -> Self {
        Self { fg: cell.fg, bg: cell.bg, flags: cell.flags }
    }
}

fn paint_grid(
    painter: &egui::Painter,
    rect: Rect,
    session: &Session,
    config: &Config,
    font_id: &FontId,
    cell_w: f32,
    cell_h: f32,
) {
    let term = session.term.lock();
    let runtime_palette = term.colors();
    let grid = term.grid();
    let display_offset = grid.display_offset() as i32;
    let screen_lines = grid.screen_lines() as i32;
    let cols = grid.columns();

    let cursor_point: Point = grid.cursor.point;
    let cursor_visible_line = cursor_point.line.0 + display_offset;
    let bg_color = background(&config.palette);

    // Resolve the active selection once per frame; the per-cell range checks
    // are cheap and avoid relocking inside paint_run.
    let selection_range = term.selection.as_ref().and_then(|s| s.to_range(&term));

    for row_idx in 0..screen_lines {
        let line = Line(row_idx - display_offset);
        let row = &grid[line];
        let y = rect.min.y + row_idx as f32 * cell_h;

        let mut col = 0;
        while col < cols {
            let start = col;
            let style = Style::from_cell(&row[Column(col)]);
            let selected = is_selected(selection_range.as_ref(), line, Column(col));
            let mut run = String::new();
            while col < cols {
                let cell = &row[Column(col)];
                if Style::from_cell(cell) != style
                    || is_selected(selection_range.as_ref(), line, Column(col)) != selected
                {
                    break;
                }
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    col += 1;
                    continue;
                }
                let ch =
                    if cell.c == '\0' || cell.flags.contains(Flags::HIDDEN) { ' ' } else { cell.c };
                run.push(ch);
                col += 1;
            }
            paint_run(
                painter,
                rect,
                &run,
                start,
                y,
                cell_w,
                cell_h,
                style,
                runtime_palette,
                config,
                font_id,
                bg_color,
                selected,
            );
        }
    }

    if cursor_visible_line >= 0 && cursor_visible_line < screen_lines {
        let cursor_shape = term.cursor_style().shape;
        paint_cursor(
            painter,
            rect,
            &term,
            config,
            cursor_point,
            cursor_visible_line,
            cell_w,
            cell_h,
            font_id,
            cursor_shape,
        );
    }
}

fn is_selected(range: Option<&SelectionRange>, line: Line, column: Column) -> bool {
    range.is_some_and(|r| r.contains(Point::new(line, column)))
}

fn font_for_flags(flags: Flags, normal: &FontId) -> FontId {
    let bold = flags.contains(Flags::BOLD);
    let italic = flags.contains(Flags::ITALIC);
    let family = match (bold, italic) {
        (true, true) => FontFamily::Name(BOLD_ITALIC_FAMILY.into()),
        (true, false) => FontFamily::Name(BOLD_FAMILY.into()),
        (false, true) => FontFamily::Name(ITALIC_FAMILY.into()),
        (false, false) => return normal.clone(),
    };
    FontId::new(normal.size, family)
}

#[allow(clippy::too_many_arguments)]
fn paint_run(
    painter: &egui::Painter,
    rect: Rect,
    run: &str,
    start_col: usize,
    y: f32,
    cell_w: f32,
    cell_h: f32,
    style: Style,
    runtime: &alacritty_terminal::term::color::Colors,
    config: &Config,
    font_id: &FontId,
    default_bg: Color32,
    selected: bool,
) {
    if run.is_empty() {
        return;
    }
    let inverse = style.flags.contains(Flags::INVERSE);
    let cell_fg = resolve(
        if inverse { style.bg } else { style.fg },
        style.flags,
        runtime,
        &config.palette,
        true,
    );
    let cell_bg = resolve(
        if inverse { style.fg } else { style.bg },
        style.flags,
        runtime,
        &config.palette,
        false,
    );
    // When `colors.selection.background` is set we honor it; otherwise we swap
    // fg/bg of the underlying cell so the highlight is always visible without
    // requiring a config entry.
    let (fg, bg) = if selected {
        let sel_bg = config
            .palette
            .selection_bg
            .map(rgb_to_color32)
            .unwrap_or_else(|| rgb_to_color32(cell_fg));
        let sel_fg = config.palette.selection_fg.map(rgb_to_color32).unwrap_or_else(|| {
            if config.palette.selection_bg.is_some() {
                rgb_to_color32(cell_fg)
            } else {
                rgb_to_color32(cell_bg)
            }
        });
        (sel_fg, sel_bg)
    } else {
        (rgb_to_color32(cell_fg), rgb_to_color32(cell_bg))
    };

    let width = run.chars().count() as f32 * cell_w;
    let x = rect.min.x + start_col as f32 * cell_w;
    let bg_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, cell_h));

    if bg != default_bg || selected {
        painter.rect_filled(bg_rect, 0.0, bg);
    }

    if !style.flags.contains(Flags::HIDDEN) {
        // Per-glyph paint: egui's run layout drifts off the cursor's `col * cell_w` grid (worse with zoom).
        let glyph_font = font_for_flags(style.flags, font_id);
        let mut buf = [0u8; 4];
        for (i, ch) in run.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            painter.text(
                Pos2::new(x + i as f32 * cell_w, y),
                egui::Align2::LEFT_TOP,
                ch.encode_utf8(&mut buf).to_string(),
                glyph_font.clone(),
                fg,
            );
        }
    }

    if style.flags.intersects(Flags::ALL_UNDERLINES) {
        let uy = y + cell_h - 1.5;
        painter.line_segment([Pos2::new(x, uy), Pos2::new(x + width, uy)], Stroke::new(1.0, fg));
    }
    if style.flags.contains(Flags::STRIKEOUT) {
        let sy = y + cell_h * 0.5;
        painter.line_segment([Pos2::new(x, sy), Pos2::new(x + width, sy)], Stroke::new(1.0, fg));
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_cursor(
    painter: &egui::Painter,
    rect: Rect,
    term: &alacritty_terminal::term::Term<crate::session::EventProxy>,
    config: &Config,
    cursor_point: Point,
    cursor_visible_line: i32,
    cell_w: f32,
    cell_h: f32,
    font_id: &FontId,
    shape: alacritty_terminal::vte::ansi::CursorShape,
) {
    use alacritty_terminal::vte::ansi::CursorShape::*;
    if matches!(shape, Hidden) {
        return;
    }

    let runtime_palette = term.colors();
    let grid = term.grid();
    let cell = &grid[Line(cursor_point.line.0)][cursor_point.column];

    let x = rect.min.x + cursor_point.column.0 as f32 * cell_w;
    let y = rect.min.y + cursor_visible_line as f32 * cell_h;
    let cursor_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h));

    let cursor_color = runtime_palette[alacritty_terminal::vte::ansi::NamedColor::Cursor]
        .map(rgb_to_color32)
        .or_else(|| config.palette.cursor_bg.map(rgb_to_color32))
        .unwrap_or_else(|| foreground(&config.palette));

    match shape {
        Block => {
            painter.rect_filled(cursor_rect, 0.0, cursor_color);
        },
        HollowBlock => {
            painter.rect_stroke(
                cursor_rect,
                0.0,
                Stroke::new(1.0, cursor_color),
                egui::StrokeKind::Inside,
            );
        },
        Beam => {
            let bar = Rect::from_min_size(Pos2::new(x, y), Vec2::new(2.0, cell_h));
            painter.rect_filled(bar, 0.0, cursor_color);
        },
        Underline => {
            let bar = Rect::from_min_size(Pos2::new(x, y + cell_h - 2.0), Vec2::new(cell_w, 2.0));
            painter.rect_filled(bar, 0.0, cursor_color);
        },
        Hidden => return,
    }

    // The solid block covers the glyph; redraw it in inverted color so it stays legible.
    if matches!(shape, Block) && cell.c != '\0' && !cell.flags.contains(Flags::HIDDEN) {
        let glyph_color = config.palette.cursor_fg.map(rgb_to_color32).unwrap_or_else(|| {
            rgb_to_color32(resolve(cell.bg, cell.flags, runtime_palette, &config.palette, false))
        });
        let glyph_color =
            if glyph_color == cursor_color { background(&config.palette) } else { glyph_color };
        painter.text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            cell.c.to_string(),
            font_for_flags(cell.flags, font_id),
            glyph_color,
        );
    }
}
