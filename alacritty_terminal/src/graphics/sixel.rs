//! This module implements a parser for the Sixel protocol, and it is based on the
//! chapter [SIXEL GRAPHICS EXTENSION] of the DEC reference manual.
//!
//! [SIXEL GRAPHICS EXTENSION]: https://archive.org/details/bitsavers_decstandar0VideoSystemsReferenceManualDec91_74264381/page/n907/mode/2up
//!
//! # Limitations
//!
//! The parser have the following limitations:
//!
//! * A single image can use up to 1024 different colors.
//!
//!   The Sixel reference requires 256, but allow more colors.
//!
//! * Image dimensions are limited to 4096 x 4096.
//!
//! * Pixel aspect ratio parameters are ignored.
//!
//!   The Sixel references specifies some parameters to change the pixel
//!   aspect ratio, but multiple implementations always use 1:1, so these
//!   parameters have no real effect.
use std::cmp::max;
use std::{fmt, mem};

use crate::graphics::{ColorType, GraphicData, GraphicId, MAX_GRAPHIC_DIMENSIONS};
use crate::vte::ansi::Rgb;

use log::trace;
use vte::Params;

/// Type for color registers.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
struct ColorRegister(u16);

/// Number of color registers.
pub const MAX_COLOR_REGISTERS: usize = 1024;

/// Color register for transparent pixels.
const REG_TRANSPARENT: ColorRegister = ColorRegister(u16::MAX);

/// Number of parameters allowed in a single Sixel command.
const MAX_COMMAND_PARAMS: usize = 5;

#[derive(Debug)]
pub enum Error {
    /// Image dimensions are too big.
    TooBigImage { width: usize, height: usize },

    /// A component in a color introducer is not valid.
    InvalidColorComponent { register: u16, component_value: u16 },

    /// The coordinate system to define the color register is not valid.
    InvalidColorCoordinateSystem { register: u16, coordinate_system: u16 },
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooBigImage { width, height } => {
                write!(fmt, "The image dimensions are too big ({}, {})", width, height)
            },

            Error::InvalidColorComponent { register, component_value } => {
                write!(fmt, "Invalid color component {} for register {}", component_value, register)
            },

            Error::InvalidColorCoordinateSystem { register, coordinate_system } => {
                write!(
                    fmt,
                    "Invalid color coordinate system {} for register {}",
                    coordinate_system, register
                )
            },
        }
    }
}

/// Commands found in the data stream.
#[derive(Debug)]
enum SixelCommand {
    /// Specifies a repeat count before a sixel.
    ///
    /// Its only parameter is the repeat count.
    RepeatIntroducer,

    /// Defines raster attributes for the following data.
    ///
    /// It expects 4 parameters:
    ///
    /// 1. Pixel aspect ratio numerator (relative height).
    /// 2. Pixel aspect ratio denominator (relative width).
    /// 3. Horizontal Extent.
    /// 4. Vertical Extent.
    SetRasterAttributes,

    /// Starts a color selection sequence.
    ///
    /// The first parameter is the register number.
    ///
    /// Optionally, it can receive 4 more parameters:
    ///
    /// 1. Color coordinate system. `1` for HLS, `2` for RGB.
    /// 2. Hue angle, or red.
    /// 3. Lightness, or green.
    /// 4. Saturation, or blue.
    ColorIntroducer,

    /// Moves the active position to the graphic left margin.
    CarriageReturn,

    /// Moves the active position to the graphic left margin
    /// and one row of sixels.
    NextLine,
}

/// Parser for commands found in the picture definition.
#[derive(Debug)]
struct CommandParser {
    /// Active command.
    command: SixelCommand,

    /// Parameter values.
    ///
    /// If a value is greater than `u16::MAX`, it will be kept as `u16::MAX`.
    ///
    /// Parameters after `MAX_COMMAND_PARAMS` are ignored.
    params: [u16; MAX_COMMAND_PARAMS],

    /// Current position.
    params_position: usize,
}

impl CommandParser {
    fn new(command: SixelCommand) -> CommandParser {
        CommandParser { command, params: [0; MAX_COMMAND_PARAMS], params_position: 0 }
    }

    fn put(&mut self, byte: u8) {
        let pos = self.params_position;
        if pos < MAX_COMMAND_PARAMS {
            match byte {
                b'0'..=b'9' => {
                    self.params[pos] =
                        self.params[pos].saturating_mul(10).saturating_add((byte - b'0') as u16);
                },

                b';' => {
                    self.params_position += 1;
                },

                _ => (), // Ignore unknown bytes.
            }
        }
    }

    /// Apply the execution of the active command to the parser.
    fn finish(self, parser: &mut Parser) -> Result<(), Error> {
        match self.command {
            SixelCommand::RepeatIntroducer => {
                parser.repeat_count = self.params[0] as usize;
            },

            SixelCommand::SetRasterAttributes => {
                if self.params_position >= 3 {
                    let width = self.params[2] as usize;
                    let height = self.params[3] as usize;
                    parser.ensure_size(width, height)?;
                }
            },

            SixelCommand::ColorIntroducer => {
                let register = ColorRegister(self.params[0]);

                if self.params_position >= 4 {
                    macro_rules! p {
                        ($index:expr, $limit:expr) => {
                            match self.params[$index] {
                                x if x <= $limit => x,
                                x => {
                                    return Err(Error::InvalidColorComponent {
                                        register: register.0,
                                        component_value: x,
                                    })
                                },
                            }
                        };

                        ($index:expr) => {
                            p!($index, 100)
                        };
                    }

                    let rgb = match self.params[1] {
                        // HLS.
                        1 => hls_to_rgb(p!(2, 360), p!(3), p!(4)),

                        // RGB.
                        2 => rgb(p!(2), p!(3), p!(4), 100),

                        // Invalid coordinate system.
                        x => {
                            return Err(Error::InvalidColorCoordinateSystem {
                                register: register.0,
                                coordinate_system: x,
                            })
                        },
                    };

                    parser.set_color_register(register, rgb);
                }

                parser.selected_color_register = register;
            },

            SixelCommand::CarriageReturn => {
                parser.x = 0;
            },

            SixelCommand::NextLine => {
                parser.x = 0;
                parser.y += 6;
            },
        }

        Ok(())
    }
}

/// A group of 6 vertical pixels.
struct Sixel(u8);

impl Sixel {
    /// Create a new sixel.
    ///
    /// It expects the byte value from the picture definition stream.
    #[inline]
    fn new(byte: u8) -> Sixel {
        debug_assert!((0x3F..=0x7E).contains(&byte));
        Sixel(byte - 0x3F)
    }

    /// Return how many rows are printed in the sixel.
    #[inline]
    fn height(&self) -> usize {
        8 - self.0.leading_zeros() as usize
    }

    /// Return an iterator to get dots in the sixel.
    #[inline]
    fn dots(&self) -> impl Iterator<Item = bool> {
        let sixel = self.0;
        (0..6).map(move |position| sixel & (1 << position) != 0)
    }
}

/// Parser of the picture definition in a Sixel data stream.
#[derive(Default, Debug)]
pub struct Parser {
    /// Active command to be parsed.
    command_parser: Option<CommandParser>,

    /// Current picture width.
    width: usize,

    /// Current picture height.
    height: usize,

    /// Current picture pixels.
    pixels: Vec<ColorRegister>,

    /// Indicates the register color for empty pixels.
    background: ColorRegister,

    /// RGB values for every register.
    color_registers: Vec<Rgb>,

    /// Selected color register.
    selected_color_register: ColorRegister,

    /// Repeat count for the next sixel.
    repeat_count: usize,

    /// Horizontal position of the active sixel.
    x: usize,

    /// Vertical position of the active sixel.
    y: usize,
}

impl Parser {
    /// Creates a new parser.
    pub fn new(params: &Params, shared_palette: Option<Vec<Rgb>>) -> Parser {
        trace!("Start Sixel parser");

        let mut parser = Parser::default();

        // According to the Sixel reference, the second parameter (Ps2) is
        // the background selector. It controls how to show pixels without
        // an explicit color, and it accepts the following values:
        //
        //   0   device default action
        //   1   no action (don't change zero value pixels)
        //   2   set zero value pixels to background color
        //
        // We replicate the xterm's behaviour:
        //
        //  - If it is set to `1`, the background is transparent.
        //  - For any other value, the background is the color register 0.

        let ps2 = params.iter().nth(1).and_then(|param| param.iter().next().copied()).unwrap_or(0);
        parser.background = if ps2 == 1 { REG_TRANSPARENT } else { ColorRegister(0) };

        if let Some(color_registers) = shared_palette {
            parser.color_registers = color_registers;
        } else {
            init_color_registers(&mut parser);
        }

        parser
    }

    /// Parse a byte from the Sixel stream.
    pub fn put(&mut self, byte: u8) -> Result<(), Error> {
        match byte {
            b'!' => self.start_command(SixelCommand::RepeatIntroducer)?,

            b'"' => self.start_command(SixelCommand::SetRasterAttributes)?,

            b'#' => self.start_command(SixelCommand::ColorIntroducer)?,

            b'$' => self.start_command(SixelCommand::CarriageReturn)?,

            b'-' => self.start_command(SixelCommand::NextLine)?,

            b'0'..=b'9' | b';' => {
                if let Some(command_parser) = &mut self.command_parser {
                    command_parser.put(byte);
                }
            },

            0x3F..=0x7E => self.add_sixel(Sixel::new(byte))?,

            _ => {
                // Invalid bytes are ignored, but we still have to finish any
                // active command.

                self.finish_command()?;
            },
        }

        Ok(())
    }

    #[inline]
    fn start_command(&mut self, command: SixelCommand) -> Result<(), Error> {
        self.finish_command()?;
        self.command_parser = Some(CommandParser::new(command));
        Ok(())
    }

    #[inline]
    fn finish_command(&mut self) -> Result<(), Error> {
        if let Some(command_parser) = self.command_parser.take() {
            command_parser.finish(self)?;
        }

        Ok(())
    }

    /// Set the RGB color for a register.
    ///
    /// Color components are expected to be in the range of 0..=100.
    fn set_color_register(&mut self, register: ColorRegister, rgb: Rgb) {
        let register = register.0 as usize;

        if register >= MAX_COLOR_REGISTERS {
            return;
        }

        if self.color_registers.len() <= register {
            self.color_registers.resize(register + 1, Rgb { r: 0, g: 0, b: 0 })
        }

        self.color_registers[register] = rgb;
    }

    /// Check if the current picture is big enough for the given dimensions. If
    /// not, the picture is resized.
    fn ensure_size(&mut self, width: usize, height: usize) -> Result<(), Error> {
        // Do nothing if the current picture is big enough.
        if self.width >= width && self.height >= height {
            return Ok(());
        }

        if width > MAX_GRAPHIC_DIMENSIONS[0] || height > MAX_GRAPHIC_DIMENSIONS[1] {
            return Err(Error::TooBigImage { width, height });
        }

        trace!(
            "Set Sixel image dimensions to {}x{}",
            max(self.width, width),
            max(self.height, height),
        );

        // If there is no current picture, creates a new one.
        if self.pixels.is_empty() {
            self.width = width;
            self.height = height;
            self.pixels = vec![self.background; width * height];
            return Ok(());
        }

        // If current width is big enough, we only need to add more pixels
        // after the current buffer.
        if self.width >= width {
            self.pixels.resize(height * self.width, self.background);
            self.height = height;
            return Ok(());
        }

        // At this point, we know that the new width is greater than the
        // current one, so we have to extend the buffer and move the rows to
        // their new positions.
        let height = usize::max(height, self.height);

        self.pixels.resize(height * width, self.background);

        for y in (0..self.height).rev() {
            for x in (0..self.width).rev() {
                let old = y * self.width + x;
                let new = y * width + x;
                self.pixels.swap(old, new);
            }
        }

        self.width = width;
        self.height = height;
        Ok(())
    }

    /// Add a sixel using the selected color register, and move the active
    /// position.
    fn add_sixel(&mut self, sixel: Sixel) -> Result<(), Error> {
        self.finish_command()?;

        // Take the repeat count and reset it.
        //
        // `max` function is used because the Sixel reference specifies
        // that a repeat count of zero implies a repeat count of 1.
        let repeat = max(1, mem::take(&mut self.repeat_count));

        self.ensure_size(self.x + repeat, self.y + sixel.height())?;

        if sixel.0 != 0 {
            let mut index = self.width * self.y + self.x;
            for dot in sixel.dots() {
                if dot {
                    for pixel in &mut self.pixels[index..index + repeat] {
                        *pixel = self.selected_color_register;
                    }
                }

                index += self.width;
            }
        }

        self.x += repeat;

        Ok(())
    }

    /// Returns the final graphic to append to the grid, with the palette
    /// built in the process.
    pub fn finish(mut self) -> Result<(GraphicData, Vec<Rgb>), Error> {
        self.finish_command()?;

        trace!(
            "Finish Sixel parser: width={}, height={}, color_registers={}",
            self.width,
            self.height,
            self.color_registers.len()
        );

        let mut rgba_pixels = Vec::with_capacity(self.pixels.len() * 4);

        let mut is_opaque = true;

        for &register in &self.pixels {
            let pixel = {
                if register == REG_TRANSPARENT {
                    is_opaque = false;
                    [0; 4]
                } else {
                    match self.color_registers.get(register.0 as usize) {
                        None => [0, 0, 0, 255],
                        Some(color) => [color.r, color.g, color.b, 255],
                    }
                }
            };

            rgba_pixels.extend_from_slice(&pixel);
        }

        let data = GraphicData {
            id: GraphicId(0),
            height: self.height,
            width: self.width,
            color_type: ColorType::Rgba,
            pixels: rgba_pixels,
            is_opaque,
        };

        Ok((data, self.color_registers))
    }
}

/// Compute a RGB value from HLS.
///
/// Input and output values are in the range of `0..=100`.
///
/// The implementation is a direct port of the same function in the
/// libsixel's code.
fn hls_to_rgb(hue: u16, lum: u16, sat: u16) -> Rgb {
    if sat == 0 {
        return rgb(lum, lum, lum, 100);
    }

    let lum = lum as f64;

    let c0 = if lum > 50.0 { ((lum * 4.0) / 100.0) - 1.0 } else { -(2.0 * (lum / 100.0) - 1.0) };
    let c = sat as f64 * (1.0 - c0) / 2.0;

    let max = lum + c;
    let min = lum - c;

    let hue = (hue + 240) % 360;
    let h = hue as f64;

    let (r, g, b) = match hue / 60 {
        0 => (max, min + (max - min) * (h / 60.0), min),
        1 => (min + (max - min) * ((120.0 - h) / 60.0), max, min),
        2 => (min, max, min + (max - min) * ((h - 120.0) / 60.0)),
        3 => (min, min + (max - min) * ((240.0 - h) / 60.0), max),
        4 => (min + (max - min) * ((h - 240.0) / 60.0), min, max),
        5 => (max, min, min + (max - min) * ((360.0 - h) / 60.0)),
        _ => (0., 0., 0.),
    };

    fn clamp(x: f64) -> u8 {
        let x = f64::round(x * 255. / 100.) % 256.;
        if x < 0. {
            0
        } else {
            x as u8
        }
    }

    Rgb { r: clamp(r), g: clamp(g), b: clamp(b) }
}

/// Initialize the color registers using the colors from the VT-340 terminal.
///
/// There is no official documentation about these colors, but multiple Sixel
/// implementations assume this palette.
fn init_color_registers(parser: &mut Parser) {
    parser.set_color_register(ColorRegister(0), rgb(0, 0, 0, 100));
    parser.set_color_register(ColorRegister(1), rgb(20, 20, 80, 100));
    parser.set_color_register(ColorRegister(2), rgb(80, 13, 13, 100));
    parser.set_color_register(ColorRegister(3), rgb(20, 80, 20, 100));
    parser.set_color_register(ColorRegister(4), rgb(80, 20, 80, 100));
    parser.set_color_register(ColorRegister(5), rgb(20, 80, 80, 100));
    parser.set_color_register(ColorRegister(6), rgb(80, 80, 20, 100));
    parser.set_color_register(ColorRegister(7), rgb(53, 53, 53, 100));
    parser.set_color_register(ColorRegister(8), rgb(26, 26, 26, 100));
    parser.set_color_register(ColorRegister(9), rgb(33, 33, 60, 100));
    parser.set_color_register(ColorRegister(10), rgb(60, 26, 26, 100));
    parser.set_color_register(ColorRegister(11), rgb(33, 60, 33, 100));
    parser.set_color_register(ColorRegister(12), rgb(60, 33, 60, 100));
    parser.set_color_register(ColorRegister(13), rgb(33, 60, 60, 100));
    parser.set_color_register(ColorRegister(14), rgb(60, 60, 33, 100));
    parser.set_color_register(ColorRegister(15), rgb(80, 80, 80, 100));
}

/// Create a `Rgb` instance, scaling the components when necessary.
#[inline]
fn rgb(r: u16, g: u16, b: u16, max: u16) -> Rgb {
    if max == 255 {
        Rgb { r: r as u8, b: b as u8, g: g as u8 }
    } else {
        let r = ((r * 255 + max / 2) / max) as u8;
        let g = ((g * 255 + max / 2) / max) as u8;
        let b = ((b * 255 + max / 2) / max) as u8;
        Rgb { r, g, b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    macro_rules! put_bytes {
        ($parser:expr, $data:expr) => {
            #[allow(clippy::string_lit_as_bytes)]
            for &byte in $data.as_bytes() {
                let _ = $parser.put(byte);
            }
        };
    }

    #[test]
    fn parse_command_parameters() {
        let mut command_parser = CommandParser::new(SixelCommand::ColorIntroducer);
        put_bytes!(command_parser, "65535;1;2;3;4;5");

        assert_eq!(command_parser.params_position, 5);
        assert_eq!(command_parser.params[0], 65535);
        assert_eq!(command_parser.params[1], 1);
        assert_eq!(command_parser.params[2], 2);
        assert_eq!(command_parser.params[3], 3);
        assert_eq!(command_parser.params[4], 4);
    }

    #[test]
    fn set_color_registers() {
        let mut parser = Parser::default();
        put_bytes!(parser, "#1;2;30;100;0#200;1;20;75;50.");

        assert!(parser.color_registers.len() >= 200);

        assert_eq!(parser.color_registers[1], Rgb { r: 77, g: 255, b: 0 });
        assert_eq!(parser.color_registers[200], Rgb { r: 213, g: 255, b: 128 });

        assert_eq!(parser.selected_color_register.0, 200);
    }

    #[test]
    fn convert_hls_colors() {
        // This test converts some values from HLS to RBG, and compares those
        // results with the values generated by the libsixel implementation
        // of the same function.
        //
        // We allow some difference between each component to ignore rounding
        // errors.

        // Reimplement abs_diff to be compatible with rustc before 1.60.
        fn abs_diff(x: u8, y: u8) -> u8 {
            if x > y {
                x - y
            } else {
                y - x
            }
        }

        macro_rules! assert_color {
            ($h:expr, $l:expr, $s:expr => $r:expr, $g:expr, $b:expr) => {
                let left = hls_to_rgb($h, $l, $s);
                let right = rgb($r, $g, $b, 255);

                assert!(
                    abs_diff(left.r, right.r) < 4
                        && abs_diff(left.g, right.g) < 4
                        && abs_diff(left.b, right.b) < 4,
                    "Expected {:?} Found {:?}",
                    right,
                    left,
                );
            };
        }

        assert_color!(282 , 33 , 87 =>  10 , 156 , 112);
        assert_color!( 45 , 36 , 78 => 128 ,  18 , 163);
        assert_color!(279 ,  9 , 93 =>   0 ,  43 ,  28);
        assert_color!(186 , 27 , 54 =>  97 , 105 ,  31);
        assert_color!( 93 , 66 , 75 => 107 , 230 , 173);
        assert_color!( 60 , 51 , 90 => 125 , 133 , 125);
        assert_color!(141 , 39 , 78 => 176 ,  74 ,  20);
        assert_color!(273 , 30 , 48 =>  38 , 112 ,  79);
        assert_color!(270 , 15 , 57 =>  15 ,  59 ,  38);
        assert_color!( 84 , 21 , 99 => 105 ,   0 ,  64);
        assert_color!(162 , 81 , 93 =>  59 , 145 , 352);
        assert_color!( 96 , 30 , 72 => 130 ,  20 ,  64);
        assert_color!(222 , 21 , 90 =>  33 ,  99 ,   5);
        assert_color!(306 , 33 , 39 =>  51 , 110 , 115);
        assert_color!(144 , 30 , 72 => 130 ,  64 ,  20);
        assert_color!( 27 ,  0 , 42 =>   0 ,   0 ,   0);
        assert_color!(123 , 10 ,  0 =>  26 ,  26 ,  26);
        assert_color!(279 ,  6 , 93 =>   0 ,  28 ,  18);
        assert_color!(270 , 45 , 69 =>  33 , 194 , 115);
        assert_color!(225 , 39 , 45 =>  77 , 143 ,  54);
    }

    #[test]
    fn resize_picture() -> Result<(), Error> {
        let mut parser = Parser { background: REG_TRANSPARENT, ..Parser::default() };

        const WIDTH: usize = 30;
        const HEIGHT: usize = 20;

        // Initialize a transparent picture with Set Raster Attributes.
        put_bytes!(parser, format!("\"1;1;{};{}.", WIDTH, HEIGHT));

        assert_eq!(parser.width, WIDTH);
        assert_eq!(parser.height, HEIGHT);
        assert_eq!(parser.pixels.len(), WIDTH * HEIGHT);

        assert!(parser.pixels.iter().all(|&pixel| pixel == REG_TRANSPARENT));

        // Fill each row with a different color register.
        for (n, row) in parser.pixels.chunks_mut(WIDTH).enumerate() {
            row.iter_mut().for_each(|pixel| *pixel = ColorRegister(n as u16));
        }

        // Increase height.
        //
        // New rows must be transparent.
        parser.ensure_size(WIDTH, HEIGHT + 5)?;

        assert_eq!(parser.width, WIDTH);
        assert_eq!(parser.height, HEIGHT + 5);
        assert_eq!(parser.pixels.len(), WIDTH * (HEIGHT + 5));

        for (n, row) in parser.pixels.chunks(WIDTH).enumerate() {
            let expected = if n < HEIGHT { ColorRegister(n as u16) } else { REG_TRANSPARENT };
            assert!(row.iter().all(|pixel| *pixel == expected));
        }

        // Increase both width and height.
        //
        // New rows and columns must be transparent.
        parser.ensure_size(WIDTH + 5, HEIGHT + 10)?;

        assert_eq!(parser.width, WIDTH + 5);
        assert_eq!(parser.height, HEIGHT + 10);
        assert_eq!(parser.pixels.len(), (WIDTH + 5) * (HEIGHT + 10));

        for (n, row) in parser.pixels.chunks(WIDTH + 5).enumerate() {
            if n < HEIGHT {
                assert!(row[..WIDTH].iter().all(|pixel| *pixel == ColorRegister(n as u16)));
                assert!(row[WIDTH..].iter().all(|pixel| *pixel == REG_TRANSPARENT));
            } else {
                assert!(row.iter().all(|pixel| *pixel == REG_TRANSPARENT));
            }
        }

        let graphics = parser.finish()?.0;
        assert!(!graphics.is_opaque);

        Ok(())
    }

    #[test]
    fn sixel_height() {
        assert_eq!(Sixel(0b000000).height(), 0);
        assert_eq!(Sixel(0b000001).height(), 1);
        assert_eq!(Sixel(0b000100).height(), 3);
        assert_eq!(Sixel(0b000101).height(), 3);
        assert_eq!(Sixel(0b101111).height(), 6);
    }

    #[test]
    fn sixel_positions() {
        macro_rules! dots {
            ($sixel:expr) => {
                Sixel($sixel).dots().collect::<Vec<_>>()
            };
        }

        assert_eq!(dots!(0b000000), &[false, false, false, false, false, false,]);
        assert_eq!(dots!(0b000001), &[true, false, false, false, false, false,]);
        assert_eq!(dots!(0b000100), &[false, false, true, false, false, false,]);
        assert_eq!(dots!(0b000101), &[true, false, true, false, false, false,]);
        assert_eq!(dots!(0b101111), &[true, true, true, true, false, true,]);
    }

    #[test]
    fn load_sixel_files() {
        let images_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sixel"));

        let test_images = ["testimage_im6", "testimage_libsixel", "testimage_ppmtosixel"];

        for test_image in &test_images {
            // Load Sixel data.
            let mut sixel = {
                let mut path = images_dir.join(test_image);
                path.set_extension("sixel");
                fs::read(path).unwrap()
            };

            // Remove DCS sequence from Sixel data.
            let dcs_end = sixel.iter().position(|&byte| byte == b'q').unwrap();
            sixel.drain(..=dcs_end);

            // Remove ST, which can be either "1B 5C" or "9C". To simplify the
            // code, we assume that any ESC byte is the start of the ST.
            if let Some(pos) = sixel.iter().position(|&b| b == 0x1B || b == 0x9C) {
                sixel.truncate(pos);
            }

            // Parse the data and get the GraphicData item.
            let mut parser = Parser::default();
            for byte in sixel {
                parser.put(byte).unwrap();
            }

            let graphics = parser.finish().unwrap().0;

            assert_eq!(graphics.width, 64);
            assert_eq!(graphics.height, 64);

            // Read the RGBA stream generated by ImageMagick and compare it
            // with our picture.
            let expected_rgba = {
                let mut path = images_dir.join(test_image);
                path.set_extension("rgba");
                fs::read(path).unwrap()
            };

            assert_eq!(graphics.pixels, expected_rgba);
            assert!(graphics.is_opaque);
        }
    }
}
