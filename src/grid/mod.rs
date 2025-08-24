// ...existing code...
use crate::grid::storage::{Storage, Swappable};
// ...existing code...

pub struct Grid<T> {
    storage: Storage<T>,
    width: usize,
    height: usize,
}

impl<T> Grid<T> {
    pub fn new(width: usize, height: usize, max_scrollback: usize) -> Grid<T> {
        Grid {
            storage: Storage::new(max_scrollback),
            width,
            height,
        }
    }

    pub fn push_row(&mut self, row: T) {
        self.storage.push(row);
    }

    pub fn swap_rows(&mut self, i: usize, j: usize)
    where
        T: Swappable,
    {
        self.storage.swap(i, j);
    }

    pub fn get_row(&self, index: usize) -> Option<&T> {
        self.storage.get(index)
    }

    pub fn get_row_mut(&mut self, index: usize) -> Option<&mut T> {
        self.storage.get_mut(index)
    }

    pub fn clear(&mut self) {
        self.storage.clear();
    }
}

// ...existing code...

