use super::{Grid};
use crate::grid::storage::Swappable;

#[derive(Clone, PartialEq, Debug)]
struct DummyRow(Vec<u8>);

impl Swappable for DummyRow {}

#[test]
fn test_grid_push_and_swap() {
    let mut grid: Grid<DummyRow> = Grid::new(10, 5, 3);
    let row1 = DummyRow(vec![1]);
    let row2 = DummyRow(vec![2]);

    grid.push_row(row1.clone());
    grid.push_row(row2.clone());

    grid.swap_rows(0, 1);

    assert_eq!(grid.get_row(0).unwrap(), &row2);
    assert_eq!(grid.get_row(1).unwrap(), &row1);
}

#[test]
fn test_grid_max_size() {
    let mut grid: Grid<DummyRow> = Grid::new(10, 5, 2);
    let row1 = DummyRow(vec![1]);
    let row2 = DummyRow(vec![2]);
    let row3 = DummyRow(vec![3]);

    grid.push_row(row1);
    grid.push_row(row2.clone());
    grid.push_row(row3.clone());

    assert_eq!(grid.get_row(0).unwrap(), &row2);
    assert_eq!(grid.get_row(1).unwrap(), &row3);
}

