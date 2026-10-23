use std::mem::size_of;

use blit_cache::{DeferredCache, Scale};
use blit_gui::ResolvedTextLayout;
use blit_text::{FontFaceId, GlyphMask, GlyphMetrics};

pub struct CachedGlyph {
    pub metrics: GlyphMetrics,
    pub alpha: Box<[u8]>,
}

pub struct GlyphCache {
    glyphs: DeferredCache<GlyphKey, CachedGlyph, GlyphScale>,
    phase_scale: f32,
}

struct GlyphScale;

impl Scale<GlyphKey, CachedGlyph> for GlyphScale {
    fn weight(&self, _key: &GlyphKey, glyph: &CachedGlyph) -> usize {
        size_of::<CachedGlyph>() + glyph.alpha.len()
    }
}

impl GlyphCache {
    pub fn new(capacity: usize, phase_scale: f32) -> Self {
        Self {
            glyphs: DeferredCache::new(GlyphScale, capacity),
            phase_scale,
        }
    }

    pub fn glyph(
        &mut self,
        text: &mut ResolvedTextLayout<'_>,
        face: FontFaceId,
        glyph: u16,
        size: u32,
        phase: u8,
    ) -> usize {
        let key = GlyphKey {
            face,
            glyph,
            size,
            phase,
        };
        let Self { glyphs, phase_scale } = self;
        let (_, index) = glyphs.get_or_insert(key, |key| {
            let GlyphMask { metrics, alpha } = text.rasterize_glyph(
                key.face,
                key.glyph,
                f32::from_bits(key.size),
                key.phase as f32 * *phase_scale,
            );
            (key, CachedGlyph { metrics, alpha })
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
    face: FontFaceId,
    glyph: u16,
    size: u32,
    phase: u8,
}
