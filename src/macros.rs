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

#[macro_export]
macro_rules! die {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        ::std::process::exit(1);
    }}
}

#[macro_export]
macro_rules! maybe {
    ($option:expr) => {
        match $option {
            Some(value) => value,
            None => return None,
        }
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let _ = write!(::std::io::stdout(), $($arg)*);
    }};
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let _ = write!(::std::io::stderr(), $($arg)*);
    }};
}
