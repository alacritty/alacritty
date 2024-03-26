use std::collections::hash_map::Entry;
use std::collections::HashMap;

use ahash::RandomState;
use crossfont::{
    Error as RasterizerError, FontDesc, FontKey, GlyphKey, Metrics, Rasterize, RasterizedGlyph,
    Rasterizer, Size, Slant, Style, Weight,
};
use log::{error, info};
use unicode_width::UnicodeWidthChar;

use crate::config::font::{Font, FontDescription};
use crate::config::ui_config::Delta;
use crate::gl::types::*;

use super::builtin_font;

/// `LoadGlyph` allows for copying a rasterized glyph into graphics memory.
pub trait LoadGlyph {
    /// Load the rasterized glyph into GPU memory.
    fn load_glyph(&mut self, rasterized: &RasterizedGlyph) -> Glyph;

    /// Clear any state accumulated from previous loaded glyphs.
    ///
    /// This can, for instance, be used to reset the texture Atlas.
    fn clear(&mut self);
}

#[derive(Copy, Clone, Debug)]
pub struct Glyph {
    pub tex_id: GLuint,
    pub multicolor: bool,
    pub top: i16,
    pub left: i16,
    pub width: i16,
    pub height: i16,
    pub uv_bot: f32,
    pub uv_left: f32,
    pub uv_width: f32,
    pub uv_height: f32,
}

/// Naïve glyph cache.
///
/// Currently only keyed by `char`, and thus not possible to hold different
/// representations of the same code point.
pub struct GlyphCache {
    /// Cache of buffered glyphs.
    cache: HashMap<GlyphKey, Glyph, RandomState>,

    /// Rasterizer for loading new glyphs.
    rasterizer: Rasterizer,

    /// Regular font.
    pub font_key: FontKey,

    /// Bold font.
    pub bold_key: FontKey,

    /// Italic font.
    pub italic_key: FontKey,

    /// Bold italic font.
    pub bold_italic_key: FontKey,

    /// Font size.
    pub font_size: crossfont::Size,

    /// Font offset.
    font_offset: Delta<i8>,

    /// Glyph offset.
    glyph_offset: Delta<i8>,

    /// Font metrics.
    metrics: Metrics,

    /// Whether to use the built-in font for box drawing characters.
    builtin_box_drawing: bool,
}

impl GlyphCache {
    pub fn new(mut rasterizer: Rasterizer, font: &Font) -> Result<GlyphCache, crossfont::Error> {
        let (regular, bold, italic, bold_italic) = Self::compute_font_keys(font, &mut rasterizer)?;

        // Need to load at least one glyph for the face before calling metrics.
        // The glyph requested here ('m' at the time of writing) has no special
        // meaning.
        rasterizer.get_glyph(GlyphKey { font_key: regular, character: 'm', size: font.size() })?;

        let metrics = rasterizer.metrics(regular, font.size())?;

        Ok(Self {
            cache: Default::default(),
            rasterizer,
            font_size: font.size(),
            font_key: regular,
            bold_key: bold,
            italic_key: italic,
            bold_italic_key: bold_italic,
            font_offset: font.offset,
            glyph_offset: font.glyph_offset,
            metrics,
            builtin_box_drawing: font.builtin_box_drawing,
        })
    }

    fn load_glyphs_for_font<L: LoadGlyph>(&mut self, font: FontKey, loader: &mut L) {
        let size = self.font_size;

        // Cache all ascii characters.
        for i in 32u8..=126u8 {
            let glyph_key = GlyphKey { font_key: font, character: i as char, size };
            let missing_glyph = match self.get(glyph_key, loader).map_err(|err| *err) {
                Ok(_) => continue,
                Err(RasterizerError::MissingGlyph(glyph)) => Some(glyph),
                _ => None,
            };

            let _ = self.insert_missing_or_default(glyph_key, loader, missing_glyph);
        }
    }

    /// Computes font keys for (Regular, Bold, Italic, Bold Italic).
    fn compute_font_keys(
        font: &Font,
        rasterizer: &mut Rasterizer,
    ) -> Result<(FontKey, FontKey, FontKey, FontKey), crossfont::Error> {
        let size = font.size();

        // Load regular font.
        let regular_desc = Self::make_desc(font.normal(), Slant::Normal, Weight::Normal);

        let regular = Self::load_regular_font(rasterizer, &regular_desc, size)?;

        // Helper to load a description if it is not the `regular_desc`.
        let mut load_or_regular = |desc: FontDesc| {
            if desc == regular_desc {
                regular
            } else {
                rasterizer.load_font(&desc, size).unwrap_or(regular)
            }
        };

        // Load bold font.
        let bold_desc = Self::make_desc(&font.bold(), Slant::Normal, Weight::Bold);

        let bold = load_or_regular(bold_desc);

        // Load italic font.
        let italic_desc = Self::make_desc(&font.italic(), Slant::Italic, Weight::Normal);

        let italic = load_or_regular(italic_desc);

        // Load bold italic font.
        let bold_italic_desc = Self::make_desc(&font.bold_italic(), Slant::Italic, Weight::Bold);

        let bold_italic = load_or_regular(bold_italic_desc);

        Ok((regular, bold, italic, bold_italic))
    }

    fn load_regular_font(
        rasterizer: &mut Rasterizer,
        description: &FontDesc,
        size: Size,
    ) -> Result<FontKey, crossfont::Error> {
        match rasterizer.load_font(description, size) {
            Ok(font) => Ok(font),
            Err(err) => {
                error!("{}", err);

                let fallback_desc =
                    Self::make_desc(Font::default().normal(), Slant::Normal, Weight::Normal);
                rasterizer.load_font(&fallback_desc, size)
            },
        }
    }

    fn make_desc(desc: &FontDescription, slant: Slant, weight: Weight) -> FontDesc {
        let style = if let Some(ref spec) = desc.style {
            Style::Specific(spec.to_owned())
        } else {
            Style::Description { slant, weight }
        };
        FontDesc::new(desc.family.clone(), style)
    }

    /// Get a glyph from the font.
    ///
    /// If the glyph has never been loaded before, it will be rasterized and inserted into the
    /// cache.
    ///
    /// # Errors
    ///
    /// This will fail when the glyph could not be rasterized. Usually this is due to the glyph
    /// not being present in any font.
    pub fn get<L: ?Sized>(
        &mut self,
        glyph_key: GlyphKey,
        loader: &mut L,
    ) -> Result<&Glyph, Box<RasterizerError>>
    where
        L: LoadGlyph,
    {
        // Try to load glyph from cache.
        match self.cache.entry(glyph_key) {
            Entry::Occupied(glyph) => Ok(glyph.into_mut()),
            Entry::Vacant(vacant) => {
                // // Rasterize the glyph using the built-in font for special characters or the
                // user's font // for everything else.
                let rasterized = self
                    .builtin_box_drawing
                    .then(|| {
                        builtin_font::builtin_glyph(
                            glyph_key.character,
                            &self.metrics,
                            &self.font_offset,
                            &self.glyph_offset,
                        )
                    })
                    .flatten()
                    .map_or_else(|| self.rasterizer.get_glyph(glyph_key), Ok)?;

                let glyph = Self::load_glyph(loader, rasterized, &self.glyph_offset, &self.metrics);

                // Cache rasterized glyph.
                Ok(vacant.insert(glyph))
            },
        }
    }

    /// Insert the `missing_glyph` for the given `glyph_key` or the default glyph.
    pub fn insert_missing_or_default<L: ?Sized>(
        &mut self,
        glyph_key: GlyphKey,
        loader: &mut L,
        missing_glyph: Option<RasterizedGlyph>,
    ) -> &Glyph
    where
        L: LoadGlyph,
    {
        let to_insert = match missing_glyph {
            Some(missing_glyph) => {
                let missing_key = GlyphKey { character: '\0', ..glyph_key };
                *match self.cache.entry(missing_key) {
                    Entry::Occupied(glyph) => glyph.into_mut(),
                    Entry::Vacant(glyph) => glyph.insert(Self::load_glyph(
                        loader,
                        missing_glyph,
                        &self.glyph_offset,
                        &self.metrics,
                    )),
                }
            },
            None => Self::load_glyph(loader, Default::default(), &self.glyph_offset, &self.metrics),
        };

        match self.cache.entry(glyph_key) {
            Entry::Occupied(glyph) => glyph.into_mut(),
            Entry::Vacant(glyph) => glyph.insert(to_insert),
        }
    }

    /// Load glyph into the atlas.
    ///
    /// This will apply all transforms defined for the glyph cache to the rasterized glyph before
    pub fn load_glyph<L: ?Sized>(
        loader: &mut L,
        mut glyph: RasterizedGlyph,
        glyph_offset: &Delta<i8>,
        metrics: &Metrics,
    ) -> Glyph
    where
        L: LoadGlyph,
    {
        glyph.left += i32::from(glyph_offset.x);
        glyph.top += i32::from(glyph_offset.y);
        glyph.top -= metrics.descent as i32;

        // The metrics of zero-width characters are based on rendering
        // the character after the current cell, with the anchor at the
        // right side of the preceding character. Since we render the
        // zero-width characters inside the preceding character, the
        // anchor has been moved to the right by one cell.
        if glyph.character.width() == Some(0) {
            glyph.left += metrics.average_advance as i32;
        }

        // Add glyph to cache.
        loader.load_glyph(&glyph)
    }

    /// Reset currently cached data in both GL and the registry to default state.
    pub fn reset_glyph_cache<L: LoadGlyph>(&mut self, loader: &mut L) {
        loader.clear();
        self.cache = Default::default();

        self.load_common_glyphs(loader);
    }

    /// Update the inner font size.
    ///
    /// NOTE: To reload the renderers's fonts [`Self::reset_glyph_cache`] should be called
    /// afterwards.
    pub fn update_font_size(&mut self, font: &Font) -> Result<(), crossfont::Error> {
        // Update dpi scaling.
        self.font_offset = font.offset;
        self.glyph_offset = font.glyph_offset;

        // Recompute font keys.
        let (regular, bold, italic, bold_italic) =
            Self::compute_font_keys(font, &mut self.rasterizer)?;

        self.rasterizer.get_glyph(GlyphKey {
            font_key: regular,
            character: 'm',
            size: font.size(),
        })?;
        let metrics = self.rasterizer.metrics(regular, font.size())?;

        info!("Font size changed to {:?} px", font.size().as_px());

        self.font_size = font.size();
        self.font_key = regular;
        self.bold_key = bold;
        self.italic_key = italic;
        self.bold_italic_key = bold_italic;
        self.metrics = metrics;
        self.builtin_box_drawing = font.builtin_box_drawing;

        Ok(())
    }

    pub fn font_metrics(&self) -> crossfont::Metrics {
        self.metrics
    }

    /// Prefetch glyphs that are almost guaranteed to be loaded anyways.
    pub fn load_common_glyphs<L: LoadGlyph>(&mut self, loader: &mut L) {
        self.load_glyphs_for_font(self.font_key, loader);
        self.load_glyphs_for_font(self.bold_key, loader);
        self.load_glyphs_for_font(self.italic_key, loader);
        self.load_glyphs_for_font(self.bold_italic_key, loader);
    }
}
