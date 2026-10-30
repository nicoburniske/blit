use std::mem::size_of;

use blit_cache::{DeferredCache, Scale};
use blit_gui::{GlyphImageId, ResolvedTextLayout, TextSystem};
use blit_text::{GlyphImage, GlyphMetrics, GlyphPixels};

pub struct CachedGlyph {
    pub metrics: GlyphMetrics,
    pub pixels: GlyphPixels,
}

pub struct GlyphCache {
    glyphs: DeferredCache<GlyphImageId, CachedGlyph, GlyphScale>,
}

struct GlyphScale;

impl Scale<GlyphImageId, CachedGlyph> for GlyphScale {
    fn weight(&self, _key: &GlyphImageId, glyph: &CachedGlyph) -> usize {
        size_of::<CachedGlyph>() + glyph.pixels.data().len()
    }
}

impl GlyphCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            glyphs: DeferredCache::new(GlyphScale, capacity),
        }
    }

    pub fn glyph(&mut self, text: &mut ResolvedTextLayout<'_>, id: GlyphImageId) -> Option<usize> {
        if let Some((_, index)) = self.glyphs.get(&id) {
            return Some(index);
        }
        let Ok(GlyphImage { metrics, pixels }) = text.rasterize_glyph(id) else {
            text.remove_glyph(id);
            return None;
        };
        Some(
            self.glyphs
                .get_or_insert(id, |key| (key, CachedGlyph { metrics, pixels }))
                .1,
        )
    }

    pub fn get(&self, index: usize) -> &CachedGlyph {
        self.glyphs.get_index(index)
    }

    pub fn finish_frame(&mut self, text: &mut TextSystem) {
        self.glyphs.trim_to_weight_if(|id, _| {
            text.remove_glyph(*id);
            true
        });
    }
}
