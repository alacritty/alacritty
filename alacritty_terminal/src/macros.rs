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
        error!($($arg)*);
        ::std::process::exit(1);
    }}
}

macro_rules! combination_err_base {
    ($ename:ident, { $( $variant:ident: $inner:ty ),+ }) => {
        #[derive(Debug)]
        pub enum $ename {
            $(
                $variant($inner)
            ),*
        }

        impl $ename {
            #[inline(always)]
            fn inner_error(&self) -> &dyn ::std::error::Error {
                match *self {
                    $(
                        $ename::$variant(ref err) => err,
                    )*
                }
            }
        }

        $(
            impl From<$inner> for $ename {
                fn from(val: $inner) -> Self {
                    $ename::$variant(val)
                }
            }
        )*
    }
}

macro_rules! combination_err {
    ($ename:ident, { $( $variant:ident : $inner:ty ),+ }) => {
        combination_err_base!($ename, { $( $variant : $inner ),* });

        impl ::std::error::Error for $ename {
            fn cause(&self) -> Option<&dyn (::std::error::Error)> {
                Some(self.inner_error())
            }

            fn description(&self) -> &str {
                self.source().unwrap().description()
            }
        }

        impl ::std::fmt::Display for $ename {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                std::fmt::Display::fmt(self.inner_error(), f)
            }
        }
    };
    ($ename:ident, { $( $variant:ident : $inner:ty : $description:expr ),+ }) => {
        combination_err_base!($ename, { $( $variant : $inner ),* });

        impl ::std::error::Error for $ename {
            fn cause(&self) -> Option<&dyn (::std::error::Error)> {
                Some(self.inner_error())
            }

            fn description(&self) -> &str {
                match *self {
                    $(
                        $ename::$variant(..) => $description,
                    )*
                }
            }
        }

        impl ::std::fmt::Display for $ename {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                let description = {
                    use std::error::Error;
                    self.description()
                };
                match *self {
                    $(
                        $ename::$variant(ref err) => write!(f, "{}: {}", description, err),
                    )*
                }
            }
        }
    };
}
