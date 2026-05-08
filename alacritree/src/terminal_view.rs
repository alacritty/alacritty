use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::Color as AnsiColor;
use egui::{Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::colors::{background, foreground, resolve, rgb_to_color32};
use crate::config::Config;
use crate::input::event_to_bytes;
use crate::session::{Session, TermSize};

pub fn show(ui: &mut Ui, session: &mut Session, config: &Config) -> Response {
    let font_id = FontId::monospace(config.font.size);
    let (cell_w, cell_h) = ui.ctx().fonts(|f| {
        let w = f.glyph_width(&font_id, 'M');
        let h = f.row_height(&font_id);
        (w, h)
    });

    // Honor [window].padding so the terminal grid sits inset from the panel
    // edges, matching alacritty.
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
        Vec2::new(cols as f32 * cell_w + 2.0 * pad_x, rows as f32 * cell_h + (if pad_y > 0.0 { 0.0 } else { 0.0 })),
        Sense::click_and_drag(),
    );
    let rect = Rect::from_min_size(
        Pos2::new(rect.min.x + pad_x, rect.min.y),
        Vec2::new(cols as f32 * cell_w, rows as f32 * cell_h),
    );

    if !response.has_focus() {
        response.request_focus();
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, background(&config.palette));

    drain_pty_events(session);
    paint_grid(&painter, rect, session, config, &font_id, cell_w, cell_h);

    if response.has_focus() {
        let consumed: Vec<Vec<u8>> = ui.input(|i| {
            i.events.iter().filter_map(event_to_bytes).collect()
        });
        for bytes in consumed {
            session.write(bytes);
        }
    }

    response
}

fn drain_pty_events(session: &mut Session) {
    use alacritty_terminal::event::Event as TermEvent;
    while let Ok(event) = session.events.try_recv() {
        match event {
            TermEvent::PtyWrite(s) => session.write(s.into_bytes()),
            TermEvent::Title(t) => session.title = t,
            TermEvent::ChildExit(_) => session.mark_exited(),
            _ => {}
        }
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

    for row_idx in 0..screen_lines {
        let line = Line(row_idx - display_offset);
        let row = &grid[line];
        let y = rect.min.y + row_idx as f32 * cell_h;

        let mut col = 0;
        while col < cols {
            let start = col;
            let style = Style::from_cell(&row[Column(col)]);
            let mut run = String::new();
            while col < cols {
                let cell = &row[Column(col)];
                if Style::from_cell(cell) != style {
                    break;
                }
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    col += 1;
                    continue;
                }
                let ch = if cell.c == '\0' || cell.flags.contains(Flags::HIDDEN) {
                    ' '
                } else {
                    cell.c
                };
                run.push(ch);
                col += 1;
            }
            paint_run(
                painter, rect, &run, start, y, cell_w, cell_h, style, runtime_palette, config,
                font_id, bg_color,
            );
        }
    }

    if cursor_visible_line >= 0 && cursor_visible_line < screen_lines {
        let cursor_shape = term.cursor_style().shape;
        paint_cursor(
            painter, rect, &term, config, cursor_point, cursor_visible_line, cell_w, cell_h,
            font_id, cursor_shape,
        );
    }
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
) {
    if run.is_empty() {
        return;
    }
    let inverse = style.flags.contains(Flags::INVERSE);
    let fg_rgb = resolve(
        if inverse { style.bg } else { style.fg },
        style.flags,
        runtime,
        &config.palette,
        true,
    );
    let bg_rgb = resolve(
        if inverse { style.fg } else { style.bg },
        style.flags,
        runtime,
        &config.palette,
        false,
    );
    let fg = rgb_to_color32(fg_rgb);
    let bg = rgb_to_color32(bg_rgb);

    let width = run.chars().count() as f32 * cell_w;
    let x = rect.min.x + start_col as f32 * cell_w;
    let bg_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, cell_h));

    if bg != default_bg {
        painter.rect_filled(bg_rect, 0.0, bg);
    }

    if !style.flags.contains(Flags::HIDDEN) {
        painter.text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            run,
            font_id.clone(),
            fg,
        );
    }

    if style.flags.intersects(Flags::ALL_UNDERLINES) {
        let uy = y + cell_h - 1.5;
        painter.line_segment(
            [Pos2::new(x, uy), Pos2::new(x + width, uy)],
            Stroke::new(1.0, fg),
        );
    }
    if style.flags.contains(Flags::STRIKEOUT) {
        let sy = y + cell_h * 0.5;
        painter.line_segment(
            [Pos2::new(x, sy), Pos2::new(x + width, sy)],
            Stroke::new(1.0, fg),
        );
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
        }
        HollowBlock => {
            painter.rect_stroke(
                cursor_rect,
                0.0,
                Stroke::new(1.0, cursor_color),
                egui::StrokeKind::Inside,
            );
        }
        Beam => {
            let bar = Rect::from_min_size(Pos2::new(x, y), Vec2::new(2.0, cell_h));
            painter.rect_filled(bar, 0.0, cursor_color);
        }
        Underline => {
            let bar = Rect::from_min_size(
                Pos2::new(x, y + cell_h - 2.0),
                Vec2::new(cell_w, 2.0),
            );
            painter.rect_filled(bar, 0.0, cursor_color);
        }
        Hidden => return,
    }

    // Re-paint the glyph on top so it stays legible — only meaningful for the
    // solid block; the other shapes don't cover the cell.
    if matches!(shape, Block) && cell.c != '\0' && !cell.flags.contains(Flags::HIDDEN) {
        let glyph_color = config
            .palette
            .cursor_fg
            .map(rgb_to_color32)
            .unwrap_or_else(|| {
                rgb_to_color32(resolve(
                    cell.bg,
                    cell.flags,
                    runtime_palette,
                    &config.palette,
                    false,
                ))
            });
        let glyph_color = if glyph_color == cursor_color {
            background(&config.palette)
        } else {
            glyph_color
        };
        painter.text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            cell.c.to_string(),
            font_id.clone(),
            glyph_color,
        );
    }
}
