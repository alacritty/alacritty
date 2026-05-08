//! Resolve font faces and register them as egui font families.
//!
//! On Unix we drive libfontconfig directly with the same pattern flow as
//! alacritty's `crossfont` backend (see `crossfont::ft::FreeTypeRasterizer::
//! get_face`):
//!
//!   1. Build a pattern with `family`, `pixelsize`, `weight`, `slant`.
//!   2. `FcConfigSubstitute(MatchKind::Pattern)` — applies `<alias>` and
//!      `<prefer>` rules to the pattern itself.
//!   3. `FcDefaultSubstitute()` — fills in remaining defaults.
//!   4. `FcFontMatch()` — returns the closest installed face.
//!
//! That's how alacritty resolves an alias like `terminal-mono-font` into a
//! real family and then picks the bold/italic variant.  Going through
//! `fc-match` on the command line gives different results because the CLI
//! parser treats `family:weight=bold` differently than building the pattern
//! programmatically.
//!
//! Four faces are loaded so that ANSI bold / italic cells use real Bold or
//! Italic glyphs.  Variants are registered under named families:
//!   - `FontFamily::Monospace` and `FontFamily::Proportional` — normal face
//!   - `BOLD_FAMILY` / `ITALIC_FAMILY` / `BOLD_ITALIC_FAMILY` — variant faces
//! If a variant resolves to the same file as the normal face we register the
//! normal face under that name as a fallback so painting code can always
//! reference the named family.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Context, FontData, FontDefinitions, FontFamily};

use crate::config::FontFace;

pub const BOLD_FAMILY: &str = "alacritree_bold";
pub const ITALIC_FAMILY: &str = "alacritree_italic";
pub const BOLD_ITALIC_FAMILY: &str = "alacritree_bold_italic";

const NORMAL_FONT_ID: &str = "alacritree_terminal_normal";
const BOLD_FONT_ID: &str = "alacritree_terminal_bold";
const ITALIC_FONT_ID: &str = "alacritree_terminal_italic";
const BOLD_ITALIC_FONT_ID: &str = "alacritree_terminal_bold_italic";

#[derive(Clone, Copy)]
enum Variant {
    Normal,
    Bold,
    Italic,
    BoldItalic,
}

impl Variant {
    fn label(self) -> &'static str {
        match self {
            Variant::Normal => "regular",
            Variant::Bold => "bold",
            Variant::Italic => "italic",
            Variant::BoldItalic => "bold italic",
        }
    }
}

pub fn install_terminal_fonts(
    ctx: &Context,
    normal: &FontFace,
    bold: &FontFace,
    italic: &FontFace,
    bold_italic: &FontFace,
) {
    let Some(family) = normal.family.as_deref() else {
        return;
    };

    // The normal face's path is what the variant lookups compare against
    // when checking whether a real bold/italic file was found — if a variant
    // resolves to the same file as normal it means fontconfig had no real
    // variant available and substituted the regular face.
    let normal_match = match resolve_face(family, normal.style.as_deref(), Variant::Normal) {
        Some(m) => m,
        None => {
            log::warn!("could not resolve font '{family}'; using bundled monospace");
            return;
        },
    };
    let normal_bytes = match std::fs::read(&normal_match.path) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("could not read font file {}: {e}", normal_match.path.display());
            return;
        },
    };

    // Bold / italic / bold-italic fall back to the normal family if the user
    // didn't override them, matching alacritty's behavior.
    let bold_family = bold.family.as_deref().unwrap_or(family);
    let italic_family = italic.family.as_deref().unwrap_or(family);
    let bold_italic_family = bold_italic.family.as_deref().unwrap_or(family);

    let bold_bytes = load_variant(
        bold_family,
        bold.style.as_deref(),
        Variant::Bold,
        &normal_match.path,
    );
    let italic_bytes = load_variant(
        italic_family,
        italic.style.as_deref(),
        Variant::Italic,
        &normal_match.path,
    );
    let bold_italic_bytes = load_variant(
        bold_italic_family,
        bold_italic.style.as_deref(),
        Variant::BoldItalic,
        &normal_match.path,
    );

    let mut defs = FontDefinitions::default();

    insert_face(&mut defs, NORMAL_FONT_ID, normal_bytes.clone());
    register_default_family(&mut defs, FontFamily::Monospace, NORMAL_FONT_ID);
    register_default_family(&mut defs, FontFamily::Proportional, NORMAL_FONT_ID);

    register_variant(&mut defs, BOLD_FONT_ID, BOLD_FAMILY, bold_bytes, &normal_bytes);
    register_variant(&mut defs, ITALIC_FONT_ID, ITALIC_FAMILY, italic_bytes, &normal_bytes);
    register_variant(
        &mut defs,
        BOLD_ITALIC_FONT_ID,
        BOLD_ITALIC_FAMILY,
        bold_italic_bytes,
        &normal_bytes,
    );

    ctx.set_fonts(defs);
}

fn insert_face(defs: &mut FontDefinitions, id: &str, bytes: Vec<u8>) {
    defs.font_data.insert(id.to_string(), Arc::new(FontData::from_owned(bytes)));
}

fn register_default_family(defs: &mut FontDefinitions, family: FontFamily, id: &str) {
    defs.families.entry(family).or_default().insert(0, id.to_string());
}

fn register_variant(
    defs: &mut FontDefinitions,
    font_id: &str,
    family_name: &str,
    bytes: Option<Vec<u8>>,
    fallback: &[u8],
) {
    let bytes = bytes.unwrap_or_else(|| fallback.to_vec());
    insert_face(defs, font_id, bytes);
    defs.families
        .insert(FontFamily::Name(family_name.into()), vec![font_id.to_string()]);
}

/// Returns the bytes of the variant face if a *real* variant exists, or
/// `None` if the matcher fell back to the normal file.  The caller registers
/// the normal face as a fallback under the variant's family name.
fn load_variant(
    family: &str,
    style: Option<&str>,
    variant: Variant,
    normal_path: &Path,
) -> Option<Vec<u8>> {
    let resolved = resolve_face(family, style, variant)?;
    if resolved.path == normal_path {
        log::debug!(
            "no real {} face for '{family}'; cells with that style will use the regular face",
            variant.label()
        );
        return None;
    }
    match std::fs::read(&resolved.path) {
        Ok(b) => Some(b),
        Err(e) => {
            log::warn!(
                "could not read {} font file {}: {e}",
                variant.label(),
                resolved.path.display()
            );
            None
        },
    }
}

struct ResolvedFace {
    path: PathBuf,
}

#[cfg(unix)]
fn resolve_face(family_or_path: &str, style: Option<&str>, variant: Variant) -> Option<ResolvedFace> {
    if let Some(face) = resolve_via_path(family_or_path) {
        return Some(face);
    }
    if let Some(face) = fontconfig_resolve::resolve(family_or_path, style, variant) {
        return Some(face);
    }
    // Fall through to fontdb only if libfontconfig isn't available at runtime;
    // it doesn't apply <alias> rules, so the fontconfig path is preferred.
    resolve_via_fontdb(family_or_path, variant)
}

#[cfg(not(unix))]
fn resolve_face(family_or_path: &str, _style: Option<&str>, variant: Variant) -> Option<ResolvedFace> {
    if let Some(face) = resolve_via_path(family_or_path) {
        return Some(face);
    }
    resolve_via_fontdb(family_or_path, variant)
}

fn resolve_via_path(family_or_path: &str) -> Option<ResolvedFace> {
    let path = Path::new(family_or_path);
    if path.is_file() {
        return Some(ResolvedFace { path: path.to_path_buf() });
    }
    None
}

fn resolve_via_fontdb(family: &str, variant: Variant) -> Option<ResolvedFace> {
    let (weight, style) = match variant {
        Variant::Normal => (fontdb::Weight::NORMAL, fontdb::Style::Normal),
        Variant::Bold => (fontdb::Weight::BOLD, fontdb::Style::Normal),
        Variant::Italic => (fontdb::Weight::NORMAL, fontdb::Style::Italic),
        Variant::BoldItalic => (fontdb::Weight::BOLD, fontdb::Style::Italic),
    };
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        weight,
        stretch: fontdb::Stretch::Normal,
        style,
    };
    let face_id = db.query(&query)?;
    let face_info = db.face(face_id)?;
    match &face_info.source {
        fontdb::Source::File(path) => Some(ResolvedFace { path: path.clone() }),
        // fontdb can also return embedded data; egui needs a path-or-bytes
        // pair from us, so for simplicity we only support file-backed faces
        // through this path (rare on Unix).
        fontdb::Source::Binary(_) | fontdb::Source::SharedFile(_, _) => None,
    }
}

#[cfg(unix)]
mod fontconfig_resolve {
    //! Pattern construction mirrors `crossfont::ft::FreeTypeRasterizer::get_face`:
    //! we add family + weight + slant, run `config_substitute(Pattern)` to
    //! expand fontconfig `<alias>`/`<prefer>` rules, then `default_substitute`
    //! and `font_match` to pick the closest installed face.

    use std::ffi::CString;
    use std::path::PathBuf;

    use fontconfig::{
        FC_FAMILY, FC_SLANT, FC_SLANT_ITALIC, FC_SLANT_ROMAN, FC_STYLE, FC_WEIGHT,
        FC_WEIGHT_BOLD, FC_WEIGHT_REGULAR, Fontconfig, Pattern,
    };

    use super::{ResolvedFace, Variant};

    pub fn resolve(family: &str, style: Option<&str>, variant: Variant) -> Option<ResolvedFace> {
        let fc = Fontconfig::new()?;
        let mut pattern = Pattern::new(&fc);

        let family_c = CString::new(family).ok()?;
        pattern.add_string(FC_FAMILY, &family_c);

        // Style hint, if the user provided one (e.g. "Bold", "Italic").  We add
        // it on top of weight+slant; FcConfigSubstitute resolves any conflict
        // by preferring the integer attributes during matching.
        if let Some(style) = style {
            if let Ok(style_c) = CString::new(style) {
                pattern.add_string(FC_STYLE, &style_c);
            }
        }

        let (weight, slant) = match variant {
            Variant::Normal => (FC_WEIGHT_REGULAR, FC_SLANT_ROMAN),
            Variant::Bold => (FC_WEIGHT_BOLD, FC_SLANT_ROMAN),
            Variant::Italic => (FC_WEIGHT_REGULAR, FC_SLANT_ITALIC),
            Variant::BoldItalic => (FC_WEIGHT_BOLD, FC_SLANT_ITALIC),
        };
        pattern.add_integer(FC_WEIGHT, weight);
        pattern.add_integer(FC_SLANT, slant);

        // `font_match` internally calls FcConfigSubstitute (MatchPattern) +
        // FcDefaultSubstitute + FcFontMatch, which is exactly what
        // `crossfont::ft::FreeTypeRasterizer::get_face` does for the primary
        // face lookup.
        let matched = pattern.font_match();
        let path = matched.filename()?;
        Some(ResolvedFace { path: PathBuf::from(path) })
    }
}
