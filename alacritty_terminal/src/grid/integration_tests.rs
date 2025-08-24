#[cfg(test)]
mod tests {
    use crate::grid::*;
    use crate::grid::storage::Swappable;
    use crate::index::Line;

    // Test struct for different cell types
    #[derive(Clone, Debug, Default, PartialEq)]
    struct TestCell {
        value: char,
    }

    impl TestCell {
        fn new(value: char) -> Self {
            Self { value }
        }
    }

    // Create a grid with a specific character
    fn create_test_grid(width: usize, height: usize, history_size: usize, ch: char) -> Grid<TestCell> {
        let mut grid = Grid::new(width, height, history_size);

        // Fill the grid with test rows
        for _ in 0..height {
            let mut row = Row::new();
            for _ in 0..width {
                row.push(TestCell::new(ch));
            }
            grid.insert_line(Line(0));

            let cells = grid.cursor.cells_mut();
            for cell in cells {
                *cell = TestCell::new(ch);
            }
        }

        grid
    }

    #[test]
    fn test_scrollback_with_custom_cell_type() {
        // Create a grid with custom cell type
        let mut grid = create_test_grid(80, 24, 100, 'a');

        // Add enough lines to trigger scrollback
        for i in 0..150 {
            let ch = (b'a' + (i % 26) as u8) as char;
            grid.insert_line(Line(0));

            let cells = grid.cursor.cells_mut();
            for cell in cells {
                *cell = TestCell::new(ch);
            }
        }

        // Verify scrollback size limit is respected
        assert_eq!(grid.history_size(), 100);

        // Test scrolling
        grid.scroll_display(Line(10));
        assert_eq!(grid.display_offset(), 10);

        // Reset scrolling
        grid.scroll_display(Line(0));
        assert_eq!(grid.display_offset(), 0);
    }

    #[test]
    fn test_swap_rows_with_custom_cell_type() {
        // Create a grid with custom cell type
        let mut grid = create_test_grid(10, 10, 20, 'a');

        // Add some different rows
        grid.insert_line(Line(0));
        let cells = grid.cursor.cells_mut();
        for cell in cells {
            *cell = TestCell::new('b');
        }

        grid.insert_line(Line(0));
        let cells = grid.cursor.cells_mut();
        for cell in cells {
            *cell = TestCell::new('c');
        }

        // Swap two rows
        let first_line = grid.line_of_index(grid.history_size() + 1);
        let second_line = grid.line_of_index(grid.history_size() + 2);
        grid.raw.swap(first_line, second_line);

        // Verify rows were swapped correctly
        let cells_first = grid.raw[first_line].cells;
        let cells_second = grid.raw[second_line].cells;

        assert_eq!(cells_first[0].value, 'b');
        assert_eq!(cells_second[0].value, 'c');
    }
}
