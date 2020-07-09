//! Grid resize and reflow.

use std::cmp::{min, Ordering};

use crate::index::{Column, Line};
use crate::term::cell::Flags;

use crate::grid::row::Row;
use crate::grid::{Dimensions, Grid, GridCell};

impl<T: GridCell + Default + PartialEq + Copy> Grid<T> {
    /// Resize the grid's width and/or height.
    pub fn resize(&mut self, reflow: bool, lines: Line, cols: Column) {
        match self.lines.cmp(&lines) {
            Ordering::Less => self.grow_lines(lines),
            Ordering::Greater => self.shrink_lines(lines),
            Ordering::Equal => (),
        }

        match self.cols.cmp(&cols) {
            Ordering::Less => self.grow_cols(reflow, cols),
            Ordering::Greater => self.shrink_cols(reflow, cols),
            Ordering::Equal => (),
        }
    }

    /// Add lines to the visible area.
    ///
    /// Alacritty keeps the cursor at the bottom of the terminal as long as there
    /// is scrollback available. Once scrollback is exhausted, new lines are
    /// simply added to the bottom of the screen.
    fn grow_lines(&mut self, new_line_count: Line) {
        let lines_added = new_line_count - self.lines;

        // Need to resize before updating buffer.
        self.raw.grow_visible_lines(new_line_count, Row::new(self.cols, T::default()));
        self.lines = new_line_count;

        let history_size = self.history_size();
        let from_history = min(history_size, lines_added.0);

        // Move existing lines up for every line that couldn't be pulled from history.
        if from_history != lines_added.0 {
            let delta = lines_added - from_history;
            self.scroll_up(&(Line(0)..new_line_count), delta, T::default());
        }

        // Move cursor down for every line pulled from history.
        self.saved_cursor.point.line += from_history;
        self.cursor.point.line += from_history;

        self.display_offset = self.display_offset.saturating_sub(*lines_added);
        self.decrease_scroll_limit(*lines_added);
    }

    /// Remove lines from the visible area.
    ///
    /// The behavior in Terminal.app and iTerm.app is to keep the cursor at the
    /// bottom of the screen. This is achieved by pushing history "out the top"
    /// of the terminal window.
    ///
    /// Alacritty takes the same approach.
    fn shrink_lines(&mut self, target: Line) {
        // Scroll up to keep content inside the window.
        let required_scrolling = (self.cursor.point.line + 1).saturating_sub(target.0);
        if required_scrolling > 0 {
            self.scroll_up(&(Line(0)..self.lines), Line(required_scrolling), T::default());

            // Clamp cursors to the new viewport size.
            self.cursor.point.line = min(self.cursor.point.line, target - 1);
        }

        // Clamp saved cursor, since only primary cursor is scrolled into viewport.
        self.saved_cursor.point.line = min(self.saved_cursor.point.line, target - 1);

        self.raw.rotate((self.lines - target).0 as isize);
        self.raw.shrink_visible_lines(target);
        self.lines = target;
    }

    /// Grow number of columns in each row, reflowing if necessary.
    fn grow_cols(&mut self, reflow: bool, cols: Column) {
        // Check if a row needs to be wrapped.
        let should_reflow = |row: &Row<T>| -> bool {
            let len = Column(row.len());
            reflow && len.0 > 0 && len < cols && row[len - 1].flags().contains(Flags::WRAPLINE)
        };

        self.cols = cols;

        let mut reversed: Vec<Row<T>> = Vec::with_capacity(self.raw.len());
        let mut cursor_line_delta = 0;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.input_needs_wrap && reflow {
            self.cursor.input_needs_wrap = false;
            self.cursor.point.col += 1;
        }

        let mut rows = self.raw.take_all();

        for (i, mut row) in rows.drain(..).enumerate().rev() {
            // Check if reflowing should be performed.
            let last_row = match reversed.last_mut() {
                Some(last_row) if should_reflow(last_row) => last_row,
                _ => {
                    reversed.push(row);
                    continue;
                },
            };

            // Remove wrap flag before appending additional cells.
            if let Some(cell) = last_row.last_mut() {
                cell.flags_mut().remove(Flags::WRAPLINE);
            }

            // Remove leading spacers when reflowing wide char to the previous line.
            let mut last_len = last_row.len();
            if last_len >= 1
                && last_row[Column(last_len - 1)].flags().contains(Flags::LEADING_WIDE_CHAR_SPACER)
            {
                last_row.shrink(Column(last_len - 1));
                last_len -= 1;
            }

            // Don't try to pull more cells from the next line than available.
            let mut num_wrapped = cols.0 - last_len;
            let len = min(row.len(), num_wrapped);

            // Insert leading spacer when there's not enough room for reflowing wide char.
            let mut cells = if row[Column(len - 1)].flags().contains(Flags::WIDE_CHAR) {
                num_wrapped -= 1;

                let mut cells = row.front_split_off(len - 1);

                let mut spacer = T::default();
                spacer.flags_mut().insert(Flags::LEADING_WIDE_CHAR_SPACER);
                cells.push(spacer);

                cells
            } else {
                row.front_split_off(len)
            };

            // Add removed cells to previous row and reflow content.
            last_row.append(&mut cells);

            let cursor_buffer_line = (self.lines - self.cursor.point.line - 1).0;
            let mut is_clear = row.is_clear();

            if i == cursor_buffer_line && reflow {
                // Resize cursor's line and reflow the cursor if necessary.
                let mut target = self.cursor.point.sub(cols, num_wrapped);

                // Clamp to the last column, if no content was reflown with the cursor.
                if target.col.0 == 0 {
                    self.cursor.input_needs_wrap = true;
                    target = target.sub(cols, 1);
                }
                self.cursor.point.col = target.col;

                let line_delta = (self.cursor.point.line - target.line).0;
                cursor_line_delta += line_delta;
                is_clear &= line_delta != 0;
            } else if is_clear {
                if i + reversed.len() >= self.lines.0 {
                    // Since we removed a line, rotate down the viewport.
                    self.display_offset = self.display_offset.saturating_sub(1);

                    // Rotate cursor down if content below them was pulled from history.
                    if i < cursor_buffer_line {
                        self.cursor.point.line += 1;
                    }
                }

                // Don't push line into the new buffer.
                continue;
            }

            if let (Some(cell), false) = (last_row.last_mut(), is_clear) {
                // Set wrap flag if next line still has cells.
                cell.flags_mut().insert(Flags::WRAPLINE);
            }

            reversed.push(row);
        }

        // Make sure we have at least the viewport filled.
        if reversed.len() < self.lines.0 {
            let delta = self.lines.0 - reversed.len();
            self.cursor.point.line.0 = self.cursor.point.line.saturating_sub(delta);
            reversed.append(&mut vec![Row::new(cols, T::default()); delta]);
        }

        // Pull content down to put cursor in correct position, or move cursor up if there's no
        // more lines to delete below the cursor.
        if cursor_line_delta != 0 {
            let cursor_buffer_line = (self.lines - self.cursor.point.line - 1).0;
            let available = min(cursor_buffer_line, reversed.len() - self.lines.0);
            let overflow = cursor_line_delta.saturating_sub(available);
            reversed.truncate(reversed.len() + overflow - cursor_line_delta);
            self.cursor.point.line.0 = self.cursor.point.line.saturating_sub(overflow);
        }

        // Reverse iterator and fill all rows that are still too short.
        let mut new_raw = Vec::with_capacity(reversed.len());
        for mut row in reversed.drain(..).rev() {
            if row.len() < cols.0 {
                row.grow(cols, T::default());
            }
            new_raw.push(row);
        }

        self.raw.replace_inner(new_raw);

        // Clamp display offset in case lines above it got merged.
        self.display_offset = min(self.display_offset, self.history_size());
    }

    /// Shrink number of columns in each row, reflowing if necessary.
    fn shrink_cols(&mut self, reflow: bool, cols: Column) {
        self.cols = cols;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.input_needs_wrap && reflow {
            self.cursor.input_needs_wrap = false;
            self.cursor.point.col += 1;
        }

        let mut new_raw = Vec::with_capacity(self.raw.len());
        let mut buffered: Option<Vec<T>> = None;

        let mut rows = self.raw.take_all();
        for (i, mut row) in rows.drain(..).enumerate().rev() {
            // Append lines left over from the previous row.
            if let Some(buffered) = buffered.take() {
                // Add a column for every cell added before the cursor, if it goes beyond the new
                // width it is then later reflown.
                let cursor_buffer_line = (self.lines - self.cursor.point.line - 1).0;
                if i == cursor_buffer_line {
                    self.cursor.point.col += buffered.len();
                }

                row.append_front(buffered);
            }

            loop {
                // Remove all cells which require reflowing.
                let mut wrapped = match row.shrink(cols) {
                    Some(wrapped) if reflow => wrapped,
                    _ => {
                        let cursor_buffer_line = (self.lines - self.cursor.point.line - 1).0;
                        if reflow && i == cursor_buffer_line && self.cursor.point.col > cols {
                            // If there are empty cells before the cursor, we assume it is explicit
                            // whitespace and need to wrap it like normal content.
                            Vec::new()
                        } else {
                            // Since it fits, just push the existing line without any reflow.
                            new_raw.push(row);
                            break;
                        }
                    },
                };

                // Insert spacer if a wide char would be wrapped into the last column.
                if row.len() >= cols.0 && row[cols - 1].flags().contains(Flags::WIDE_CHAR) {
                    wrapped.insert(0, row[cols - 1]);

                    let mut spacer = T::default();
                    spacer.flags_mut().insert(Flags::LEADING_WIDE_CHAR_SPACER);
                    row[cols - 1] = spacer;
                }

                // Remove wide char spacer before shrinking.
                let len = wrapped.len();
                if wrapped[len - 1].flags().contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                    if len == 1 {
                        row[cols - 1].flags_mut().insert(Flags::WRAPLINE);
                        new_raw.push(row);
                        break;
                    } else {
                        // Remove the leading spacer from the end of the wrapped row.
                        wrapped[len - 2].flags_mut().insert(Flags::WRAPLINE);
                        wrapped.truncate(len - 1);
                    }
                }

                new_raw.push(row);

                // Set line as wrapped if cells got removed.
                if let Some(cell) = new_raw.last_mut().and_then(|r| r.last_mut()) {
                    cell.flags_mut().insert(Flags::WRAPLINE);
                }

                if wrapped
                    .last()
                    .map(|c| c.flags().contains(Flags::WRAPLINE) && i >= 1)
                    .unwrap_or(false)
                    && wrapped.len() < cols.0
                {
                    // Make sure previous wrap flag doesn't linger around.
                    if let Some(cell) = wrapped.last_mut() {
                        cell.flags_mut().remove(Flags::WRAPLINE);
                    }

                    // Add removed cells to start of next row.
                    buffered = Some(wrapped);
                    break;
                } else {
                    // Reflow the cursor if it is on this line beyond the width.
                    let cursor_buffer_line = (self.lines - self.cursor.point.line - 1).0;
                    if cursor_buffer_line == i && self.cursor.point.col >= cols {
                        // Since only a single new line is created, we subtract only `cols` from
                        // the cursor instead of reflowing it completely.
                        self.cursor.point.col -= cols;
                    }

                    // Make sure new row is at least as long as new width.
                    let occ = wrapped.len();
                    if occ < cols.0 {
                        wrapped.append(&mut vec![T::default(); cols.0 - occ]);
                    }
                    row = Row::from_vec(wrapped, occ);
                }
            }
        }

        // Reverse iterator and use it as the new grid storage.
        let mut reversed: Vec<Row<T>> = new_raw.drain(..).rev().collect();
        reversed.truncate(self.max_scroll_limit + self.lines.0);
        self.raw.replace_inner(reversed);

        // Reflow the primary cursor, or clamp it if reflow is disabled.
        if !reflow {
            self.cursor.point.col = min(self.cursor.point.col, cols - 1);
        } else if self.cursor.point.col == cols {
            self.cursor.input_needs_wrap = true;
            self.cursor.point.col -= 1;
        } else {
            self.cursor.point = self.cursor.point.add(cols, 0);
        }

        // Clamp the saved cursor to the grid.
        self.saved_cursor.point.col = min(self.saved_cursor.point.col, cols - 1);
    }
}
