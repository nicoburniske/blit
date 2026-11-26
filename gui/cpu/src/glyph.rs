use core::mem::size_of;

use blit_gui::text::system::{GlyphImageId, ResolvedTextLayout, TextSystem};
use blit_text::{GlyphImage, GlyphMetrics, GlyphPixels};
use blit_util::cache::{DeferredCache, Scale};

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

    pub fn glyph(&mut self, text: &mut ResolvedTextLayout<'_>, id: GlyphImageId) -> Option<&CachedGlyph> {
        self.glyphs
            .try_get_or_insert(id, |key| {
                let Ok(GlyphImage { metrics, pixels }) = text.rasterize_glyph(key) else {
                    text.remove_glyph(key);
                    return None;
                };
                Some((key, CachedGlyph { metrics, pixels }))
            })
            .map(|(glyph, _)| glyph)
    }

    pub fn finish_frame(&mut self, text: &mut TextSystem) {
        self.glyphs.trim_to_weight_if(|id, _| {
            text.remove_glyph(*id);
            true
        });
    }
}
