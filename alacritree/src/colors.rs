//! Resolve ANSI / 256-color values against the runtime palette (OSC 4) → user
//! config → built-in defaults, in that order.

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

pub fn resolve(
    color: Color,
    flags: Flags,
    runtime: &Colors,
    palette: &Palette,
    is_fg: bool,
) -> Rgb {
    // Mirrors alacritty's `RenderableCellContent::compute_fg_rgb` for `is_fg`
    // and `compute_bg_rgb` otherwise.  Background paths skip DIM/BOLD swaps
    // entirely; that matches alacritty (only the glyph color dims).
    if !is_fg {
        return match color {
            Color::Spec(rgb) => rgb,
            Color::Indexed(i) => resolve_indexed(i, runtime, palette),
            Color::Named(named) => resolve_named_raw(named, runtime, palette),
        };
    }

    match color {
        Color::Spec(rgb) => {
            if flags.contains(Flags::DIM) {
                apply_dim(rgb)
            } else {
                rgb
            }
        },
        Color::Indexed(idx) => resolve_indexed_fg(idx, flags, runtime, palette),
        Color::Named(named) => resolve_named_fg(named, flags, runtime, palette),
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

/// Foreground variant of indexed lookup that honors BOLD-as-bright and DIM,
/// per alacritty's `compute_fg_rgb`:
///   - bold-as-bright + BOLD + 0..=7 → idx + 8 (promote to bright)
///   - DIM + 8..=15                  → idx - 8 (downgrade bright to normal)
///   - DIM + 0..=7                   → DimBlack + idx (one of the eight Dim*)
fn resolve_indexed_fg(idx: u8, flags: Flags, runtime: &Colors, palette: &Palette) -> Rgb {
    let dim_bold = flags & (Flags::DIM | Flags::BOLD);
    let promote_bright = palette.draw_bold_with_bright && dim_bold == Flags::BOLD && idx < 8;
    let dim_to_normal = !palette.draw_bold_with_bright && dim_bold == Flags::DIM && (8..=15).contains(&idx);
    let dim_to_dim_named =
        !palette.draw_bold_with_bright && dim_bold == Flags::DIM && idx < 8;

    if promote_bright {
        return resolve_indexed(idx + 8, runtime, palette);
    }
    if dim_to_normal {
        return resolve_indexed(idx - 8, runtime, palette);
    }
    if dim_to_dim_named {
        // Dim variants live in NamedColor::DimBlack..=DimWhite; resolve via
        // the runtime palette first, falling back to dim_rgb of the normal
        // ANSI color when [colors.dim] isn't configured.
        let dim_named = match idx {
            0 => NamedColor::DimBlack,
            1 => NamedColor::DimRed,
            2 => NamedColor::DimGreen,
            3 => NamedColor::DimYellow,
            4 => NamedColor::DimBlue,
            5 => NamedColor::DimMagenta,
            6 => NamedColor::DimCyan,
            _ => NamedColor::DimWhite,
        };
        return resolve_named_raw(dim_named, runtime, palette);
    }
    resolve_indexed(idx, runtime, palette)
}

fn resolve_named_raw(named: NamedColor, runtime: &Colors, palette: &Palette) -> Rgb {
    if let Some(rgb) = runtime[named] {
        return rgb;
    }
    palette_named(named, palette).unwrap_or_else(|| named_fallback(named, palette))
}

fn resolve_named_fg(
    named: NamedColor,
    flags: Flags,
    runtime: &Colors,
    palette: &Palette,
) -> Rgb {
    // Logic ported from alacritty's `compute_fg_rgb` (see
    // `alacritty/src/display/content.rs`).  Differences from the previous
    // impl: handles `DIM + Foreground` → DimForeground and the special
    // `DIM_BOLD + Foreground` case when no bright_foreground is configured.
    let dim_bold = flags & (Flags::DIM | Flags::BOLD);
    let bold_only = dim_bold == Flags::BOLD;
    let dim_only = dim_bold == Flags::DIM;
    let dim_bold_combined = dim_bold == (Flags::DIM | Flags::BOLD);

    let promoted = if dim_bold_combined
        && named == NamedColor::Foreground
        && palette.bright_fg.is_none()
    {
        // DIM + BOLD on the default foreground when no bright fg is configured:
        // alacritty treats the bold as if absent and dims the foreground.
        NamedColor::DimForeground
    } else if palette.draw_bold_with_bright && bold_only && (named as usize) < 8 {
        named.to_bright()
    } else if (dim_only || (dim_bold_combined && !palette.draw_bold_with_bright))
        && (named as usize) < 8
    {
        named.to_dim()
    } else if dim_only && named == NamedColor::Foreground {
        NamedColor::DimForeground
    } else {
        named
    };

    if let Some(rgb) = runtime[promoted] {
        return rgb;
    }

    palette_named(promoted, palette).unwrap_or_else(|| named_fallback(promoted, palette))
}

fn apply_dim(c: Rgb) -> Rgb {
    // Match alacritty's DIM_FACTOR = 0.66 from src/display/color.rs.
    Rgb {
        r: (c.r as f32 * 0.66) as u8,
        g: (c.g as f32 * 0.66) as u8,
        b: (c.b as f32 * 0.66) as u8,
    }
}

fn ansi16(index: usize, palette: &Palette) -> Rgb {
    if index < 8 { palette.normal[index] } else { palette.bright[index - 8] }
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
    // Reached only for Dim* when [colors.dim] is unset — fake it by darkening.
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
    apply_dim(c)
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
