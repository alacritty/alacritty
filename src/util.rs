// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
#[cfg(not(windows))]
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::{cmp, io};

/// Threading utilities
pub mod thread {
    /// Like `thread::spawn`, but with a `name` argument
    pub fn spawn_named<F, T, S>(name: S, f: F) -> ::std::thread::JoinHandle<T>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
        S: Into<String>,
    {
        ::std::thread::Builder::new()
            .name(name.into())
            .spawn(f)
            .expect("thread spawn works")
    }

    pub use std::thread::*;
}

pub fn limit<T: Ord>(value: T, min: T, max: T) -> T {
    cmp::min(cmp::max(value, min), max)
}

/// Utilities for writing to the
pub mod fmt {
    use std::fmt;

    macro_rules! define_colors {
        ($($(#[$attrs:meta])* pub struct $s:ident => $color:expr;)*) => {
            $(
                $(#[$attrs])*
                pub struct $s<T>(pub T);

                impl<T: fmt::Display> fmt::Display for $s<T> {
                    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        write!(f, concat!("\x1b[", $color, "m{}\x1b[0m"), self.0)
                    }
                }

                impl<T: fmt::Debug> fmt::Debug for $s<T> {
                    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        write!(f, concat!("\x1b[", $color, "m{:?}\x1b[0m"), self.0)
                    }
                }
            )*
        }
    }

    define_colors! {
        /// Write a `Display` or `Debug` escaped with Red
        pub struct Red => "31";

        /// Write a `Display` or `Debug` escaped with Green
        pub struct Green => "32";

        /// Write a `Display` or `Debug` escaped with Yellow
        pub struct Yellow => "33";
    }
}

#[cfg(not(windows))]
pub fn start_daemon(program: &str, args: &[String]) -> io::Result<()> {
    Command::new(program)
        .args(args)
        .before_exec(|| unsafe {
            ::libc::daemon(1, 0);
            Ok(())
        })
        .spawn()?
        .wait()
        .map(|_| ())
}

#[cfg(windows)]
pub fn start_daemon(program: &str, args: &[String]) -> io::Result<()> {
    Command::new(program).args(args).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::limit;

    #[test]
    fn limit_works() {
        assert_eq!(10, limit(10, 0, 100));
        assert_eq!(10, limit(5, 10, 100));
        assert_eq!(100, limit(1000, 10, 100));
    }
}
