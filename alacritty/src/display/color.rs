use std::ops::{Index, IndexMut};

use log::trace;

use alacritty_terminal::ansi::NamedColor;
use alacritty_terminal::term::color::{Rgb, COUNT};

use crate::config::color::Colors;

/// Factor for automatic computation of dim colors.
pub const DIM_FACTOR: f32 = 0.66;

#[derive(Copy, Clone)]
pub struct List([Rgb; COUNT]);

impl<'a> From<&'a Colors> for List {
    fn from(colors: &Colors) -> List {
        // Type inference fails without this annotation.
        let mut list = List([Rgb::default(); COUNT]);

        list.fill_named(colors);
        list.fill_cube(colors);
        list.fill_gray_ramp(colors);

        list
    }
}

impl List {
    pub fn fill_named(&mut self, colors: &Colors) {
        // Normals.
        self[NamedColor::Black] = colors.normal.black;
        self[NamedColor::Red] = colors.normal.red;
        self[NamedColor::Green] = colors.normal.green;
        self[NamedColor::Yellow] = colors.normal.yellow;
        self[NamedColor::Blue] = colors.normal.blue;
        self[NamedColor::Magenta] = colors.normal.magenta;
        self[NamedColor::Cyan] = colors.normal.cyan;
        self[NamedColor::White] = colors.normal.white;

        // Brights.
        self[NamedColor::BrightBlack] = colors.bright.black;
        self[NamedColor::BrightRed] = colors.bright.red;
        self[NamedColor::BrightGreen] = colors.bright.green;
        self[NamedColor::BrightYellow] = colors.bright.yellow;
        self[NamedColor::BrightBlue] = colors.bright.blue;
        self[NamedColor::BrightMagenta] = colors.bright.magenta;
        self[NamedColor::BrightCyan] = colors.bright.cyan;
        self[NamedColor::BrightWhite] = colors.bright.white;
        self[NamedColor::BrightForeground] =
            colors.primary.bright_foreground.unwrap_or(colors.primary.foreground);

        // Foreground and background.
        self[NamedColor::Foreground] = colors.primary.foreground;
        self[NamedColor::Background] = colors.primary.background;

        // Dims.
        self[NamedColor::DimForeground] =
            colors.primary.dim_foreground.unwrap_or(colors.primary.foreground * DIM_FACTOR);
        match colors.dim {
            Some(ref dim) => {
                trace!("Using config-provided dim colors");
                self[NamedColor::DimBlack] = dim.black;
                self[NamedColor::DimRed] = dim.red;
                self[NamedColor::DimGreen] = dim.green;
                self[NamedColor::DimYellow] = dim.yellow;
                self[NamedColor::DimBlue] = dim.blue;
                self[NamedColor::DimMagenta] = dim.magenta;
                self[NamedColor::DimCyan] = dim.cyan;
                self[NamedColor::DimWhite] = dim.white;
            },
            None => {
                trace!("Deriving dim colors from normal colors");
                self[NamedColor::DimBlack] = colors.normal.black * DIM_FACTOR;
                self[NamedColor::DimRed] = colors.normal.red * DIM_FACTOR;
                self[NamedColor::DimGreen] = colors.normal.green * DIM_FACTOR;
                self[NamedColor::DimYellow] = colors.normal.yellow * DIM_FACTOR;
                self[NamedColor::DimBlue] = colors.normal.blue * DIM_FACTOR;
                self[NamedColor::DimMagenta] = colors.normal.magenta * DIM_FACTOR;
                self[NamedColor::DimCyan] = colors.normal.cyan * DIM_FACTOR;
                self[NamedColor::DimWhite] = colors.normal.white * DIM_FACTOR;
            },
        }
    }

    pub fn fill_cube(&mut self, colors: &Colors) {
        let mut index: usize = 16;
        // Build colors.
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    // Override colors 16..232 with the config (if present).
                    if let Some(indexed_color) =
                        colors.indexed_colors.iter().find(|ic| ic.index() == index as u8)
                    {
                        self[index] = indexed_color.color;
                    } else {
                        self[index] = Rgb::new(
                            if r == 0 { 0 } else { r * 40 + 55 },
                            if g == 0 { 0 } else { g * 40 + 55 },
                            if b == 0 { 0 } else { b * 40 + 55 },
                        );
                    }
                    index += 1;
                }
            }
        }

        debug_assert!(index == 232);
    }

    pub fn fill_gray_ramp(&mut self, colors: &Colors) {
        let mut index: usize = 232;

        for i in 0..24 {
            // Index of the color is number of named colors + number of cube colors + i.
            let color_index = 16 + 216 + i;

            // Override colors 232..256 with the config (if present).
            if let Some(indexed_color) =
                colors.indexed_colors.iter().find(|ic| ic.index() == color_index)
            {
                self[index] = indexed_color.color;
                index += 1;
                continue;
            }

            let value = i * 10 + 8;
            self[index] = Rgb::new(value, value, value);
            index += 1;
        }

        debug_assert!(index == 256);
    }
}

impl Index<usize> for List {
    type Output = Rgb;

    #[inline]
    fn index(&self, idx: usize) -> &Self::Output {
        &self.0[idx]
    }
}

impl IndexMut<usize> for List {
    #[inline]
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.0[idx]
    }
}

impl Index<NamedColor> for List {
    type Output = Rgb;

    #[inline]
    fn index(&self, idx: NamedColor) -> &Self::Output {
        &self.0[idx as usize]
    }
}

impl IndexMut<NamedColor> for List {
    #[inline]
    fn index_mut(&mut self, idx: NamedColor) -> &mut Self::Output {
        &mut self.0[idx as usize]
    }
}
