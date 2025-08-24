use super::{Grid};
use crate::grid::storage::Swappable;

#[derive(Clone, PartialEq, Debug)]
struct DummyRow(u8);

impl Swappable for DummyRow {}

#[test]
fn test_grid_scrollback() {
    let mut grid: Grid<DummyRow> = Grid::new(80, 24, 1000);

    for i in 0..1500 {
        grid.push_row(DummyRow((i % 255) as u8));
    }

    assert_eq!(grid.storage.len(), 1000);

    grid.swap_rows(0, 999);

    assert_eq!(grid.get_row(0).unwrap(), &DummyRow((1499 % 255) as u8));
    assert_eq!(grid.get_row(999).unwrap(), &DummyRow((500 % 255) as u8));
}

