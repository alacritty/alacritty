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
        err_println!($($arg)*);
        ::std::process::exit(1);
    }}
}

#[macro_export]
macro_rules! err_println {
    ($($arg:tt)*) => {{
        if cfg!(feature = "err-println") {
            use std::io::Write;
            (writeln!(&mut ::std::io::stderr(), $($arg)*)).expect("stderr");
        }
    }}
}

#[macro_export]
macro_rules! debug_println {
    ($($t:tt)*) => {
        if cfg!(debug_assertions) {
            println!($($t)*);
        }
    }
}

#[macro_export]
macro_rules! debug_print {
    ($($t:tt)*) => {
        if cfg!(debug_assertions) {
            print!($($t)*);
        }
    }
}

