//! Resolve font faces and register them as egui font families.
//!
//! Four faces are loaded so ANSI bold/italic cells use real Bold/Italic
//! glyphs.  On Unix we go through libfontconfig directly (same pattern flow
//! as `crossfont::ft::FreeTypeRasterizer::get_face`) — `fc-match` on the CLI
//! mishandles `family:weight=bold` patterns when the family is an `<alias>`,
//! so building the pattern programmatically is what makes weight/slant pick
//! the real variant for aliased families.

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

    // The variant lookups compare their resolved path against this one to
    // detect when fontconfig substituted the regular face for a missing variant.
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

    // Bold/italic/bold-italic inherit the normal family unless overridden.
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
    // fontdb fallback for the case where libfontconfig isn't available; it
    // doesn't expand <alias> rules, so it's strictly second-best on Unix.
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
        // Embedded faces aren't path-addressable; we'd have to re-architect
        // the loader to support them and they're rare on Unix.
        fontdb::Source::Binary(_) | fontdb::Source::SharedFile(_, _) => None,
    }
}

#[cfg(unix)]
mod fontconfig_resolve {
    //! Mirrors `crossfont::ft::FreeTypeRasterizer::get_face`: build a pattern
    //! with family + weight + slant and let `font_match` run substitution.
    //! Doing this in code (vs `fc-match` CLI) is what makes `<alias>` rules
    //! plus weight/slant pick the right variant.

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

        let matched = pattern.font_match();
        let path = matched.filename()?;
        Some(ResolvedFace { path: PathBuf::from(path) })
    }
}
