use std::{collections::hash_map::RandomState, num::NonZeroUsize};

use bullseye::{FontId, FontWeight};
use clru::{CLruCache, CLruCacheConfig, WeightScale};
use fontdue::{Metrics, layout::GlyphRasterConfig};

use crate::FontFace;

pub struct CachedGlyph {
    pub metrics: Metrics,
    pub alpha: Box<[u8]>,
}

struct GlyphWeight;

impl WeightScale<GlyphRasterConfig, CachedGlyph> for GlyphWeight {
    fn weight(&self, _: &GlyphRasterConfig, glyph: &CachedGlyph) -> usize {
        glyph.alpha.len()
    }
}

type GlyphCache = CLruCache<GlyphRasterConfig, CachedGlyph, RandomState, GlyphWeight>;

pub struct FontCache {
    faces: Vec<FontFace>,
    glyphs: GlyphCache,
}

impl FontCache {
    pub fn new(faces: Vec<FontFace>, capacity: usize) -> Self {
        assert!(!faces.is_empty());
        Self {
            faces,
            glyphs: CLruCache::with_config(
                CLruCacheConfig::new(
                    NonZeroUsize::new(capacity).expect("glyph cache capacity must be non-zero"),
                )
                .with_scale(GlyphWeight),
            ),
        }
    }

    pub fn font(&self, id: FontId, weight: FontWeight) -> Option<&fontdue::Font> {
        self.faces
            .iter()
            .find(|face| face.id == id && face.weight == weight)
            .map(|face| &face.font)
    }

    pub fn glyph(&mut self, key: GlyphRasterConfig) -> Result<&CachedGlyph, CachedGlyph> {
        if self.glyphs.peek(&key).is_some() {
            return Ok(self.glyphs.get(&key).unwrap());
        }
        let font = self
            .faces
            .iter()
            .find(|face| face.font.file_hash() == key.font_hash)
            .map(|face| &face.font)
            .expect("glyph references an unregistered font");
        let (metrics, alpha) = font.rasterize_config(key);
        let glyph = CachedGlyph {
            metrics,
            alpha: alpha.into_boxed_slice(),
        };
        match self.glyphs.put_with_weight(key, glyph) {
            Ok(_) => Ok(self.glyphs.get(&key).unwrap()),
            Err((_, glyph)) => Err(glyph),
        }
    }
}
