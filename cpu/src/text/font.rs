use std::{cmp::Reverse, mem::size_of};

use blit::paint::FontId;
use blit_font::{GlyphRasterConfig, Metrics, Rasterizer};

use crate::{cache::Cache, Font, FontFace};

pub struct CachedGlyph {
    pub metrics: Metrics,
    pub alpha: Box<[u8]>,
}

pub struct FontCache {
    faces: Vec<FontFace>,
    glyphs: Cache<GlyphKey, CachedGlyph>,
    rasterizer: Rasterizer,
}

impl FontCache {
    pub fn new(faces: Vec<FontFace>, capacity: usize) -> Self {
        assert!(!faces.is_empty());
        Self { faces, glyphs: Cache::new(capacity), rasterizer: Rasterizer::default() }
    }

    pub fn font(&self, id: FontId, weight: u16) -> Option<(usize, &Font)> {
        self.faces
            .iter()
            .enumerate()
            .filter(|(_, face)| face.id == id)
            .min_by_key(|(_, face)| (face.weight.abs_diff(weight), Reverse(face.weight)))
            .map(|(index, face)| (index, &face.font))
    }

    pub fn glyph(&mut self, face: usize, glyph: GlyphRasterConfig) -> Result<&CachedGlyph, CachedGlyph> {
        let key = GlyphKey { face, glyph };
        let Self { faces, glyphs, rasterizer } = self;
        glyphs.get_or_insert_with(key, || {
            let font = &faces[key.face].font;
            let (metrics, alpha) = rasterizer.rasterize(font, key.glyph.glyph_id, key.glyph.size);
            let glyph = CachedGlyph { metrics, alpha: alpha.into_boxed_slice() };
            let weight = size_of::<CachedGlyph>() + glyph.alpha.len();
            (glyph, weight)
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    face: usize,
    glyph: GlyphRasterConfig,
}
