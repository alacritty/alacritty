/// Trait for types that can be swapped.
pub trait Swappable {
    fn swap(&mut self, other: &mut Self);
}

// Blanket impl for all types using std::mem::swap.
impl<T> Swappable for T {
    fn swap(&mut self, other: &mut Self) {
        std::mem::swap(self, other);
    }
}

/// Storage for grid rows, fully generic over T.
pub struct Storage<T> {
    inner: Vec<T>,
    max_size: usize,
}

impl<T> Storage<T> {
    pub fn new(max_size: usize) -> Storage<T> {
        Storage {
            inner: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.inner.len() >= self.max_size {
            self.inner.remove(0);
        }
        self.inner.push(item);
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.inner.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.inner.get_mut(index)
    }

    /// Swap two elements using the Swappable trait.
    pub fn swap(&mut self, i: usize, j: usize)
    where
        T: Swappable,
    {
        if i < self.inner.len() && j < self.inner.len() && i != j {
            self.inner[i].swap(&mut self.inner[j]);
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}
