//! Tests for the Grid.

use super::*;

use crate::term::cell::Cell;

impl GridCell for usize {
    fn is_empty(&self) -> bool {
        *self == 0
    }

    fn reset(&mut self, template: &Self) {
        *self = *template;
    }

    fn flags(&self) -> &Flags {
        unimplemented!();
    }

    fn flags_mut(&mut self) -> &mut Flags {
        unimplemented!();
    }
}

#[test]
fn grid_clamp_buffer_point() {
    let mut grid = Grid::<usize>::new(Line(10), Column(10), 1_000);
    grid.display_offset = 5;

    let point = grid.clamp_buffer_to_visible(Point::new(10, Column(3)));
    assert_eq!(point, Point::new(Line(4), Column(3)));

    let point = grid.clamp_buffer_to_visible(Point::new(15, Column(3)));
    assert_eq!(point, Point::new(Line(0), Column(0)));

    let point = grid.clamp_buffer_to_visible(Point::new(4, Column(3)));
    assert_eq!(point, Point::new(Line(9), Column(9)));

    grid.display_offset = 0;

    let point = grid.clamp_buffer_to_visible(Point::new(4, Column(3)));
    assert_eq!(point, Point::new(Line(5), Column(3)));
}

#[test]
fn visible_to_buffer() {
    let mut grid = Grid::<usize>::new(Line(10), Column(10), 1_000);
    grid.display_offset = 5;

    let point = grid.visible_to_buffer(Point::new(Line(4), Column(3)));
    assert_eq!(point, Point::new(10, Column(3)));

    grid.display_offset = 0;

    let point = grid.visible_to_buffer(Point::new(Line(5), Column(3)));
    assert_eq!(point, Point::new(4, Column(3)));
}

// Scroll up moves lines upward.
#[test]
fn scroll_up() {
    let mut grid = Grid::<usize>::new(Line(10), Column(1), 0);
    for i in 0..10 {
        grid[Line(i)][Column(0)] = i;
    }

    grid.scroll_up::<usize>(&(Line(0)..Line(10)), Line(2));

    assert_eq!(grid[Line(0)][Column(0)], 2);
    assert_eq!(grid[Line(0)].occ, 1);
    assert_eq!(grid[Line(1)][Column(0)], 3);
    assert_eq!(grid[Line(1)].occ, 1);
    assert_eq!(grid[Line(2)][Column(0)], 4);
    assert_eq!(grid[Line(2)].occ, 1);
    assert_eq!(grid[Line(3)][Column(0)], 5);
    assert_eq!(grid[Line(3)].occ, 1);
    assert_eq!(grid[Line(4)][Column(0)], 6);
    assert_eq!(grid[Line(4)].occ, 1);
    assert_eq!(grid[Line(5)][Column(0)], 7);
    assert_eq!(grid[Line(5)].occ, 1);
    assert_eq!(grid[Line(6)][Column(0)], 8);
    assert_eq!(grid[Line(6)].occ, 1);
    assert_eq!(grid[Line(7)][Column(0)], 9);
    assert_eq!(grid[Line(7)].occ, 1);
    assert_eq!(grid[Line(8)][Column(0)], 0); // was 0.
    assert_eq!(grid[Line(8)].occ, 0);
    assert_eq!(grid[Line(9)][Column(0)], 0); // was 1.
    assert_eq!(grid[Line(9)].occ, 0);
}

// Scroll down moves lines downward.
#[test]
fn scroll_down() {
    let mut grid = Grid::<usize>::new(Line(10), Column(1), 0);
    for i in 0..10 {
        grid[Line(i)][Column(0)] = i;
    }

    grid.scroll_down::<usize>(&(Line(0)..Line(10)), Line(2));

    assert_eq!(grid[Line(0)][Column(0)], 0); // was 8.
    assert_eq!(grid[Line(0)].occ, 0);
    assert_eq!(grid[Line(1)][Column(0)], 0); // was 9.
    assert_eq!(grid[Line(1)].occ, 0);
    assert_eq!(grid[Line(2)][Column(0)], 0);
    assert_eq!(grid[Line(2)].occ, 1);
    assert_eq!(grid[Line(3)][Column(0)], 1);
    assert_eq!(grid[Line(3)].occ, 1);
    assert_eq!(grid[Line(4)][Column(0)], 2);
    assert_eq!(grid[Line(4)].occ, 1);
    assert_eq!(grid[Line(5)][Column(0)], 3);
    assert_eq!(grid[Line(5)].occ, 1);
    assert_eq!(grid[Line(6)][Column(0)], 4);
    assert_eq!(grid[Line(6)].occ, 1);
    assert_eq!(grid[Line(7)][Column(0)], 5);
    assert_eq!(grid[Line(7)].occ, 1);
    assert_eq!(grid[Line(8)][Column(0)], 6);
    assert_eq!(grid[Line(8)].occ, 1);
    assert_eq!(grid[Line(9)][Column(0)], 7);
    assert_eq!(grid[Line(9)].occ, 1);
}

// Test that GridIterator works.
#[test]
fn test_iter() {
    let mut grid = Grid::<usize>::new(Line(5), Column(5), 0);
    for i in 0..5 {
        for j in 0..5 {
            grid[Line(i)][Column(j)] = i * 5 + j;
        }
    }

    let mut iter = grid.iter_from(Point { line: 4, col: Column(0) });

    assert_eq!(None, iter.prev());
    assert_eq!(Some(&1), iter.next());
    assert_eq!(Column(1), iter.point().col);
    assert_eq!(4, iter.point().line);

    assert_eq!(Some(&2), iter.next());
    assert_eq!(Some(&3), iter.next());
    assert_eq!(Some(&4), iter.next());

    // Test line-wrapping.
    assert_eq!(Some(&5), iter.next());
    assert_eq!(Column(0), iter.point().col);
    assert_eq!(3, iter.point().line);

    assert_eq!(Some(&4), iter.prev());
    assert_eq!(Column(4), iter.point().col);
    assert_eq!(4, iter.point().line);

    // Make sure iter.cell() returns the current iterator position.
    assert_eq!(&4, iter.cell());

    // Test that iter ends at end of grid.
    let mut final_iter = grid.iter_from(Point { line: 0, col: Column(4) });
    assert_eq!(None, final_iter.next());
    assert_eq!(Some(&23), final_iter.prev());
}

#[test]
fn shrink_reflow() {
    let mut grid = Grid::<Cell>::new(Line(1), Column(5), 2);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = cell('2');
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = cell('5');

    grid.resize(true, Line(1), Column(2));

    assert_eq!(grid.total_lines(), 3);

    assert_eq!(grid[2].len(), 2);
    assert_eq!(grid[2][Column(0)], cell('1'));
    assert_eq!(grid[2][Column(1)], wrap_cell('2'));

    assert_eq!(grid[1].len(), 2);
    assert_eq!(grid[1][Column(0)], cell('3'));
    assert_eq!(grid[1][Column(1)], wrap_cell('4'));

    assert_eq!(grid[0].len(), 2);
    assert_eq!(grid[0][Column(0)], cell('5'));
    assert_eq!(grid[0][Column(1)], Cell::default());
}

#[test]
fn shrink_reflow_twice() {
    let mut grid = Grid::<Cell>::new(Line(1), Column(5), 2);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = cell('2');
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = cell('5');

    grid.resize(true, Line(1), Column(4));
    grid.resize(true, Line(1), Column(2));

    assert_eq!(grid.total_lines(), 3);

    assert_eq!(grid[2].len(), 2);
    assert_eq!(grid[2][Column(0)], cell('1'));
    assert_eq!(grid[2][Column(1)], wrap_cell('2'));

    assert_eq!(grid[1].len(), 2);
    assert_eq!(grid[1][Column(0)], cell('3'));
    assert_eq!(grid[1][Column(1)], wrap_cell('4'));

    assert_eq!(grid[0].len(), 2);
    assert_eq!(grid[0][Column(0)], cell('5'));
    assert_eq!(grid[0][Column(1)], Cell::default());
}

#[test]
fn shrink_reflow_empty_cell_inside_line() {
    let mut grid = Grid::<Cell>::new(Line(1), Column(5), 3);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = Cell::default();
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = Cell::default();

    grid.resize(true, Line(1), Column(2));

    assert_eq!(grid.total_lines(), 2);

    assert_eq!(grid[1].len(), 2);
    assert_eq!(grid[1][Column(0)], cell('1'));
    assert_eq!(grid[1][Column(1)], wrap_cell(' '));

    assert_eq!(grid[0].len(), 2);
    assert_eq!(grid[0][Column(0)], cell('3'));
    assert_eq!(grid[0][Column(1)], cell('4'));

    grid.resize(true, Line(1), Column(1));

    assert_eq!(grid.total_lines(), 4);

    assert_eq!(grid[3].len(), 1);
    assert_eq!(grid[3][Column(0)], wrap_cell('1'));

    assert_eq!(grid[2].len(), 1);
    assert_eq!(grid[2][Column(0)], wrap_cell(' '));

    assert_eq!(grid[1].len(), 1);
    assert_eq!(grid[1][Column(0)], wrap_cell('3'));

    assert_eq!(grid[0].len(), 1);
    assert_eq!(grid[0][Column(0)], cell('4'));
}

#[test]
fn grow_reflow() {
    let mut grid = Grid::<Cell>::new(Line(2), Column(2), 0);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = wrap_cell('2');
    grid[Line(1)][Column(0)] = cell('3');
    grid[Line(1)][Column(1)] = Cell::default();

    grid.resize(true, Line(2), Column(3));

    assert_eq!(grid.total_lines(), 2);

    assert_eq!(grid[1].len(), 3);
    assert_eq!(grid[1][Column(0)], cell('1'));
    assert_eq!(grid[1][Column(1)], cell('2'));
    assert_eq!(grid[1][Column(2)], cell('3'));

    // Make sure rest of grid is empty.
    assert_eq!(grid[0].len(), 3);
    assert_eq!(grid[0][Column(0)], Cell::default());
    assert_eq!(grid[0][Column(1)], Cell::default());
    assert_eq!(grid[0][Column(2)], Cell::default());
}

#[test]
fn grow_reflow_multiline() {
    let mut grid = Grid::<Cell>::new(Line(3), Column(2), 0);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = wrap_cell('2');
    grid[Line(1)][Column(0)] = cell('3');
    grid[Line(1)][Column(1)] = wrap_cell('4');
    grid[Line(2)][Column(0)] = cell('5');
    grid[Line(2)][Column(1)] = cell('6');

    grid.resize(true, Line(3), Column(6));

    assert_eq!(grid.total_lines(), 3);

    assert_eq!(grid[2].len(), 6);
    assert_eq!(grid[2][Column(0)], cell('1'));
    assert_eq!(grid[2][Column(1)], cell('2'));
    assert_eq!(grid[2][Column(2)], cell('3'));
    assert_eq!(grid[2][Column(3)], cell('4'));
    assert_eq!(grid[2][Column(4)], cell('5'));
    assert_eq!(grid[2][Column(5)], cell('6'));

    // Make sure rest of grid is empty.
    for r in 0..2 {
        assert_eq!(grid[r].len(), 6);
        for c in 0..6 {
            assert_eq!(grid[r][Column(c)], Cell::default());
        }
    }
}

#[test]
fn grow_reflow_disabled() {
    let mut grid = Grid::<Cell>::new(Line(2), Column(2), 0);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = wrap_cell('2');
    grid[Line(1)][Column(0)] = cell('3');
    grid[Line(1)][Column(1)] = Cell::default();

    grid.resize(false, Line(2), Column(3));

    assert_eq!(grid.total_lines(), 2);

    assert_eq!(grid[1].len(), 3);
    assert_eq!(grid[1][Column(0)], cell('1'));
    assert_eq!(grid[1][Column(1)], wrap_cell('2'));
    assert_eq!(grid[1][Column(2)], Cell::default());

    assert_eq!(grid[0].len(), 3);
    assert_eq!(grid[0][Column(0)], cell('3'));
    assert_eq!(grid[0][Column(1)], Cell::default());
    assert_eq!(grid[0][Column(2)], Cell::default());
}

#[test]
fn shrink_reflow_disabled() {
    let mut grid = Grid::<Cell>::new(Line(1), Column(5), 2);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = cell('2');
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = cell('5');

    grid.resize(false, Line(1), Column(2));

    assert_eq!(grid.total_lines(), 1);

    assert_eq!(grid[0].len(), 2);
    assert_eq!(grid[0][Column(0)], cell('1'));
    assert_eq!(grid[0][Column(1)], cell('2'));
}

// https://github.com/rust-lang/rust-clippy/pull/6375
#[allow(clippy::all)]
fn cell(c: char) -> Cell {
    let mut cell = Cell::default();
    cell.c = c;
    cell
}

fn wrap_cell(c: char) -> Cell {
    let mut cell = cell(c);
    cell.flags.insert(Flags::WRAPLINE);
    cell
}
