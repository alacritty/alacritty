use std::mem::replace;

use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Rgb;
use alacritty_terminal::term::search::Match;

use crate::display::content::{RenderableCell, RenderableContent};

#[derive(Debug, Default, Clone, Copy)]
struct RunStart {
    line: usize,
    column: Column,
    fg: Rgb,
    bg: Rgb,
    bg_alpha: f32,
    flags: Flags,
}

impl RunStart {
    fn new(cell: &RenderableCell) -> Self {
        Self {
            line: cell.point.line,
            column: cell.point.column,
            fg: cell.fg,
            bg: cell.bg,
            bg_alpha: cell.bg_alpha,
            flags: cell.flags,
        }
    }

    /// Compare cell and check if it belongs to the same run.
    #[inline]
    fn belongs_to_text_run(&self, render_cell: &RenderableCell) -> bool {
        self.line == render_cell.point.line
            && self.fg == render_cell.fg
            && self.flags == render_cell.flags
            && self.bg == render_cell.bg
            && (self.bg_alpha - render_cell.bg_alpha).abs() < f32::EPSILON
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

    /// Iterates over each RenderableCell in column range `[run.0, run.1]`
    pub fn cells(&self) -> impl Iterator<Item = RenderableCell> + '_ {
        let step = if self.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
        let (Column(start), Column(end)) = self.span;
        // TODO: impl Step for Column (once Step is stable) to avoid unwrapping then rewrapping.
        (start..=end).step_by(step).map(move |col| self.dummy_cell_at(Column(col)))
    }
}

#[derive(Default, Clone, Copy)]
pub struct LatestCol {
    column: Column,
    is_wide: bool,
}

impl LatestCol {
    #[inline]
    fn new(cell: &RenderableCell) -> Self {
        Self { column: cell.point.column, is_wide: cell.flags.contains(Flags::WIDE_CHAR) }
    }
}

/// Wraps an Iterator<Item=RenderableCell> and produces TextRuns to represent batches of cells
pub struct TextRunIter<I> {
    iter: I,
    run_start: RunStart,
    latest_col: LatestCol,
    display_offset: usize,
    hint: Option<Match>,
    vi_hint: Option<Match>,
    buffer_text: String,
    buffer_zero_width: Vec<Option<Vec<char>>>,
}

type TextRunIterFromContent<'a, 'c> =
    TextRunIter<std::iter::Filter<&'a mut RenderableContent<'c>, fn(&RenderableCell) -> bool>>;

impl<'a, 'c> TextRunIterFromContent<'a, 'c> {
    pub fn from_content(
        content: &'a mut RenderableContent<'c>,
        hint: Option<Match>,
        vi_hint: Option<Match>,
    ) -> Self {
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
        mut iter: I,
        hint: Option<Match>,
        vi_hint: Option<Match>,
        display_offset: usize,
    ) -> Self {
        if let Some(cell) = iter.next() {
            let latest_col = LatestCol::new(&cell);
            let run_start = RunStart::new(&cell);
            let buffer_text = cell.character.to_string();
            let buffer_zero_width = vec![cell.zerowidth];

            TextRunIter {
                iter,
                run_start,
                latest_col,
                display_offset,
                hint,
                vi_hint,
                buffer_text,
                buffer_zero_width,
            }
        } else {
            // There are no cells in the grid. This rarely happens.
            #[cold]
            #[inline]
            fn dummy<I>(iter: I) -> TextRunIter<I> {
                TextRunIter {
                    iter,
                    run_start: RunStart::default(),
                    latest_col: LatestCol::default(),
                    display_offset: 0,
                    hint: None,
                    vi_hint: None,
                    buffer_text: String::new(),
                    buffer_zero_width: Vec::new(),
                }
            }

            dummy(iter)
        }
    }
}
impl<I> TextRunIter<I> {
    /// Check if the cell belongs to this text run. Returns `true` if it does not belong.
    fn cell_does_not_belong_to_run(&self, render_cell: &RenderableCell) -> bool {
        !self.run_start.belongs_to_text_run(render_cell)
    }

    /// Check if the column is not adjacent to the latest column.
    fn is_col_not_adjacent(&self, col: Column) -> bool {
        let LatestCol { column, is_wide } = self.latest_col;

        let width = if is_wide { 2 } else { 1 };
        col + width != column && column + width != col
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
        let text = self.buffer_text.clone();
        self.buffer_text.clear();
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
    fn start_run(&mut self, render_cell: RenderableCell) -> (RunStart, LatestCol) {
        let prev_start = replace(&mut self.run_start, RunStart::new(&render_cell));
        let prev_latest = replace(&mut self.latest_col, LatestCol::new(&render_cell));

        self.buffer_content(render_cell);

        (prev_start, prev_latest)
    }

    /// Create a run of chars from the current state of the `TextRunIter`.
    /// This is a destructive operation, the iterator will be in a new run state after it's
    /// completion.
    fn produce_char_run(&mut self, render_cell: RenderableCell) -> TextRun {
        let prev_buffer = self.drain_buffer();
        let (start, latest_col) = self.start_run(render_cell);

        Self::build_text_run(start, latest_col, prev_buffer)
    }

    /// Build a TextRun instance from passed state of TextRunIter
    fn build_text_run(start: RunStart, latest_col: LatestCol, content: TextRunContent) -> TextRun {
        let end_column = latest_col.column + latest_col.is_wide as usize;
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
        while let Some(mut render_cell) = self.iter.next() {
            if self.is_hinted(render_cell.point) {
                render_cell.flags.insert(Flags::UNDERLINE);
            }
            if self.is_end_of_run(&render_cell) {
                // If we find a run break,
                // return what we have so far and start a new run.
                return Some(self.produce_char_run(render_cell));
            }

            // Build up buffer and track the latest column we've seen
            self.latest_col = LatestCol::new(&render_cell);
            self.buffer_content(render_cell);
        }

        // Check for any remaining buffered content and return it as a text run.
        // This is a destructive operation, it will return None after it excutes once.
        if !self.buffer_text.is_empty() || !self.buffer_zero_width.is_empty() {
            // Save leftover buffer and empty it
            Some(Self::build_text_run(self.run_start, self.latest_col, self.drain_buffer()))
        } else {
            None
        }
    }
}
