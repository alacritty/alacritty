// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Unmerged utf8 chars iterator vendored from std::io
//!
use std::io::{BufRead, ErrorKind, Error};
use std::fmt;
use std::error as std_error;
use std::result;
use std::char;

static UTF8_CHAR_WIDTH: [u8; 256] = [
1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, // 0x1F
1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, // 0x3F
1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, // 0x5F
1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, // 0x7F
0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, // 0x9F
0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, // 0xBF
0,0,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2, // 0xDF
3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3, // 0xEF
4,4,4,4,4,0,0,0,0,0,0,0,0,0,0,0, // 0xFF
];

/// Given a first byte, determine how many bytes are in this UTF-8 character
#[inline]
pub fn utf8_char_width(b: u8) -> usize {
    return UTF8_CHAR_WIDTH[b as usize] as usize;
}

/// An iterator over the `char`s of a reader.
///
/// This struct is generally created by calling [`utf8_chars()`][utf8_chars] on a reader.
/// Please see the documentation of `utf8_chars()` for more details.
///
/// [utf8_chars]: trait.BufRead.html#method.utf8_chars
pub struct Utf8Chars<R> {
    inner: R,
}

impl<R> Utf8Chars<R> {
    pub fn new(inner: R) -> Utf8Chars<R> {
        Utf8Chars { inner: inner }
    }
}

/// An enumeration of possible errors that can be generated from the `Utf8Chars`
/// adapter.
#[derive(Debug)]
pub enum Utf8CharsError {
    /// Variant representing that the underlying stream was read successfully
    /// but contains a byte sequence ill-formed in UTF-8.
    InvalidUtf8,

    /// Variant representing that the underlying stream contains the start
    /// of a byte sequence well-formed in UTF-8, but ends prematurely.
    ///
    /// Contains number of unused bytes
    IncompleteUtf8(u8),

    /// Variant representing that an I/O error occurred.
    Io(Error),
}

impl<R: BufRead> Iterator for Utf8Chars<R> {
    type Item = result::Result<char, Utf8CharsError>;

    // allow(unused_assignments) because consumed += 1 is not recognized as being used
    #[allow(unused_assignments)]
    fn next(&mut self) -> Option<result::Result<char, Utf8CharsError>> {
        macro_rules! read_byte {
            (EOF => $on_eof: expr) => {
                {
                    let byte;
                    loop {
                        match self.inner.fill_buf() {
                            Ok(buffer) => {
                                if let Some(&b) = buffer.first() {
                                    byte = b;
                                    break
                                } else {
                                    $on_eof
                                }
                            }
                            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                            Err(e) => return Some(Err(Utf8CharsError::Io(e))),
                        }
                    }
                    byte
                }
            }
        }

        let first = read_byte!(EOF => return None);
        self.inner.consume(1);

        let mut consumed = 1;

        macro_rules! continuation_byte {
            ($range: pat) => {
                {
                    match read_byte!(EOF => return Some(Err(Utf8CharsError::IncompleteUtf8(consumed)))) {
                        byte @ $range => {
                            self.inner.consume(1);
                            consumed += 1;
                            (byte & 0b0011_1111) as u32
                        }
                        _ => return Some(Err(Utf8CharsError::InvalidUtf8))
                    }
                }
            }
        }

        // Ranges can be checked against https://tools.ietf.org/html/rfc3629#section-4
        let code_point = match utf8_char_width(first) {
            1 => return Some(Ok(first as char)),
            2 => {
                let second = continuation_byte!(0x80...0xBF);
                ((first & 0b0001_1111) as u32) << 6 | second
            }
            3 => {
                let second = match first {
                    0xE0        => continuation_byte!(0xA0...0xBF),
                    0xE1...0xEC => continuation_byte!(0x80...0xBF),
                    0xED        => continuation_byte!(0x80...0x9F),
                    0xEE...0xEF => continuation_byte!(0x80...0xBF),
                    _ => unreachable!(),
                };
                let third = continuation_byte!(0x80...0xBF);
                ((first & 0b0000_1111) as u32) << 12 | second << 6 | third
            }
            4 => {
                let second = match first {
                    0xF0        => continuation_byte!(0x90...0xBF),
                    0xF0...0xF3 => continuation_byte!(0x80...0xBF),
                    0xF4        => continuation_byte!(0x80...0x8F),
                    _ => unreachable!(),
                };
                let third = continuation_byte!(0x80...0xBF);
                let fourth = continuation_byte!(0x80...0xBF);
                ((first & 0b0000_0111) as u32) << 18 | second << 12 | third << 6 | fourth
            }
            _ => return Some(Err(Utf8CharsError::InvalidUtf8))
        };
        unsafe {
            Some(Ok(char::from_u32_unchecked(code_point)))
        }
    }
}

impl std_error::Error for Utf8CharsError {
    fn description(&self) -> &str {
        match *self {
            Utf8CharsError::InvalidUtf8 => "invalid UTF-8 byte sequence",
            Utf8CharsError::IncompleteUtf8(_) => {
                "stream ended in the middle of an UTF-8 byte sequence"
            }
            Utf8CharsError::Io(ref e) => std_error::Error::description(e),
        }
    }
    fn cause(&self) -> Option<&std_error::Error> {
        match *self {
            Utf8CharsError::InvalidUtf8 | Utf8CharsError::IncompleteUtf8(_) => None,
            Utf8CharsError::Io(ref e) => e.cause(),
        }
    }
}

impl fmt::Display for Utf8CharsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Utf8CharsError::InvalidUtf8 => {
                "invalid UTF-8 byte sequence".fmt(f)
            }
            Utf8CharsError::IncompleteUtf8(_) => {
                "stream ended in the middle of an UTF-8 byte sequence".fmt(f)
            }
            Utf8CharsError::Io(ref e) => e.fmt(f),
        }
    }
}
