use super::{
    cell::{Flags, MAX_ZEROWIDTH_CHARS},
    color::Rgb,
    CursorKey, Point, RenderableCell, RenderableCellContent,
};
use crate::index::{Column, Line};

#[derive(Debug)]
struct RunStart {
    pub line: Line,
    pub column: Column,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bg_alpha: f32,
    pub flags: Flags,
}

// Use a macro instead of a function to make use of partial move semantics that don't cross
// function boundaries
// Convert a RenderableCell into a RunStart
macro_rules! from_rc {
    ($rc:ident) => {
        RunStart {
            line: $rc.line,
            column: $rc.column,
            fg: $rc.fg,
            bg: $rc.bg,
            bg_alpha: $rc.bg_alpha,
            flags: $rc.flags,
        }
    };
}

/// Compare cells and check they are in the same text run
fn is_adjacent_context(a: &RunStart, b: &RenderableCell) -> bool {
    a.line == b.line
        && a.fg == b.fg
        && a.bg == b.bg
        && (a.bg_alpha - b.bg_alpha).abs() < std::f32::EPSILON
        && a.flags == b.flags
}

type LatestCol = (Column, bool);
/// Checks two columns are adjacent
fn is_adjacent_col((a, is_wide): LatestCol, b: Column) -> bool {
    let width = if is_wide { 2usize } else { 1usize };
    a + width == b || b + width == a
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
    /// By definition a run is on one line.
    pub line: Line,
    /// Span of columns the text run covers.
    pub span: (Column, Column),
    /// Cursor or sequence of characters.
    pub content: TextRunContent,
    /// Foreground color of text run content.
    pub fg: Rgb,
    /// Background color of text run content.
    pub bg: Rgb,
    /// Opacity of background color of text run content.
    pub bg_alpha: f32,
    /// Attributes of this text run (e.g. HIDDEN, STRIKETHROUGH, etc.)
    pub flags: Flags,
}
impl TextRun {
    // These two constructors are used by TextRunIter and are not widely applicable
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

    /// Number of columns this TextRun spans
    pub fn len(&self) -> usize {
        let (start, end) = self.span;
        end.0 - start.0
    }

    /// True if TextRun contains characters, false if it contains no characters.
    pub fn is_empty(&self) -> bool {
        match &self.content {
            TextRunContent::Cursor(_) => false,
            TextRunContent::CharRun(ref string, ref zero_widths) => {
                string.is_empty() && zero_widths.is_empty()
            },
        }
    }

    /// First column of the run
    pub fn start_col(&self) -> Column {
        self.span.0
    }

    /// First cell in the TextRun
    pub fn start_cell(&self) -> RenderableCell {
        self.cell_at(self.span.0)
    }

    /// End cell in the TextRun
    pub fn end_cell(&self) -> RenderableCell {
        self.cell_at(self.span.1)
    }

    /// First point covered by this TextRun
    pub fn start_point(&self) -> Point {
        Point { line: self.line, col: self.span.0 }
    }

    /// End point covered by this TextRun
    pub fn end_point(&self) -> Point {
        Point { line: self.line, col: self.span.1 }
    }

    /// Returns iterator over range of columns [run.0, run.1]
    pub fn cols(&self) -> impl Iterator<Item = Column> {
        let (start, end) = self.span;
        let step = if self.flags.contains(Flags::WIDE_CHAR) {
            // If our run contains ide chars treat each cell like it's 2 cells wide
            2
        } else {
            1
        };
        // unpacking is neccessary while Step trait is nightly
        // hopefully this compiles away.
        (start.0..=end.0).step_by(step).map(Column)
    }

    /// Iterates over each RenderableCell in column range [run.0, run.1]
    pub fn cells<'a>(&'a self) -> impl Iterator<Item = RenderableCell> + 'a {
        self.cols().map(move |col| self.cell_at(col))
    }
}

/// Wraps an Iterator<Item=RenderableCell> and produces TextRuns to represent batches of cells
pub struct TextRunIter<I> {
    iter: I,
    run_start: Option<RunStart>,
    latest: Option<LatestCol>,
    cursor: Option<CursorKey>,
    buffer_text: String,
    buffer_zero_width: Vec<[char; MAX_ZEROWIDTH_CHARS]>,
}
impl<I> TextRunIter<I> {
    pub fn new(iter: I) -> Self {
        TextRunIter {
            iter,
            latest: None,
            run_start: None,
            cursor: None,
            buffer_text: String::new(),
            buffer_zero_width: Vec::new(),
        }
    }

    /// Check if current run ends with incoming RenderableCell
    fn is_end_of_run(&self, rc: &RenderableCell) -> bool {
        let is_cell_adjacent =
            self.run_start.as_ref().map(|cell| !is_adjacent_context(cell, &rc)).unwrap_or(false);
        let is_col_adjacent =
            self.latest.as_ref().map(|col| !is_adjacent_col(*col, rc.column)).unwrap_or(false);
        is_cell_adjacent || is_col_adjacent
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

    /// Handles bookkeeping needed when starting a new run
    fn start_run(&mut self, rc: RenderableCell) -> (Option<RunStart>, Option<LatestCol>) {
        self.buffer_content(rc.inner);
        let latest = self.latest.replace((rc.column, rc.flags.contains(Flags::WIDE_CHAR)));
        let start = self.run_start.replace(from_rc!(rc));
        (start, latest)
    }

    /// Producer a run containing a single cursor from state of the `TextRunIter`.
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
        let (start_opt, latest_opt) = self.start_run(rc);
        let start = start_opt?;
        let latest = latest_opt?;
        Some(TextRun::from_iter_state(start, latest, prev_buffer))
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
            if self.latest.is_none() || self.run_start.is_none() {
                // Initial state, this is should only be hit on the first next() call of
                // iterator
                self.run_start = Some(from_rc!(rc));
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
            self.latest = Some((rc.column, rc.flags.contains(Flags::WIDE_CHAR)));
            self.buffer_content(rc.inner);
        }
        // If we generated output we're done.
        // Otherwise check for any remaining buffered content and return it as a text run.
        // This is a destructive operation, it will return None after it excutes once.
        output.or_else(|| {
            if !self.buffer_text.is_empty() || !self.buffer_zero_width.is_empty() {
                let start = self.run_start.take()?;
                let latest = self.latest.take()?;
                // Save leftover buffer and empty it
                Some(TextRun::from_iter_state(start, latest, self.drain_buffer()))
            } else if let Some(cursor) = self.cursor {
                let start = self.run_start.take()?;
                Some(TextRun::from_cursor_rc(start, cursor))
            } else {
                None
            }
        })
    }
}
