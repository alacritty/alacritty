This crate only exists to allow us to force `servo-fontconfig-sys` to
use `freetype-sys` instead of `servo-freetype-sys`, which is in turn
needed so that we don't try to link with `freetype` multiple times from
different crates.
