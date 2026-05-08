//! ANSI / 256-color resolution against a [`Palette`] from the user's config.
//!
//! Resolution priority (high → low):
//!   1. The terminal's runtime palette (set by the application via OSC 4)
//!   2. The user's config (`alacritty.toml` + `alacritree.toml`)
//!   3. Built-in defaults (mirroring alacritty's defaults)

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use egui::Color32;

use crate::config::Palette;

pub fn rgb_to_color32(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.r, rgb.g, rgb.b)
}

pub fn background(palette: &Palette) -> Color32 {
    rgb_to_color32(palette.bg)
}

pub fn foreground(palette: &Palette) -> Color32 {
    rgb_to_color32(palette.fg)
}

/// Resolve a `vte::ansi::Color` to an explicit RGB, applying SGR flags
/// (bold-as-bright, dim) along the way.
pub fn resolve(
    color: Color,
    flags: Flags,
    runtime: &Colors,
    palette: &Palette,
    is_fg: bool,
) -> Rgb {
    match color {
        Color::Spec(rgb) => rgb,
        Color::Indexed(i) => resolve_indexed(i, runtime, palette),
        Color::Named(named) => resolve_named(named, flags, runtime, palette, is_fg),
    }
}

fn resolve_indexed(index: u8, runtime: &Colors, palette: &Palette) -> Rgb {
    if let Some(rgb) = runtime[index as usize] {
        return rgb;
    }
    if (index as usize) < 16 {
        return ansi16(index as usize, palette);
    }
    if let Some(&(_, rgb)) = palette.indexed.iter().find(|(i, _)| *i == index) {
        return rgb;
    }
    indexed_default(index)
}

fn resolve_named(
    named: NamedColor,
    flags: Flags,
    runtime: &Colors,
    palette: &Palette,
    is_fg: bool,
) -> Rgb {
    // Bold-as-bright: only applies to foreground, only to ANSI 0..7, and only
    // when the user has opted in (alacritty's default is also off).
    let promoted = if is_fg
        && flags.contains(Flags::BOLD)
        && palette.draw_bold_with_bright
        && (named as usize) < 8
    {
        named.to_bright()
    } else if flags.contains(Flags::DIM) && (named as usize) < 8 {
        named.to_dim()
    } else {
        named
    };

    if let Some(rgb) = runtime[promoted] {
        return rgb;
    }

    palette_named(promoted, palette).unwrap_or_else(|| named_fallback(promoted, palette))
}

fn ansi16(index: usize, palette: &Palette) -> Rgb {
    if index < 8 {
        palette.normal[index]
    } else {
        palette.bright[index - 8]
    }
}

fn palette_named(named: NamedColor, palette: &Palette) -> Option<Rgb> {
    use NamedColor::*;
    let n = named as usize;
    if n < 8 {
        return Some(palette.normal[n]);
    }
    if (8..16).contains(&n) {
        return Some(palette.bright[n - 8]);
    }
    match named {
        Foreground => Some(palette.fg),
        Background => Some(palette.bg),
        Cursor => palette.cursor_bg,
        BrightForeground => palette.bright_fg.or(Some(palette.fg)),
        DimForeground => palette.dim_fg.or_else(|| Some(dim_rgb(palette.fg))),
        DimBlack => palette.dim.map(|d| d[0]),
        DimRed => palette.dim.map(|d| d[1]),
        DimGreen => palette.dim.map(|d| d[2]),
        DimYellow => palette.dim.map(|d| d[3]),
        DimBlue => palette.dim.map(|d| d[4]),
        DimMagenta => palette.dim.map(|d| d[5]),
        DimCyan => palette.dim.map(|d| d[6]),
        DimWhite => palette.dim.map(|d| d[7]),
        _ => None,
    }
}

fn named_fallback(named: NamedColor, palette: &Palette) -> Rgb {
    // Only reached for Dim* named colors when no `[colors.dim]` table is set;
    // approximate by darkening the corresponding normal color.
    use NamedColor::*;
    let normal = match named {
        DimBlack => palette.normal[0],
        DimRed => palette.normal[1],
        DimGreen => palette.normal[2],
        DimYellow => palette.normal[3],
        DimBlue => palette.normal[4],
        DimMagenta => palette.normal[5],
        DimCyan => palette.normal[6],
        DimWhite => palette.normal[7],
        _ => palette.fg,
    };
    dim_rgb(normal)
}

fn dim_rgb(c: Rgb) -> Rgb {
    // 2/3 of original — rough approximation of SGR `dim`.
    Rgb {
        r: (c.r as u16 * 2 / 3) as u8,
        g: (c.g as u16 * 2 / 3) as u8,
        b: (c.b as u16 * 2 / 3) as u8,
    }
}

fn indexed_default(index: u8) -> Rgb {
    // Standard 6×6×6 cube + grayscale ramp for indices 16..256.
    if index < 232 {
        let i = index - 16;
        let r = i / 36;
        let g = (i % 36) / 6;
        let b = i % 6;
        return Rgb { r: cube_step(r), g: cube_step(g), b: cube_step(b) };
    }
    let level = 8 + 10 * (index - 232);
    Rgb { r: level, g: level, b: level }
}

fn cube_step(x: u8) -> u8 {
    match x {
        0 => 0,
        n => 55 + n * 40,
    }
}
