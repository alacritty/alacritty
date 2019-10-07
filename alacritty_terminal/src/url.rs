use crate::ansi::TermInfo;
use crate::index::{Column, Linear, Point};
use crate::term::Term;
use std::ops::RangeInclusive;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
pub struct Url {
    pub start: Point<usize>,
    pub end: Point<usize>,
}

impl Url {
    pub fn new(start: Point<usize>, length: usize, num_cols: usize) -> Self {
        let unwrapped_end_col = start.col.0 + length - 1;
        let end_col = unwrapped_end_col % num_cols;
        let end_line = start.line - unwrapped_end_col / num_cols;

        Url { end: Point::new(end_line, Column(end_col)), start }
    }

    /// Check if point is within this URL
    pub fn contains(&self, point: impl Into<Point<usize>>) -> bool {
        let point = point.into();

        point.line <= self.start.line
            && point.line >= self.end.line
            && (point.line != self.start.line || point.col >= self.start.col)
            && (point.line != self.end.line || point.col <= self.end.col)
    }

    /// Convert URLs bounding points to linear indices
    pub fn linear_bounds<T>(&self, terminal: &Term<T>) -> RangeInclusive<Linear> {
        let mut start = self.start;
        let mut end = self.end;

        start = terminal.buffer_to_visible(start);
        end = terminal.buffer_to_visible(end);

        let start = Linear::from_point(terminal.cols(), start);
        let end = Linear::from_point(terminal.cols(), end);

        RangeInclusive::new(start, end)
    }
}
