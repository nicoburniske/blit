use std::{collections::hash_map::RandomState, num::NonZeroUsize};

use clru::{CLruCache, CLruCacheConfig, WeightScale};
use fontdue::{Font, Metrics, layout::GlyphRasterConfig};

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
    font: Font,
    glyphs: GlyphCache,
}

impl FontCache {
    pub fn new(font: Font, capacity: usize) -> Self {
        Self {
            font,
            glyphs: CLruCache::with_config(
                CLruCacheConfig::new(
                    NonZeroUsize::new(capacity).expect("glyph cache capacity must be non-zero"),
                )
                .with_scale(GlyphWeight),
            ),
        }
    }

    pub fn font(&self) -> &Font {
        &self.font
    }

    pub fn glyph(&mut self, key: GlyphRasterConfig) -> Result<&CachedGlyph, CachedGlyph> {
        if self.glyphs.peek(&key).is_some() {
            return Ok(self.glyphs.get(&key).unwrap());
        }
        let (metrics, alpha) = self.font.rasterize_config(key);
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
