use std::{cmp::Reverse, mem::size_of};

use blit::text::FontId;
use blit_cache::{DeferredCache, Scale};
use blit_font::{GlyphRasterConfig, Metrics, Rasterizer};

use crate::{Font, FontFace};

pub struct CachedGlyph {
    pub metrics: Metrics,
    pub alpha: Box<[u8]>,
}

pub struct FontCache {
    faces: Vec<FontFace>,
    glyphs: DeferredCache<GlyphKey, CachedGlyph, GlyphScale>,
    rasterizer: Rasterizer,
}

struct GlyphScale;

impl Scale<GlyphKey, CachedGlyph> for GlyphScale {
    fn weight(&self, _key: &GlyphKey, glyph: &CachedGlyph) -> usize {
        size_of::<CachedGlyph>() + glyph.alpha.len()
    }
}

impl FontCache {
    pub fn new(faces: Vec<FontFace>, capacity: usize) -> Self {
        assert!(!faces.is_empty());
        Self {
            faces,
            glyphs: DeferredCache::new(GlyphScale, capacity),
            rasterizer: Rasterizer::default(),
        }
    }

    pub fn font(&self, id: FontId, weight: u16) -> Option<(usize, &Font)> {
        self.faces
            .iter()
            .enumerate()
            .filter(|(_, face)| face.id == id)
            .min_by_key(|(_, face)| (face.weight.abs_diff(weight), Reverse(face.weight)))
            .map(|(index, face)| (index, &face.font))
    }

    pub fn get_font(&self, face: usize) -> &Font {
        &self.faces[face].font
    }

    pub fn glyph(&mut self, face: usize, glyph: GlyphRasterConfig) -> usize {
        let key = GlyphKey { face, glyph };
        let Self {
            faces,
            glyphs,
            rasterizer,
        } = self;
        let (_, index) = glyphs.get_or_insert(key, || {
            let font = &faces[key.face].font;
            let (metrics, alpha) = rasterizer.rasterize(font, key.glyph.glyph_id, key.glyph.size);
            CachedGlyph {
                metrics,
                alpha: alpha.into_boxed_slice(),
            }
        });
        index
    }

    pub fn get(&self, index: usize) -> &CachedGlyph {
        self.glyphs.get_index(index)
    }

    pub fn finish_frame(&mut self) {
        self.glyphs.trim_to_weight();
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    face: usize,
    glyph: GlyphRasterConfig,
}
