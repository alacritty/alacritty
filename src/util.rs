/// Threading utilities
pub mod thread {
    /// Like `thread::spawn`, but with a `name` argument
    pub fn spawn_named<F, T, S>(name: S, f: F) -> ::std::thread::JoinHandle<T>
        where F: FnOnce() -> T,
              F: Send + 'static,
              T: Send + 'static,
              S: Into<String>
    {
        ::std::thread::Builder::new().name(name.into()).spawn(f).expect("thread spawn works")
    }
}

/// Types that can have their elements rotated
pub trait Rotate {
    fn rotate(&mut self, positions: isize);
}

impl<T> Rotate for [T] {
    fn rotate(&mut self, positions: isize) {
        // length is needed over and over
        let len = self.len();

        // Enforce positions in [0, len) and treat negative rotations as a
        // posititive rotation of len - positions.
        let positions = if positions > 0 {
            positions as usize % len
        } else {
            len - (-positions as usize) % len
        };

        // If positions is 0 or the entire slice, it's a noop.
        if positions == 0 || positions == len {
            return;
        }

        self[..positions].reverse();
        self[positions..].reverse();
        self.reverse();
    }
}


#[cfg(test)]
mod tests {
    use super::Rotate;

    #[test]
    fn rotate_forwards_works() {
        let s = &mut [1, 2, 3, 4, 5];
        s.rotate(1);
        assert_eq!(&[2, 3, 4, 5, 1], s);
    }

    #[test]
    fn rotate_backwards_works() {
        let s = &mut [1, 2, 3, 4, 5];
        s.rotate(-1);
        assert_eq!(&[5, 1, 2, 3, 4], s);
    }

    #[test]
    fn rotate_multiple_forwards() {
        let s = &mut [1, 2, 3, 4, 5, 6, 7];
        s.rotate(2);
        assert_eq!(&[3, 4, 5, 6, 7, 1, 2], s);
    }

    #[test]
    fn rotate_multiple_backwards() {
        let s = &mut [1, 2, 3, 4, 5];
        s.rotate(-3);
        assert_eq!(&[3, 4, 5, 1, 2], s);
    }

    #[test]
    fn rotate_forwards_overflow() {
        let s = &mut [1, 2, 3, 4, 5];
        s.rotate(6);
        assert_eq!(&[2, 3, 4, 5, 1], s);
    }

    #[test]
    fn rotate_backwards_overflow() {
        let s = &mut [1, 2, 3, 4, 5];
        s.rotate(-6);
        assert_eq!(&[5, 1, 2, 3, 4], s);
    }
}
