//! Resolve a font family / file and register it as egui's Monospace face.

use std::path::Path;
use std::sync::Arc;

use egui::{Context, FontData, FontDefinitions, FontFamily};

pub fn install_terminal_font(ctx: &Context, family_or_path: Option<&str>) {
    let Some(name) = family_or_path else {
        return;
    };

    let bytes = match load_font_bytes(name) {
        Some(b) => b,
        None => {
            log::warn!("could not resolve font '{name}'; using bundled monospace");
            return;
        },
    };

    let mut defs = FontDefinitions::default();
    let font_id = "alacritree_terminal";
    defs.font_data.insert(font_id.to_string(), Arc::new(FontData::from_owned(bytes)));
    let monospace = defs.families.entry(FontFamily::Monospace).or_default();
    monospace.insert(0, font_id.to_string());
    let proportional = defs.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, font_id.to_string());

    ctx.set_fonts(defs);
}

fn load_font_bytes(name: &str) -> Option<Vec<u8>> {
    let path = Path::new(name);
    if path.is_file() {
        return std::fs::read(path).ok();
    }
    if let Some(bytes) = resolve_via_fontdb(name) {
        return Some(bytes);
    }
    // fontdb only matches exact family names — shell out for fontconfig aliases.
    resolve_via_fc_match(name)
}

fn resolve_via_fc_match(name: &str) -> Option<Vec<u8>> {
    let output =
        std::process::Command::new("fc-match").arg("-f").arg("%{file}").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    std::fs::read(path).ok()
}

fn resolve_via_fontdb(family: &str) -> Option<Vec<u8>> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        style: fontdb::Style::Normal,
    };
    let face_id = db.query(&query)?;
    let face_info = db.face(face_id)?;
    match &face_info.source {
        fontdb::Source::File(path) => std::fs::read(path).ok(),
        fontdb::Source::Binary(data) | fontdb::Source::SharedFile(_, data) => {
            Some(data.as_ref().as_ref().to_vec())
        },
    }
}
