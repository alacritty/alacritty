use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Rgb;
use alacritty_terminal::term::search::Match;

use crate::display::content::{RenderableCell, RenderableContent};

#[derive(Debug)]
struct RunStart {
    line: usize,
    column: Column,
    fg: Rgb,
    bg: Rgb,
    bg_alpha: f32,
    flags: Flags,
}

impl RunStart {
    /// Compare cell and check if it belongs to the same run.
    #[inline]
    fn belongs_to_text_run(&self, render_cell: &RenderableCell) -> bool {
        self.line == render_cell.point.line
            && self.fg == render_cell.fg
            && self.bg == render_cell.bg
            && (self.bg_alpha - render_cell.bg_alpha).abs() < std::f32::EPSILON
            && self.flags == render_cell.flags
    }
}

#[derive(Debug)]
pub struct TextRunContent {
    pub text: String,
    pub zero_widths: Vec<Option<Vec<char>>>,
}

/// Represents a set of renderable cells that all share the same rendering propreties.
/// The assumption is that if two cells are in the same TextRun they can be sent off together to
/// be shaped. This allows for ligatures to be rendered but not when something breaks up a ligature
/// (e.g. selection hightlight) which is desired behavior.
#[derive(Debug)]
pub struct TextRun {
    /// A run never spans multiple lines.
    pub line: usize,
    /// Span of columns the text run covers.
    pub span: (Column, Column),
    /// Cursor or sequence of characters.
    pub content: TextRunContent,
    /// Foreground color of text run content.
    pub fg: Rgb,
    /// Background color of text run content.
    pub bg: Rgb,
    /// Background color opacity of the text run
    pub bg_alpha: f32,
    /// Attributes of this text run.
    pub flags: Flags,
}

impl TextRun {
    /// Returns dummy RenderableCell containing no content with positioning and color information
    /// from this TextRun.
    fn dummy_cell_at(&self, col: Column) -> RenderableCell {
        RenderableCell {
            point: Point { line: self.line, column: col },
            character: ' ',
            zerowidth: None,
            fg: self.fg,
            bg: self.bg,
            bg_alpha: self.bg_alpha,
            flags: self.flags,
        }
    }

    /// First point covered by this TextRun
    pub fn start_point(&self) -> Point<usize> {
        Point { line: self.line, column: self.span.0 }
    }

    /// End point covered by this TextRun
    pub fn end_point(&self) -> Point<usize> {
        Point { line: self.line, column: self.span.1 }
    }

    /// Iterates over each RenderableCell in column range [run.0, run.1]
    pub fn cells(&self) -> impl Iterator<Item = RenderableCell> + '_ {
        let step = if self.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
        let (Column(start), Column(end)) = self.span;
        // TODO: impl Step for Column (once Step is stable) to avoid unwrapping then rewrapping.
        (start..=end).step_by(step).map(move |col| self.dummy_cell_at(Column(col)))
    }
}

type IsWide = bool;
type LatestCol = (Column, IsWide);

/// Wraps an Iterator<Item=RenderableCell> and produces TextRuns to represent batches of cells
pub struct TextRunIter<I> {
    iter: I,
    run_start: Option<RunStart>,
    latest_col: Option<LatestCol>,
    display_offset: usize,
    hint: Option<Match>,
    vi_hint: Option<Match>,
    buffer_text: String,
    buffer_zero_width: Vec<Option<Vec<char>>>,
}

type TextRunIterFromContent<'a, 'c> =
    TextRunIter<std::iter::Filter<&'a mut RenderableContent<'c>, fn(&RenderableCell) -> bool>>;

impl<I> TextRunIter<I> {
    pub fn from_content<'a, 'c>(
        content: &'a mut RenderableContent<'c>,
        hint: Option<Match>,
        vi_hint: Option<Match>,
    ) -> TextRunIterFromContent<'a, 'c> {
        fn check(cell: &RenderableCell) -> bool {
            !cell.flags.contains(Flags::WIDE_CHAR_SPACER)
        }

        let display_offset = content.display_offset();

        // Logic for WIDE_CHAR is handled internally by TextRun
        // So we no longer need WIDE_CHAR_SPACER at this point.
        TextRunIter::new(content.filter(check), hint, vi_hint, display_offset)
    }
}

impl<I> TextRunIter<I>
where
    I: Iterator<Item = RenderableCell>,
{
    pub fn new(
        iter: I,
        hint: Option<Match>,
        vi_hint: Option<Match>,
        display_offset: usize,
    ) -> Self {
        TextRunIter {
            iter,
            latest_col: None,
            display_offset,
            run_start: None,
            buffer_text: String::new(),
            buffer_zero_width: Vec::new(),
            hint,
            vi_hint,
        }
    }
}
impl<I> TextRunIter<I> {
    /// Check if the cell belongs to this text run. Returns `true` if it does not belong.
    fn cell_does_not_belong_to_run(&self, render_cell: &RenderableCell) -> bool {
        self.run_start
            .as_ref()
            .map(|run_start| !run_start.belongs_to_text_run(render_cell))
            .unwrap_or_default()
    }

    /// Check if the column is not adjacent to the latest column.
    fn is_col_not_adjacent(&self, column: Column) -> bool {
        self.latest_col
            .as_ref()
            .map(|&(col, is_wide)| {
                let width = if is_wide { 2 } else { 1 };
                col + width != column && column + width != col
            })
            .unwrap_or_default()
    }

    /// Check if current run ends at incoming RenderableCell
    /// Run will not include incoming RenderableCell if it ends
    fn is_end_of_run(&self, render_cell: &RenderableCell) -> bool {
        self.cell_does_not_belong_to_run(render_cell)
            || self.is_col_not_adjacent(render_cell.point.column)
    }

    /// Add content of cell to pending TextRun buffer
    fn buffer_content(&mut self, cell: RenderableCell) {
        // Add to buffer only if the next RenderableCell is a Chars (not a cursor)
        self.buffer_text.push(cell.character);
        self.buffer_zero_width.push(cell.zerowidth);
    }

    /// Empty out pending buffer producing owned collections that can be moved into a TextRun
    fn drain_buffer(&mut self) -> TextRunContent {
        use std::mem::take;
        let text = take(&mut self.buffer_text);
        let zero_widths = take(&mut self.buffer_zero_width);

        TextRunContent { text, zero_widths }
    }

    fn is_hinted(&self, point: Point<usize>) -> bool {
        fn viewport_to_point(display_offset: usize, point: Point<usize>) -> Point {
            let line = Line(point.line as i32) - display_offset;
            Point::new(line, point.column)
        }

        let pt = viewport_to_point(self.display_offset, point);

        self.hint.as_ref().map_or(false, |bounds| bounds.contains(&pt))
            || self.vi_hint.as_ref().map_or(false, |bounds| bounds.contains(&pt))
    }

    /// Start a new run by setting latest_col, run_start, and buffering content of rc
    /// Returns the previous runs run_start and latest_col data if available.
    fn start_run(&mut self, render_cell: RenderableCell) -> (Option<RunStart>, Option<LatestCol>) {
        let latest = self
            .latest_col
            .replace((render_cell.point.column, render_cell.flags.contains(Flags::WIDE_CHAR)));
        let start = self.run_start.replace(RunStart {
            line: render_cell.point.line,
            column: render_cell.point.column,
            fg: render_cell.fg,
            bg: render_cell.bg,
            bg_alpha: render_cell.bg_alpha,
            flags: render_cell.flags,
        });
        self.buffer_content(render_cell);
        (start, latest)
    }

    /// Create a run of chars from the current state of the `TextRunIter`.
    /// This is a destructive operation, the iterator will be in a new run state after it's
    /// completion.
    fn produce_char_run(&mut self, render_cell: RenderableCell) -> Option<TextRun> {
        let prev_buffer = self.drain_buffer();
        let (start_opt, latest_col_opt) = self.start_run(render_cell);
        let start = start_opt?;
        let latest_col = latest_col_opt?;
        Some(Self::build_text_run(start, latest_col, prev_buffer))
    }

    /// Build a TextRun instance from passed state of TextRunIter
    fn build_text_run(
        start: RunStart,
        (latest, is_wide): LatestCol,
        content: TextRunContent,
    ) -> TextRun {
        let end_column = if is_wide { latest + 1 } else { latest };
        TextRun {
            line: start.line,
            span: (start.column, end_column),
            content,
            fg: start.fg,
            bg: start.bg,
            bg_alpha: start.bg_alpha,
            flags: start.flags,
        }
    }
}

impl<I> Iterator for TextRunIter<I>
where
    I: Iterator<Item = RenderableCell>,
{
    type Item = TextRun;

    fn next(&mut self) -> Option<Self::Item> {
        let mut output = None;
        while let Some(mut render_cell) = self.iter.next() {
            if self.is_hinted(render_cell.point) {
                render_cell.flags.insert(Flags::UNDERLINE);
            }
            if self.latest_col.is_none() || self.run_start.is_none() {
                // Initial state, this is should only be hit on the first next() call of
                // iterator

                self.run_start = Some(RunStart {
                    line: render_cell.point.line,
                    column: render_cell.point.column,
                    fg: render_cell.fg,
                    bg: render_cell.bg,
                    bg_alpha: render_cell.bg_alpha,
                    flags: render_cell.flags,
                });
            } else if self.is_end_of_run(&render_cell) {
                // If we find a run break,
                // return what we have so far and start a new run.
                output = self.produce_char_run(render_cell);
                break;
            }

            // Build up buffer and track the latest column we've seen
            self.latest_col =
                Some((render_cell.point.column, render_cell.flags.contains(Flags::WIDE_CHAR)));
            self.buffer_content(render_cell);
        }
        // Check for any remaining buffered content and return it as a text run.
        // This is a destructive operation, it will return None after it excutes once.
        output.or_else(|| {
            if !self.buffer_text.is_empty() || !self.buffer_zero_width.is_empty() {
                let start = self.run_start.take()?;
                let latest_col = self.latest_col.take()?;
                // Save leftover buffer and empty it
                Some(Self::build_text_run(start, latest_col, self.drain_buffer()))
            } else {
                None
            }
        })
    }
}
