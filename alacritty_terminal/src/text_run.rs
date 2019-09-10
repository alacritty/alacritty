use crate::cursor::CursorKey;
use crate::index::{Column, Line, Point};
use crate::term::{
    cell::{Flags, MAX_ZEROWIDTH_CHARS},
    color::Rgb,
    RenderableCell, RenderableCellContent,
};

#[derive(Debug)]
struct RunStart {
    line: Line,
    column: Column,
    fg: Rgb,
    bg: Rgb,
    bg_alpha: f32,
    flags: Flags,
}
impl RunStart {
    /// Compare cell and check if it belongs to the same run.
    fn is_adjacent_to_cell(&self, rc: &RenderableCell) -> bool {
        self.line == rc.line
            && self.fg == rc.fg
            && self.bg == rc.bg
            && (self.bg_alpha - rc.bg_alpha).abs() < std::f32::EPSILON
            && self.flags == rc.flags
    }
}

#[derive(Debug)]
pub enum TextRunContent {
    Cursor(CursorKey),
    CharRun(String, Vec<[char; MAX_ZEROWIDTH_CHARS]>),
}

/// Represents a set of renderable cells that all share the same rendering propreties.
/// The assumption is that if two cells are in the same TextRun they can be sent off together to
/// be shaped. This allows for ligatures to be rendered but not when something breaks up a ligature
/// (e.g. selection hightlight) which is desired behavior.
#[derive(Debug)]
pub struct TextRun {
    /// A run never spans multiple lines.
    pub line: Line,
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
    fn from_iter_state(
        start: RunStart,
        (latest, is_wide): LatestCol,
        buffer: (String, Vec<[char; MAX_ZEROWIDTH_CHARS]>),
    ) -> Self {
        let end_column = if is_wide { latest + 1 } else { latest };
        TextRun {
            line: start.line,
            span: (start.column, end_column),
            content: TextRunContent::CharRun(buffer.0, buffer.1),
            fg: start.fg,
            bg: start.bg,
            bg_alpha: start.bg_alpha,
            flags: start.flags,
        }
    }

    fn from_cursor_rc(start: RunStart, cursor: CursorKey) -> Self {
        TextRun {
            line: start.line,
            span: (start.column, start.column),
            content: TextRunContent::Cursor(cursor),
            fg: start.fg,
            bg: start.bg,
            bg_alpha: start.bg_alpha,
            flags: start.flags,
        }
    }

    /// Holdover method while converting from rendering Cells to TextRuns
    fn cell_at(&self, col: Column) -> RenderableCell {
        RenderableCell {
            line: self.line,
            column: col,
            inner: RenderableCellContent::Chars([' '; crate::term::cell::MAX_ZEROWIDTH_CHARS + 1]),
            fg: self.fg,
            bg: self.bg,
            bg_alpha: self.bg_alpha,
            flags: self.flags,
        }
    }

    /// First cell in the TextRun
    pub fn start_cell(&self) -> RenderableCell {
        self.cell_at(self.span.0)
    }

    /// First point covered by this TextRun
    pub fn start_point(&self) -> Point {
        Point { line: self.line, col: self.span.0 }
    }

    /// End point covered by this TextRun
    pub fn end_point(&self) -> Point {
        Point { line: self.line, col: self.span.1 }
    }

    /// Iterates over each RenderableCell in column range [run.0, run.1]
    pub fn cells<'a>(&'a self) -> impl Iterator<Item = RenderableCell> + 'a {
        let step = if self.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
        let (Column(start), Column(end)) = self.span;
        // TODO: impl Step for Column (once Step is stable) to avoid unwrapping then rewrapping.
        (start..=end).step_by(step).map(move |col| self.cell_at(Column(col)))
    }
}

type IsWide = bool;
type LatestCol = (Column, IsWide);

/// Wraps an Iterator<Item=RenderableCell> and produces TextRuns to represent batches of cells
pub struct TextRunIter<I> {
    iter: I,
    run_start: Option<RunStart>,
    latest_col: Option<LatestCol>,
    cursor: Option<CursorKey>,
    buffer_text: String,
    buffer_zero_width: Vec<[char; MAX_ZEROWIDTH_CHARS]>,
}

// This is an explicit function (as opposed to a closure) to make it easy to use as a function
// pointer.
fn is_not_wide_char_spacer(rc: &RenderableCell) -> bool {
    !rc.flags.contains(Flags::WIDE_CHAR_SPACER)
}
impl<BaseIter> TextRunIter<std::iter::Filter<BaseIter, fn(&RenderableCell) -> bool>>
where
    BaseIter: Iterator<Item = RenderableCell>,
{
    pub fn new(iter: BaseIter) -> Self {
        TextRunIter {
            // Logic for WIDE_CHAR is handled internally by TextRun
            // So we no longer need WIDE_CHAR_SPACER at this point.
            iter: iter.filter(is_not_wide_char_spacer),
            latest_col: None,
            run_start: None,
            cursor: None,
            buffer_text: String::new(),
            buffer_zero_width: Vec::new(),
        }
    }
}
impl<I> TextRunIter<I> {
    /// Check if current run ends at incoming RenderableCell
    /// Run will not include incoming RenderableCell if it ends
    fn is_end_of_run(&self, rc: &RenderableCell) -> bool {
        let is_cell_not_adjacent = self
            .run_start
            .as_ref()
            .map(|run_start| !run_start.is_adjacent_to_cell(rc))
            .unwrap_or(false);
        let is_col_not_adjacent = self
            .latest_col
            .as_ref()
            .map(|&(col, is_wide)| {
                let width = if is_wide { 2usize } else { 1usize };
                col + width != rc.column && rc.column + width != col
            })
            .unwrap_or(false);
        is_cell_not_adjacent || is_col_not_adjacent
    }

    /// Add content of cell to pending TextRun buffer
    fn buffer_content(&mut self, inner: RenderableCellContent) {
        // Add to buffer only if the next rc is a Char (not a cursor)
        match inner {
            RenderableCellContent::Chars(chars) => {
                self.buffer_text.push(chars[0]);
                let mut arr: [char; MAX_ZEROWIDTH_CHARS] = Default::default();
                arr.copy_from_slice(&chars[1..]);
                self.buffer_zero_width.push(arr);
            },
            RenderableCellContent::Cursor(cursor) => {
                self.cursor = Some(cursor);
            },
        }
    }

    /// Empty out pending buffer producing owned collections that can be moved into a TextRun
    fn drain_buffer(&mut self) -> (String, Vec<[char; MAX_ZEROWIDTH_CHARS]>) {
        (self.buffer_text.drain(..).collect(), self.buffer_zero_width.drain(..).collect())
    }

    /// Start a new run by setting latest_col, run_start, and buffering content of rc
    /// Returns the previous runs run_start and latest_col data if available.
    fn start_run(&mut self, rc: RenderableCell) -> (Option<RunStart>, Option<LatestCol>) {
        self.buffer_content(rc.inner);
        let latest = self.latest_col.replace((rc.column, rc.flags.contains(Flags::WIDE_CHAR)));
        let start = self.run_start.replace(RunStart {
            line: rc.line,
            column: rc.column,
            fg: rc.fg,
            bg: rc.bg,
            bg_alpha: rc.bg_alpha,
            flags: rc.flags,
        });
        (start, latest)
    }

    /// Produce a run containing a single cursor from state of the `TextRunIter`.
    /// This is a destructive operation, the iterator will be in a new run state after it's
    /// completion.
    fn produce_cursor(&mut self, rc: RenderableCell) -> Option<TextRun> {
        let (opt_start, _) = self.start_run(rc);
        let start = opt_start?;
        let cursor = self.cursor.take()?;
        Some(TextRun::from_cursor_rc(start, cursor))
    }

    /// Create a run of chars from the current state of the `TextRunIter`.
    /// This is a destructive operation, the iterator will be in a new run state after it's
    /// completion.
    fn produce_char_run(&mut self, rc: RenderableCell) -> Option<TextRun> {
        let prev_buffer = self.drain_buffer();
        let (start_opt, latest_col_opt) = self.start_run(rc);
        let start = start_opt?;
        let latest_col = latest_col_opt?;
        Some(TextRun::from_iter_state(start, latest_col, prev_buffer))
    }
}

impl<I> Iterator for TextRunIter<I>
where
    I: Iterator<Item = RenderableCell>,
{
    type Item = TextRun;

    fn next(&mut self) -> Option<Self::Item> {
        let mut output = None;
        while let Some(rc) = self.iter.next() {
            if self.latest_col.is_none() || self.run_start.is_none() {
                // Initial state, this is should only be hit on the first next() call of
                // iterator
                self.run_start = Some(RunStart {
                    line: rc.line,
                    column: rc.column,
                    fg: rc.fg,
                    bg: rc.bg,
                    bg_alpha: rc.bg_alpha,
                    flags: rc.flags,
                })
            } else if self.cursor.is_some() {
                // Last iteration of the loop found a cursor
                // Return a run for the cursor and start a new run
                output = self.produce_cursor(rc);
                break;
            } else if self.is_end_of_run(&rc) || rc.is_cursor() {
                // If we find a run break or a cursor,
                // return what we have so far and start a new run.
                output = self.produce_char_run(rc);
                break;
            }
            // Build up buffer and track the latest column we've seen
            self.latest_col = Some((rc.column, rc.flags.contains(Flags::WIDE_CHAR)));
            self.buffer_content(rc.inner);
        }
        // Check for any remaining buffered content and return it as a text run.
        // This is a destructive operation, it will return None after it excutes once.
        output.or_else(|| {
            if !self.buffer_text.is_empty() || !self.buffer_zero_width.is_empty() {
                let start = self.run_start.take()?;
                let latest_col = self.latest_col.take()?;
                // Save leftover buffer and empty it
                Some(TextRun::from_iter_state(start, latest_col, self.drain_buffer()))
            } else if let Some(cursor) = self.cursor {
                let start = self.run_start.take()?;
                Some(TextRun::from_cursor_rc(start, cursor))
            } else {
                None
            }
        })
    }
}
