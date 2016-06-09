/// Exports the `Term` type which is a high-level API for the Grid
use std::ops::Range;

use ansi::{self, Attr};
use grid::{self, Grid, CellFlags};
use tty;
use ::Rgb;

/// tomorrow night bright
///
/// because contrast
pub static COLORS: &'static [Rgb] = &[
    Rgb {r: 0x00, g: 0x00, b: 0x00}, // Black
    Rgb {r: 0xd5, g: 0x4e, b: 0x53}, // Red
    Rgb {r: 0xb9, g: 0xca, b: 0x4a}, // Green
    Rgb {r: 0xe6, g: 0xc5, b: 0x47}, // Yellow
    Rgb {r: 0x7a, g: 0xa6, b: 0xda}, // Blue
    Rgb {r: 0xc3, g: 0x97, b: 0xd8}, // Magenta
    Rgb {r: 0x70, g: 0xc0, b: 0xba}, // Cyan
    Rgb {r: 0x42, g: 0x42, b: 0x42}, // White
    Rgb {r: 0x66, g: 0x66, b: 0x66}, // Bright black
    Rgb {r: 0xff, g: 0x33, b: 0x34}, // Bright red
    Rgb {r: 0x9e, g: 0xc4, b: 0x00}, // Bright green
    Rgb {r: 0xe7, g: 0xc5, b: 0x47}, // Bright yellow
    Rgb {r: 0x7a, g: 0xa6, b: 0xda}, // Bright blue
    Rgb {r: 0xb7, g: 0x7e, b: 0xe0}, // Bright magenta
    Rgb {r: 0x54, g: 0xce, b: 0xd6}, // Bright cyan
    Rgb {r: 0x2a, g: 0x2a, b: 0x2a}, // Bright white
];

pub mod mode {
    bitflags! {
        pub flags TermMode: u32 {
            const TEXT_CURSOR = 0b00000001,
        }
    }
}

pub use self::mode::TermMode;

pub const CURSOR_SHAPE: char = '█';

pub const DEFAULT_FG: Rgb = Rgb { r: 0xea, g: 0xea, b: 0xea};
pub const DEFAULT_BG: Rgb = Rgb { r: 0, g: 0, b: 0};
pub const TAB_SPACES: usize = 8;

/// State for cursor
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Cursor {
    pub x: u16,
    pub y: u16,
}

impl Default for Cursor {
    fn default() -> Cursor {
        Cursor { x: 0, y: 0 }
    }
}

impl Cursor {
    pub fn goto(&mut self, x: u16, y: u16) {
        self.x = x;
        self.y = y;
    }

    pub fn advance(&mut self, rows: i64, cols: i64) {
        self.x = (self.x as i64 + cols) as u16;
        self.y = (self.y as i64 + rows) as u16;
    }
}

pub struct Term {
    /// The grid
    grid: Grid,

    /// Alternate grid
    alt_grid: Grid,

    /// Alt is active
    alt: bool,

    /// Reference to the underlying tty
    _tty: tty::Tty,

    /// The cursor
    cursor: Cursor,

    /// Alt cursor
    alt_cursor: Cursor,

    /// Active foreground color
    fg: Rgb,

    /// Active background color
    bg: Rgb,

    /// Tabstops
    tabs: Vec<bool>,

    /// Cell attributes
    attr: grid::CellFlags,

    /// Mode flags
    mode: TermMode,

    /// Scroll region
    scroll_region: Range<usize>,
}

impl Term {
    pub fn new(tty: tty::Tty, grid: Grid) -> Term {

        let mut tabs = (0..grid.num_cols()).map(|i| i % TAB_SPACES == 0)
                                       .collect::<Vec<bool>>();
        tabs[0] = false;

        let alt = grid.clone();
        let scroll_region = 0..grid.num_rows();

        Term {
            grid: grid,
            alt_grid: alt,
            alt: false,
            cursor: Cursor::default(),
            alt_cursor: Cursor::default(),
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            _tty: tty,
            tabs: tabs,
            attr: CellFlags::empty(),
            mode: TermMode::empty(),
            scroll_region: scroll_region,
        }
    }

    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    #[inline]
    pub fn mode(&self) -> &TermMode {
        &self.mode
    }

    pub fn swap_alt(&mut self) {
        self.alt = !self.alt;
        ::std::mem::swap(&mut self.grid, &mut self.alt_grid);
        ::std::mem::swap(&mut self.cursor, &mut self.alt_cursor);

        if self.alt {
            self.grid.clear();
        }
    }

    #[inline]
    pub fn cursor_x(&self) -> u16 {
        self.cursor.x
    }

    #[inline]
    pub fn cursor_y(&self) -> u16 {
        self.cursor.y
    }

    #[inline]
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// Set character in current cursor position
    fn set_char(&mut self, c: char) {
        if self.cursor.x == self.grid.num_cols() as u16 {
            println!("wrapping");
            self.cursor.y += 1;
            self.cursor.x = 0;
        }

        if self.cursor.y == self.grid.num_rows() as u16 {
            panic!("cursor fell off grid");
        }

        let cell = &mut self.grid[self.cursor];
        cell.c = c;
        cell.fg = self.fg;
        cell.bg = self.bg;
        cell.flags = self.attr;
    }

    /// Convenience function for scrolling
    fn scroll(&mut self, count: isize) {
        println!("[TERM] scrolling {} lines", count);
        self.grid.scroll(self.scroll_region.clone(), count);
        if count > 0 {
            // Scrolled down, so need to clear from bottom
            let start = self.scroll_region.end - (count as usize);
            self.grid.clear_region(start..self.scroll_region.end);
        } else {
            // Scrolled up, clear from top
            let end = self.scroll_region.start + ((-count) as usize);
            self.grid.clear_region(self.scroll_region.start..end);
        }
    }
}

impl ansi::TermInfo for Term {
    #[inline]
    fn rows(&self) -> usize {
        self.grid.num_rows()
    }

    #[inline]
    fn cols(&self) -> usize {
        self.grid.num_cols()
    }
}

impl ansi::Handler for Term {
    /// A character to be displayed
    #[inline]
    fn input(&mut self, c: char) {
        self.set_char(c);
        self.cursor.x += 1;
    }

    fn goto(&mut self, x: i64, y: i64) {
        println!("goto: x={}, y={}", x, y);
        self.cursor.goto(x as u16, y as u16);
    }
    fn goto_row(&mut self, y: i64) {
        println!("goto_row: {}", y);
        let x = self.cursor_x();
        self.cursor.goto(x, y as u16);
    }
    fn goto_col(&mut self, x: i64) {
        println!("goto_col: {}", x);
        let y = self.cursor_y();
        self.cursor.goto(x as u16, y);
    }

    fn insert_blank(&mut self, num: i64) { println!("insert_blank: {}", num); }

    fn move_up(&mut self, rows: i64) {
        println!("move_up: {}", rows);
        self.cursor.advance(-rows, 0);
    }

    fn move_down(&mut self, rows: i64) {
        println!("move_down: {}", rows);
        self.cursor.advance(rows, 0);
    }

    fn move_forward(&mut self, cols: i64) {
        println!("move_forward: {}", cols);
        self.cursor.advance(0, cols);
    }

    fn move_backward(&mut self, spaces: i64) {
        println!("move_backward: {}", spaces);
        self.cursor.advance(0, -spaces);
    }

    fn identify_terminal(&mut self) { println!("identify_terminal"); }
    fn move_down_and_cr(&mut self, rows: i64) { println!("move_down_and_cr: {}", rows); }
    fn move_up_and_cr(&mut self, rows: i64) { println!("move_up_and_cr: {}", rows); }
    fn put_tab(&mut self, mut count: i64) {
        println!("put_tab: {}", count);

        let mut x = self.cursor_x();
        while x < self.grid.num_cols() as u16 && count != 0 {
            count -= 1;
            loop {
                if x == self.grid.num_cols() as u16 || self.tabs[x as usize] {
                    break;
                }
                x += 1;
            }
        }

        self.cursor.x = x;
    }

    /// Backspace `count` characters
    #[inline]
    fn backspace(&mut self, count: i64) {
        println!("backspace");
        // TODO this is incorrect; count unused
        self.cursor.x -= 1;
        self.set_char(' ');
    }

    /// Carriage return
    #[inline]
    fn carriage_return(&mut self) {
        println!("carriage_return");
        self.cursor.x = 0;
    }

    /// Linefeed
    #[inline]
    fn linefeed(&mut self) {
        println!("linefeed");
        // TODO handle scroll? not clear what parts of this the pty handle
        if self.cursor_y() + 1 >= self.scroll_region.end as u16 {
            self.scroll(1);
            self.clear_line(ansi::LineClearMode::Right);
        } else {
            self.cursor.y += 1;
        }
    }

    /// Set current position as a tabstop
    fn bell(&mut self) { println!("bell"); }
    fn substitute(&mut self) { println!("substitute"); }
    fn newline(&mut self) { println!("newline"); }
    fn set_horizontal_tabstop(&mut self) { println!("set_horizontal_tabstop"); }
    fn scroll_up(&mut self, rows: i64) {
        println!("scroll_up: {}", rows);
        self.scroll(-rows as isize);
    }
    fn scroll_down(&mut self, rows: i64) {
        println!("scroll_down: {}", rows);
        self.scroll(rows as isize);
    }
    fn insert_blank_lines(&mut self, count: i64) {
        println!("insert_blank_lines: {}", count);
        if self.scroll_region.contains(self.cursor_y() as usize) {
            self.scroll(-count as isize);
        }
    }
    fn delete_lines(&mut self, count: i64) {
        if self.scroll_region.contains(self.cursor_y() as usize) {
            self.scroll(count as isize);
        }
    }
    fn erase_chars(&mut self, count: i64) { println!("erase_chars: {}", count); }
    fn delete_chars(&mut self, count: i64) { println!("delete_chars: {}", count); }
    fn move_backward_tabs(&mut self, count: i64) { println!("move_backward_tabs: {}", count); }
    fn move_forward_tabs(&mut self, count: i64) { println!("move_forward_tabs: {}", count); }
    fn save_cursor_position(&mut self) { println!("save_cursor_position"); }
    fn restore_cursor_position(&mut self) { println!("restore_cursor_position"); }
    fn clear_line(&mut self, mode: ansi::LineClearMode) {
        println!("clear_line: {:?}", mode);
        match mode {
            ansi::LineClearMode::Right => {
                let row = &mut self.grid[self.cursor.y as usize];
                let start = self.cursor.x as usize;
                for cell in row[start..].iter_mut() {
                    cell.reset();
                }
            },
            _ => (),
        }
    }
    fn clear_screen(&mut self, mode: ansi::ClearMode) {
        println!("clear_screen: {:?}", mode);
        match mode {
            ansi::ClearMode::Below => {
                let start = self.cursor_y() as usize;
                let end = self.grid.num_rows();
                for i in start..end {
                    let row = &mut self.grid[i];
                    for cell in row.iter_mut() {
                        cell.c = ' ';
                    }
                }
            },
            ansi::ClearMode::All => {
                self.grid.clear();
            },
            _ => {
                panic!("ansi::ClearMode::Above not implemented");
            }
        }
    }
    fn clear_tabs(&mut self, mode: ansi::TabulationClearMode) { println!("clear_tabs: {:?}", mode); }
    fn reset_state(&mut self) { println!("reset_state"); }
    fn reverse_index(&mut self) {
        println!("reverse_index");
        // if cursor is at the top
        if self.cursor.y == 0 {
            self.scroll(-1);
        } else {
            // can't wait for nonlexical lifetimes.. omg borrowck
            let x = self.cursor.x;
            let y = self.cursor.y;
            self.cursor.goto(x, y - 1);
        }
    }

    /// set a terminal attribute
    fn terminal_attribute(&mut self, attr: Attr) {
        match attr {
            Attr::DefaultForeground => {
                self.fg = DEFAULT_FG;
            },
            Attr::DefaultBackground => {
                self.bg = DEFAULT_BG;
            },
            Attr::Foreground(named_color) => {
                self.fg = COLORS[named_color as usize];
            },
            Attr::Background(named_color) => {
                self.bg = COLORS[named_color as usize];
            },
            Attr::ForegroundSpec(rgb) => {
                self.fg = rgb;
            },
            Attr::BackgroundSpec(rgb) => {
                self.bg = rgb;
            },
            Attr::Reset => {
                self.fg = DEFAULT_FG;
                self.bg = DEFAULT_BG;
                self.attr = CellFlags::empty();
            },
            Attr::Reverse => self.attr.insert(grid::INVERSE),
            Attr::CancelReverse => self.attr.remove(grid::INVERSE),
            Attr::Bold => self.attr.insert(grid::BOLD),
            Attr::CancelBoldDim => self.attr.remove(grid::BOLD),
            Attr::Italic => self.attr.insert(grid::ITALIC),
            Attr::CancelItalic => self.attr.remove(grid::ITALIC),
            Attr::Underscore => self.attr.insert(grid::UNDERLINE),
            Attr::CancelUnderline => self.attr.remove(grid::UNDERLINE),
            _ => {
                println!("Term got unhandled attr: {:?}", attr);
            }
        }
    }

    fn set_mode(&mut self, mode: ansi::Mode) {
        println!("set_mode: {:?}", mode);
        match mode {
            ansi::Mode::SwapScreenAndSetRestoreCursor => self.swap_alt(),
            ansi::Mode::TextCursor => self.mode.insert(mode::TEXT_CURSOR),
            _ => {
                println!(".. ignoring set_mode");
            }
        }
    }

    fn unset_mode(&mut self,mode: ansi::Mode) {
        println!("unset_mode: {:?}", mode);
        match mode {
            ansi::Mode::SwapScreenAndSetRestoreCursor => self.swap_alt(),
            ansi::Mode::TextCursor => self.mode.remove(mode::TEXT_CURSOR),
            _ => {
                println!(".. ignoring unset_mode");
            }
        }
    }

    fn set_scrolling_region(&mut self, top: i64, bot: i64) {
        println!("set scroll region: {:?} - {:?}", top, bot);
        // 1 is added to bottom for inclusive range
        self.scroll_region = (top as usize)..((bot as usize) + 1);
    }
}
