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
//
//! ANSI Terminal Stream Parsing
//!
//! The `Parser` implementation is largely based on the suck-less _simple terminal_ parser. Because
//! this is Rust and Rust has a fantastic type system, some improvements are possible. First,
//! `Parser` is a struct, and its data is stored internally instead of statically. Second, there's
//! no terminal updates hard-coded into the parser. Instead, `Parser` is generic over a `Handler`
//! type which has methods for all of the actions supported by the parser. Because Parser is
//! generic, it should be possible (with proper inlining) to have equivalent performance to the
//! hard-coded version.
//!
//! In addition to using _simple terminal_ as a reference, there's a doc in Alacritty's repository
//! `docs/ansicode.txt`, a summary of the ANSI terminal protocol, which has been referenced
//! extensively.
//!
//! There's probably a large number escapes we don't handle, and that's ok. There's a lot that
//! aren't necessary for everyday terminal usage. If you feel like something that's not supported
//! should be, feel free to add it. Please try not to become overzealous and adding support for
//! sequences only used by folks trapped in 1988.
use index::{Column, Line};

use ::Rgb;

/// A CSI Escape sequence
#[derive(Debug, Eq, PartialEq)]
pub enum Escape {
    DisplayAttr(u8),
}

/// Trait that provides properties of terminal
pub trait TermInfo {
    fn lines(&self) -> Line;
    fn cols(&self) -> Column;
}

pub const CSI_ATTR_MAX: usize = 16;

pub struct Parser {
    /// Workspace for building a control sequence
    buf: [char; 1024],

    /// Index of control sequence
    ///
    /// Alternatively, this can be viewed at the current length of used buffer
    idx: usize,

    /// Current state
    state: State,
}

/// Terminal modes
#[derive(Debug, Eq, PartialEq)]
pub enum Mode {
    /// ?1
    CursorKeys = 1,
    /// ?25
    ShowCursor = 25,
    /// ?12
    BlinkingCursor = 12,
    /// ?1049
    SwapScreenAndSetRestoreCursor = 1049,
}

impl Mode {
    /// Create mode from a primitive
    ///
    /// TODO lots of unhandled values..
    pub fn from_primitive(private: bool, num: i64) -> Option<Mode> {
        if private {
            Some(match num {
                1 => Mode::CursorKeys,
                12 => Mode::BlinkingCursor,
                25 => Mode::ShowCursor,
                1049 => Mode::SwapScreenAndSetRestoreCursor,
                _ => return None
            })
        } else {
            // TODO
            None
        }
    }
}

/// Mode for clearing line
///
/// Relative to cursor
#[derive(Debug)]
pub enum LineClearMode {
    /// Clear right of cursor
    Right,
    /// Clear left of cursor
    Left,
    /// Clear entire line
    All,
}

/// Mode for clearing terminal
///
/// Relative to cursor
#[derive(Debug)]
pub enum ClearMode {
    /// Clear below cursor
    Below,
    /// Clear above cursor
    Above,
    /// Clear entire terminal
    All,
}

/// Mode for clearing tab stops
#[derive(Debug)]
pub enum TabulationClearMode {
    /// Clear stop under cursor
    Current,
    /// Clear all stops
    All,
}

/// Standard colors
///
/// The order here matters since the enum should be castable to a `usize` for
/// indexing a color list.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Color {
    /// Black
    Black = 0,
    /// Red
    Red,
    /// Green
    Green,
    /// Yellow
    Yellow,
    /// Blue
    Blue,
    /// Magenta
    Magenta,
    /// Cyan
    Cyan,
    /// White
    White,
    /// Bright black
    BrightBlack,
    /// Bright red
    BrightRed,
    /// Bright green
    BrightGreen,
    /// Bright yellow
    BrightYellow,
    /// Bright blue
    BrightBlue,
    /// Bright magenta
    BrightMagenta,
    /// Bright cyan
    BrightCyan,
    /// Bright white
    BrightWhite,
}

/// Terminal character attributes
#[derive(Debug, Eq, PartialEq)]
pub enum Attr {
    /// Clear all special abilities
    Reset,
    /// Bold text
    Bold,
    /// Dim or secondary color
    Dim,
    /// Italic text
    Italic,
    /// Underscore text
    Underscore,
    /// Blink cursor slowly
    BlinkSlow,
    /// Blink cursor fast
    BlinkFast,
    /// Invert colors
    Reverse,
    /// Do not display characters
    Hidden,
    /// Strikethrough text
    Strike,
    /// Cancel bold and dim
    CancelBoldDim,
    /// Cancel italic
    CancelItalic,
    /// Cancel underline
    CancelUnderline,
    /// Cancel blink
    CancelBlink,
    /// Cancel inversion
    CancelReverse,
    /// Cancel text hiding
    CancelHidden,
    /// Cancel strike through
    CancelStrike,
    /// Set indexed foreground color
    Foreground(Color),
    /// Set specific foreground color
    ForegroundSpec(Rgb),
    /// Set indexed background color
    Background(Color),
    /// Set specific background color
    BackgroundSpec(Rgb),
    /// Set default foreground
    DefaultForeground,
    /// Set default background
    DefaultBackground,
}

/// Type that handles actions from the parser
///
/// XXX Should probably not provide default impls for everything, but it makes
/// writing specific handler impls for tests far easier.
pub trait Handler {
    /// A character to be displayed
    fn input(&mut self, _c: char) {}

    /// Set cursor to position
    fn goto(&mut self, _x: i64, _y: i64) {}

    /// Set cursor to specific row
    fn goto_row(&mut self, _y: i64) {}

    /// Set cursor to specific column
    fn goto_col(&mut self, _x: i64) {}

    /// Insert blank characters
    fn insert_blank(&mut self, _num: i64) {}

    /// Move cursor up `rows`
    fn move_up(&mut self, _rows: i64) {}

    /// Move cursor down `rows`
    fn move_down(&mut self, _rows: i64) {}

    /// Identify the terminal (should write back to the pty stream)
    fn identify_terminal(&mut self) {}

    /// Move cursor forward `cols`
    fn move_forward(&mut self, _cols: i64) {}

    /// Move cursor backward `cols`
    fn move_backward(&mut self, _cols: i64) {}

    /// Move cursor down `rows` and set to column 1
    fn move_down_and_cr(&mut self, _rows: i64) {}

    /// Move cursor up `rows` and set to column 1
    fn move_up_and_cr(&mut self, _rows: i64) {}

    /// Put `count` tabs
    fn put_tab(&mut self, _count: i64) {}

    /// Backspace `count` characters
    fn backspace(&mut self) {}

    /// Carriage return
    fn carriage_return(&mut self) {}

    /// Linefeed
    fn linefeed(&mut self) {}

    /// Ring the bell
    ///
    /// Hopefully this is never implemented
    fn bell(&mut self) {}

    /// Substitute char under cursor
    fn substitute(&mut self) {}

    /// Newline
    fn newline(&mut self) {}

    /// Set current position as a tabstop
    fn set_horizontal_tabstop(&mut self) {}

    /// Scroll up `rows` rows
    fn scroll_up(&mut self, _rows: i64) {}

    /// Scroll down `rows` rows
    fn scroll_down(&mut self, _rows: i64) {}

    /// Insert `count` blank lines
    fn insert_blank_lines(&mut self, _count: i64) {}

    /// Delete `count` lines
    fn delete_lines(&mut self, _count: i64) {}

    /// Erase `count` chars
    ///
    /// TODO figure out AND comment what it means to "erase" chars
    fn erase_chars(&mut self, _count: i64) {}

    /// Delete `count` chars
    ///
    /// TODO figure out AND comment what it means to "delete" chars
    fn delete_chars(&mut self, _count: i64) {}

    /// Move backward `count` tabs
    fn move_backward_tabs(&mut self, _count: i64) {}

    /// Move forward `count` tabs
    fn move_forward_tabs(&mut self, _count: i64) {}

    /// Save current cursor position
    fn save_cursor_position(&mut self) {}

    /// Restore cursor position
    fn restore_cursor_position(&mut self) {}

    /// Clear current line
    fn clear_line(&mut self, _mode: LineClearMode) {}

    /// Clear screen
    fn clear_screen(&mut self, _mode: ClearMode) {}

    /// Clear tab stops
    fn clear_tabs(&mut self, _mode: TabulationClearMode) {}

    /// Reset terminal state
    fn reset_state(&mut self) {}

    /// Reverse Index
    ///
    /// Move the active position to the same horizontal position on the preceding line. If the
    /// active position is at the top margin, a scroll down is performed
    fn reverse_index(&mut self) {}

    /// set a terminal attribute
    fn terminal_attribute(&mut self, _attr: Attr) {}

    /// Set mode
    fn set_mode(&mut self, _mode: Mode) {}

    /// Unset mode
    fn unset_mode(&mut self, Mode) {}

    /// DECSTBM - Set the terminal scrolling region
    fn set_scrolling_region(&mut self, _top: i64, _bot: i64) {}
}

impl Parser {
    pub fn new() -> Parser {
        Parser {
            buf: [0 as char; 1024],
            idx: 0,
            state: Default::default(),
        }
    }

    /// Advance the state machine.
    ///
    /// Maybe returns an Item which represents a state change of the terminal
    pub fn advance<H>(&mut self, handler: &mut H, c: char)
        where H: Handler + TermInfo
    {
        // println!("state: {:?}; char: {:?}", self.state, c);
        // Control characters get handled immediately
        if is_control(c) {
            self.control(handler, c);
            return;
        }

        match self.state {
            State::Base => {
                self.advance_base(handler, c);
            },
            State::Escape => {
                self.escape(handler, c);
            },
            State::Csi => {
                self.csi(handler, c);
            },
            State::EscapeOther => {
                self.other(handler, c);
            }
        }
    }

    fn advance_base<H>(&mut self, handler: &mut H, c: char)
        where H: Handler + TermInfo
    {
        handler.input(c);
    }

    fn other<H>(&mut self, _handler: &mut H, c: char)
        where H: Handler + TermInfo
    {
        if c == 0x07 as char || c == 0x18 as char || c == 0x1a as char ||
            c == 0x1b as char || is_control_c1(c)
        {
            self.state = State::Base;
        }

        // TODO actually use these bytes. For now, we just throw them away.
    }

    /// Handle character following an ESC
    ///
    /// TODO Handle `ST`, `'#'`, `'P'`, `'_'`, `'^'`, `']'`, `'k'`,
    /// 'n', 'o', '(', ')', '*', '+', '=', '>'
    fn escape<H>(&mut self, handler: &mut H, c: char)
        where H: Handler + TermInfo
    {
        // Helper for items which complete a sequence.
        macro_rules! sequence_complete {
            ($fun:ident) => {{
                handler.$fun();
                self.state = State::Base;
            }}
        }

        match c {
            '[' => {
                self.state = State::Csi;
            },
            'D' => sequence_complete!(linefeed),
            'E' => sequence_complete!(newline),
            'H' => sequence_complete!(set_horizontal_tabstop),
            'M' => sequence_complete!(reverse_index),
            'Z' => sequence_complete!(identify_terminal),
            'c' => sequence_complete!(reset_state),
            '7' => sequence_complete!(save_cursor_position),
            '8' => sequence_complete!(restore_cursor_position),
            'P' | '_' | '^' | ']' | 'k' | '(' => {
                self.state = State::EscapeOther;
            },
            _ => {
                self.state = State::Base;
                err_println!("Unknown ESC 0x{:02x} {:?}", c as usize, c);
            }
        }
    }

    fn csi<H>(&mut self, handler: &mut H, c: char)
        where H: Handler + TermInfo
    {
        self.buf[self.idx] = c;
        self.idx += 1;

        if (self.idx == self.buf.len()) || is_csi_terminator(c) {
            self.csi_parse(handler);
        }
    }

    /// Parse current CSI escape buffer
    ///
    /// ESC '[' [[ [<priv>] <arg> [;]] <mode> [<mode>]] */
    fn csi_parse<H>(&mut self, handler: &mut H)
        where H: Handler + TermInfo
    {
        let mut args = [0i64; CSI_ATTR_MAX];
        let mut args_idx = 0;

        // Get a slice which is the used subset of self.buf
        let mut raw = &self.buf[..self.idx];
        let mut private = false;
        if raw[0] == '?' {
            private = true;
            raw = &raw[1..];
        }

        // Parse args
        while !raw.is_empty() {
            // Parse next arg in buf
            let (subslice, val) = parse_next_num(raw);
            raw = subslice;

            // Add arg to list
            args[args_idx] = val;
            args_idx += 1;

            // Max args or next char isn't arg sep
            if args_idx == CSI_ATTR_MAX || raw[0] != ';' {
                break;
            }

            // Need extra check before indexing
            if raw.is_empty() {
                break;
            }

            raw = &raw[1..];
        }

        macro_rules! unknown {
            () => {{
                err_println!("Failed parsing CSI: {:?}", &self.buf[..self.idx]);
                self.state = State::Base;
                return;
            }}
        }

        macro_rules! unhandled {
            () => {{
                err_println!("Recognized, but unhandled CSI: {:?}", &self.buf[..self.idx]);
                self.state = State::Base;
                return;
            }}
        }

        macro_rules! arg_or_default {
            ($arg:expr, $default:expr) => {
                if $arg == 0 { $default } else { $arg }
            }
        }

        macro_rules! debug_csi {
            () => {
                err_println!("CSI: {:?}", &self.buf[..self.idx]);
            }
        }

        if raw.is_empty() {
            println!("raw is empty");
            unknown!();
        }

        match raw[0] {
            '@' => handler.insert_blank(arg_or_default!(args[0], 1)),
            'A' => {
                handler.move_up(arg_or_default!(args[0], 1));
            },
            'B' | 'e' => handler.move_down(arg_or_default!(args[0], 1)),
            'c' => handler.identify_terminal(),
            'C' | 'a' => {
                handler.move_forward(arg_or_default!(args[0], 1))
            },
            'D' => handler.move_backward(arg_or_default!(args[0], 1)),
            'E' => handler.move_down_and_cr(arg_or_default!(args[0], 1)),
            'F' => handler.move_up_and_cr(arg_or_default!(args[0], 1)),
            'g' => {
                let mode = match args[0] {
                    0 => TabulationClearMode::Current,
                    3 => TabulationClearMode::All,
                    _ => unknown!(),
                };

                handler.clear_tabs(mode);
            },
            'G' | '`' => handler.goto_col(arg_or_default!(args[0], 1) - 1),
            'H' | 'f' => {
                let y = arg_or_default!(args[0], 1);
                let x = arg_or_default!(args[1], 1);
                handler.goto(x - 1, y - 1);
            },
            'I' => handler.move_forward_tabs(arg_or_default!(args[0], 1)),
            'J' => {
                let mode = match args[0] {
                    0 => ClearMode::Below,
                    1 => ClearMode::Above,
                    2 => ClearMode::All,
                    _ => unknown!(),
                };

                handler.clear_screen(mode);
            },
            'K' => {
                let mode = match args[0] {
                    0 => LineClearMode::Right,
                    1 => LineClearMode::Left,
                    2 => LineClearMode::All,
                    _ => unknown!(),
                };

                handler.clear_line(mode);
            },
            'S' => handler.scroll_up(arg_or_default!(args[0], 1)),
            'T' => handler.scroll_down(arg_or_default!(args[0], 1)),
            'L' => handler.insert_blank_lines(arg_or_default!(args[0], 1)),
            'l' => {
                let mode = Mode::from_primitive(private, args[0]);
                match mode {
                    Some(mode) => handler.unset_mode(mode),
                    None => unhandled!(),
                }
            },
            'M' => handler.delete_lines(arg_or_default!(args[0], 1)),
            'X' => handler.erase_chars(arg_or_default!(args[0], 1)),
            'P' => handler.delete_chars(arg_or_default!(args[0], 1)),
            'Z' => handler.move_backward_tabs(arg_or_default!(args[0], 1)),
            'd' => handler.goto_row(arg_or_default!(args[0], 1) - 1),
            'h' => {
                let mode = Mode::from_primitive(private, args[0]);
                match mode {
                    Some(mode) => handler.set_mode(mode),
                    None => unhandled!(),
                }
            },
            'm' => {
                let raw_attrs = &args[..args_idx];

                // Sometimes a C-style for loop is just what you need
                let mut i = 0; // C-for initializer
                loop {
                    if i >= raw_attrs.len() { // C-for condition
                        break;
                    }

                    let attr = match raw_attrs[i] {
                        0 => Attr::Reset,
                        1 => Attr::Bold,
                        2 => Attr::Dim,
                        3 => Attr::Italic,
                        4 => Attr::Underscore,
                        5 => Attr::BlinkSlow,
                        6 => Attr::BlinkFast,
                        7 => Attr::Reverse,
                        8 => Attr::Hidden,
                        9 => Attr::Strike,
                        22 => Attr::CancelBoldDim,
                        23 => Attr::CancelItalic,
                        24 => Attr::CancelUnderline,
                        25 => Attr::CancelBlink,
                        27 => Attr::CancelReverse,
                        28 => Attr::CancelHidden,
                        29 => Attr::CancelStrike,
                        30 => Attr::Foreground(Color::Black),
                        31 => Attr::Foreground(Color::Red),
                        32 => Attr::Foreground(Color::Green),
                        33 => Attr::Foreground(Color::Yellow),
                        34 => Attr::Foreground(Color::Blue),
                        35 => Attr::Foreground(Color::Magenta),
                        36 => Attr::Foreground(Color::Cyan),
                        37 => Attr::Foreground(Color::White),
                        38 => {
                            if let Some(spec) = parse_color(&raw_attrs[i..], &mut i) {
                                Attr::ForegroundSpec(spec)
                            } else {
                                break;
                            }
                        },
                        39 => Attr::DefaultForeground,
                        40 => Attr::Background(Color::Black),
                        41 => Attr::Background(Color::Red),
                        42 => Attr::Background(Color::Green),
                        43 => Attr::Background(Color::Yellow),
                        44 => Attr::Background(Color::Blue),
                        45 => Attr::Background(Color::Magenta),
                        46 => Attr::Background(Color::Cyan),
                        47 => Attr::Background(Color::White),
                        48 =>  {
                            if let Some(spec) = parse_color(&raw_attrs[i..], &mut i) {
                                Attr::BackgroundSpec(spec)
                            } else {
                                break;
                            }
                        },
                        49 => Attr::DefaultBackground,
                        90 => Attr::Foreground(Color::BrightBlack),
                        91 => Attr::Foreground(Color::BrightRed),
                        92 => Attr::Foreground(Color::BrightGreen),
                        93 => Attr::Foreground(Color::BrightYellow),
                        94 => Attr::Foreground(Color::BrightBlue),
                        95 => Attr::Foreground(Color::BrightMagenta),
                        96 => Attr::Foreground(Color::BrightCyan),
                        97 => Attr::Foreground(Color::BrightWhite),
                        100 => Attr::Foreground(Color::BrightBlack),
                        101 => Attr::Foreground(Color::BrightRed),
                        102 => Attr::Foreground(Color::BrightGreen),
                        103 => Attr::Foreground(Color::BrightYellow),
                        104 => Attr::Foreground(Color::BrightBlue),
                        105 => Attr::Foreground(Color::BrightMagenta),
                        106 => Attr::Foreground(Color::BrightCyan),
                        107 => Attr::Foreground(Color::BrightWhite),
                        _ => unknown!(),
                    };

                    handler.terminal_attribute(attr);

                    i += 1; // C-for expr
                }
            }
            'n' => handler.identify_terminal(),
            'r' => {
                if private {
                    unknown!();
                }
                let top = arg_or_default!(args[0], 1);
                let bottom = arg_or_default!(args[1], *handler.lines() as i64);

                handler.set_scrolling_region(top - 1, bottom - 1);
            },
            's' => handler.save_cursor_position(),
            'u' => handler.restore_cursor_position(),
            _ => unknown!(),
        }

        self.state = State::Base;
    }

    fn csi_reset(&mut self) {
        self.idx = 0;
    }

    fn control<H>(&mut self, handler: &mut H, c: char)
        where H: Handler + TermInfo
    {
        match c {
            C0::HT => handler.put_tab(1),
            C0::BS => handler.backspace(),
            C0::CR => handler.carriage_return(),
            C0::LF |
            C0::VT |
            C0::FF => handler.linefeed(),
            C0::BEL => handler.bell(),
            C0::ESC => {
                self.csi_reset();
                self.state = State::Escape;
                return;
            },
            // C0::S0 => Control::SwitchG1,
            // C0::S1 => Control::SwitchG0,
            C0::SUB => handler.substitute(),
            C0::CAN => {
                self.csi_reset();
                return;
            },
            C0::ENQ |
            C0::NUL |
            C0::XON |
            C0::XOFF |
            C0::DEL  => {
                // Ignored
                return;
            },
            C1::PAD | C1::HOP | C1::BPH | C1::NBH | C1::IND => {
                ()
            },
            C1::NEL => {
                handler.newline();
                ()
            },
            C1::SSA | C1::ESA => {
                ()
            },
            C1::HTS => {
                handler.set_horizontal_tabstop();
                ()
            },
            C1::HTJ | C1::VTS | C1::PLD | C1::PLU | C1::RI | C1::SS2 |
            C1::SS3 | C1::PU1 | C1::PU2 | C1::STS | C1::CCH | C1::MW |
            C1::SPA | C1::EPA | C1::SOS | C1::SGCI => {
                ()
            },
            C1::DECID => {
                handler.identify_terminal();
            },
            C1::CSI | C1::ST => {
                ()
            },
            C1::DCS | C1::OSC | C1::PM | C1::APC => {
                // FIXME complete str sequence
            },
            _ => return,
        };

        // TODO interrupt sequence on CAN, SUB, \a and C1 chars
    }
}


/// Parse a color specifier from list of attributes
fn parse_color(attrs: &[i64], i: &mut usize) -> Option<Rgb> {
    if attrs.len() < 2 {
        return None;
    }

    match attrs[*i+1] {
        2 => {
            // RGB color spec
            if attrs.len() < 5 {
                err_println!("Expected RGB color spec; got {:?}", attrs);
                return None;
            }

            let r = attrs[*i+2];
            let g = attrs[*i+3];
            let b = attrs[*i+4];

            *i = *i + 4;

            let range = 0...255;
            if !range.contains(r) || !range.contains(g) || !range.contains(b) {
                err_println!("Invalid RGB color spec: ({}, {}, {})", r, g, b);
                return None;
            }

            Some(Rgb {
                r: r as u8,
                g: g as u8,
                b: b as u8
            })
        },
        _ => {
            err_println!("Unexpected color attr: {}", attrs[*i+1]);
            None
        }
    }
}

/// Utility for parsing next number from a slice of chars
fn parse_next_num(buf: &[char]) -> (&[char], i64) {
    let mut idx = 0;
    while idx < buf.len() {
        let c = buf[idx];
        match c {
            '0'...'9' => {
                idx += 1;
                continue;
            },
            _ => break
        }
    }

    match idx {
        0 => (buf, 0),
        _ => {
            // FIXME maybe write int parser based on &[char].. just stay off the heap!
            let v = buf[..idx]
                .iter().cloned()
                .collect::<String>()
                .parse::<i64>()
                .unwrap_or(-1);
            (&buf[idx..], v)
        }
    }
}


/// Is c a CSI terminator?
#[inline]
fn is_csi_terminator(c: char) -> bool {
    match c as u32 {
        0x40...0x7e => true,
        _ => false,
    }
}

/// Is `c` a control character?
#[inline]
fn is_control(c: char) -> bool {
    is_control_c0(c) || is_control_c1(c)
}

/// Is provided char one of the C0 set of 7-bit control characters?
#[inline]
fn is_control_c0(c: char) -> bool {
    match c as u32 {
        0...0x1f | 0x7f => true,
        _ => false,
    }
}

/// Is provided char one of the C1 set of 8-bit control characters?
#[inline]
fn is_control_c1(c: char) -> bool {
    match c as u32 {
        0x80...0x9f => true,
        _ => false,
    }
}

/// C0 set of 7-bit control characters (from ANSI X3.4-1977).
#[allow(non_snake_case)]
pub mod C0 {
    /// Null filler, terminal should ignore this character
    pub const NUL: char = 0x00 as char;
    /// Start of Header
    pub const SOH: char = 0x01 as char;
    /// Start of Text, implied end of header
    pub const STX: char = 0x02 as char;
    /// End of Text, causes some terminal to respond with ACK or NAK
    pub const ETX: char = 0x03 as char;
    /// End of Transmission
    pub const EOT: char = 0x04 as char;
    /// Enquiry, causes terminal to send ANSWER-BACK ID
    pub const ENQ: char = 0x05 as char;
    /// Acknowledge, usually sent by terminal in response to ETX
    pub const ACK: char = 0x06 as char;
    /// Bell, triggers the bell, buzzer, or beeper on the terminal
    pub const BEL: char = 0x07 as char;
    /// Backspace, can be used to define overstruck characters
    pub const BS: char = 0x08 as char;
    /// Horizontal Tabulation, move to next predetermined position
    pub const HT: char = 0x09 as char;
    /// Linefeed, move to same position on next line (see also NL)
    pub const LF: char = 0x0A as char;
    /// Vertical Tabulation, move to next predetermined line
    pub const VT: char = 0x0B as char;
    /// Form Feed, move to next form or page
    pub const FF: char = 0x0C as char;
    /// Carriage Return, move to first character of current line
    pub const CR: char = 0x0D as char;
    /// Shift Out, switch to G1 (other half of character set)
    pub const SO: char = 0x0E as char;
    /// Shift In, switch to G0 (normal half of character set)
    pub const SI: char = 0x0F as char;
    /// Data Link Escape, interpret next control character specially
    pub const DLE: char = 0x10 as char;
    /// (DC1) Terminal is allowed to resume transmitting
    pub const XON: char = 0x11 as char;
    /// Device Control 2, causes ASR-33 to activate paper-tape reader
    pub const DC2: char = 0x12 as char;
    /// (DC2) Terminal must pause and refrain from transmitting
    pub const XOFF: char = 0x13 as char;
    /// Device Control 4, causes ASR-33 to deactivate paper-tape reader
    pub const DC4: char = 0x14 as char;
    /// Negative Acknowledge, used sometimes with ETX and ACK
    pub const NAK: char = 0x15 as char;
    /// Synchronous Idle, used to maintain timing in Sync communication
    pub const SYN: char = 0x16 as char;
    /// End of Transmission block
    pub const ETB: char = 0x17 as char;
    /// Cancel (makes VT100 abort current escape sequence if any)
    pub const CAN: char = 0x18 as char;
    /// End of Medium
    pub const EM: char = 0x19 as char;
    /// Substitute (VT100 uses this to display parity errors)
    pub const SUB: char = 0x1A as char;
    /// Prefix to an ESCape sequence
    pub const ESC: char = 0x1B as char;
    /// File Separator
    pub const FS: char = 0x1C as char;
    /// Group Separator
    pub const GS: char = 0x1D as char;
    /// Record Separator (sent by VT132 in block-transfer mode)
    pub const RS: char = 0x1E as char;
    /// Unit Separator
    pub const US: char = 0x1F as char;
    /// Delete, should be ignored by terminal
    pub const DEL: char = 0x7f as char;
}


/// C1 set of 8-bit control characters (from ANSI X3.64-1979)
///
/// 0x80 (@), 0x81 (A), 0x82 (B), 0x83 (C) are reserved
/// 0x98 (X), 0x99 (Y) are reserved
/// 0x9a (Z) is resezved, but causes DEC terminals to respond with DA codes
#[allow(non_snake_case)]
pub mod C1 {
    /// Reserved
    pub const PAD: char = 0x80 as char;
    /// Reserved
    pub const HOP: char = 0x81 as char;
    /// Reserved
    pub const BPH: char = 0x82 as char;
    /// Reserved
    pub const NBH: char = 0x83 as char;
    /// Index, moves down one line same column regardless of NL
    pub const IND: char = 0x84 as char;
    /// NEw Line, moves done one line and to first column (CR+LF)
    pub const NEL: char = 0x85 as char;
    /// Start of Selected Area to be  as charsent to auxiliary output device
    pub const SSA: char = 0x86 as char;
    /// End of Selected Area to be sent to auxiliary output device
    pub const ESA: char = 0x87 as char;
    /// Horizontal Tabulation Set at current position
    pub const HTS: char = 0x88 as char;
    /// Hor Tab Justify, moves string to next tab position
    pub const HTJ: char = 0x89 as char;
    /// Vertical Tabulation Set at current line
    pub const VTS: char = 0x8A as char;
    /// Partial Line Down (subscript)
    pub const PLD: char = 0x8B as char;
    /// Partial Line Up (superscript)
    pub const PLU: char = 0x8C as char;
    /// Reverse Index, go up one line, reverse scroll if necessary
    pub const RI: char = 0x8D as char;
    /// Single Shift to G2
    pub const SS2: char = 0x8E as char;
    /// Single Shift to G3 (VT100 uses this for sending PF keys)
    pub const SS3: char = 0x8F as char;
    /// Device Control String, terminated by ST (VT125 enters graphics)
    pub const DCS: char = 0x90 as char;
    /// Private Use 1
    pub const PU1: char = 0x91 as char;
    /// Private Use 2
    pub const PU2: char = 0x92 as char;
    /// Set Transmit State
    pub const STS: char = 0x93 as char;
    /// Cancel CHaracter, ignore previous character
    pub const CCH: char = 0x94 as char;
    /// Message Waiting, turns on an indicator on the terminal
    pub const MW: char = 0x95 as char;
    /// Start of Protected Area
    pub const SPA: char = 0x96 as char;
    /// End of Protected Area
    pub const EPA: char = 0x97 as char;
    /// SOS
    pub const SOS: char = 0x98 as char;
    /// SGCI
    pub const SGCI: char = 0x99 as char;
    /// DECID - Identify Terminal
    pub const DECID: char = 0x9a as char;
    /// Control Sequence Introducer (described in a seperate table)
    pub const CSI: char = 0x9B as char;
    /// String Terminator (VT125 exits graphics)
    pub const ST: char = 0x9C as char;
    /// Operating System Command (reprograms intelligent terminal)
    pub const OSC: char = 0x9D as char;
    /// Privacy Message (password verification), terminated by ST
    pub const PM: char = 0x9E as char;
    /// Application Program Command (to word processor), term by ST
    pub const APC: char = 0x9F as char;
}

#[derive(Debug)]
enum State {
    /// Base state
    ///
    /// Expects control characters or characters for display
    Base,

    /// Just got an escape
    Escape,

    /// Parsing a CSI escape,
    Csi,

    /// Parsing non csi escape
    EscapeOther,
}

impl Default for State {
    fn default() -> State {
        State::Base
    }
}

/// Tests for parsing escape sequences
///
/// Byte sequences used in these tests are recording of pty stdout.
#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};
    use index::{Line, Column};
    use super::{Parser, Handler, Attr, TermInfo};
    use ::Rgb;

    #[derive(Default)]
    struct AttrHandler {
        attr: Option<Attr>,
    }

    impl Handler for AttrHandler {
        fn terminal_attribute(&mut self, attr: Attr) {
            self.attr = Some(attr);
        }
    }

    impl TermInfo for AttrHandler {
        fn lines(&self) -> Line {
            Line(24)
        }

        fn cols(&self) -> Column {
            Column(80)
        }
    }

    #[test]
    fn parse_control_attribute() {
        static BYTES: &'static [u8] = &[
            0x1b, 0x5b, 0x31, 0x6d
        ];

        let cursor = Cursor::new(BYTES);
        let mut parser = Parser::new();
        let mut handler = AttrHandler::default();

        for c in cursor.chars() {
            parser.advance(&mut handler, c.unwrap());
        }

        assert_eq!(handler.attr, Some(Attr::Bold));
    }

    #[test]
    fn parse_truecolor_attr() {
        static BYTES: &'static [u8] = &[
            0x1b, 0x5b, 0x33, 0x38, 0x3b, 0x32, 0x3b, 0x31, 0x32,
            0x38, 0x3b, 0x36, 0x36, 0x3b, 0x32, 0x35, 0x35, 0x6d
        ];

        let cursor = Cursor::new(BYTES);
        let mut parser = Parser::new();
        let mut handler = AttrHandler::default();

        for c in cursor.chars() {
            parser.advance(&mut handler, c.unwrap());
        }

        let spec = Rgb {
            r: 128,
            g: 66,
            b: 255
        };

        assert_eq!(handler.attr, Some(Attr::ForegroundSpec(spec)));
    }

    /// No exactly a test; useful for debugging
    #[test]
    fn parse_zsh_startup() {
        static BYTES: &'static [u8] = &[
            0x1b, 0x5b, 0x31, 0x6d, 0x1b, 0x5b, 0x37, 0x6d, 0x25, 0x1b, 0x5b, 0x32, 0x37, 0x6d,
            0x1b, 0x5b, 0x31, 0x6d, 0x1b, 0x5b, 0x30, 0x6d, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
            0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
            0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
            0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
            0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
            0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
            0x20, 0x20, 0x20, 0x0d, 0x20, 0x0d, 0x0d, 0x1b, 0x5b, 0x30, 0x6d, 0x1b, 0x5b, 0x32,
            0x37, 0x6d, 0x1b, 0x5b, 0x32, 0x34, 0x6d, 0x1b, 0x5b, 0x4a, 0x6a, 0x77, 0x69, 0x6c,
            0x6d, 0x40, 0x6a, 0x77, 0x69, 0x6c, 0x6d, 0x2d, 0x64, 0x65, 0x73, 0x6b, 0x20, 0x1b,
            0x5b, 0x30, 0x31, 0x3b, 0x33, 0x32, 0x6d, 0xe2, 0x9e, 0x9c, 0x20, 0x1b, 0x5b, 0x30,
            0x31, 0x3b, 0x33, 0x32, 0x6d, 0x20, 0x1b, 0x5b, 0x33, 0x36, 0x6d, 0x7e, 0x2f, 0x63,
            0x6f, 0x64, 0x65
        ];

        let cursor = Cursor::new(BYTES);
        let mut handler = AttrHandler::default();
        let mut parser = Parser::new();

        for c in cursor.chars() {
            parser.advance(&mut handler, c.unwrap());
        }
    }
}
