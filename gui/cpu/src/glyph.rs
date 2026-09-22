use std::mem::size_of;

use blit_cache::{DeferredCache, Scale};
use blit_gui::ResolvedTextLayout;
use blit_raster::{Metrics, Rasterizer};
use blit_text::FontFaceId;

pub struct CachedGlyph {
    pub metrics: Metrics,
    pub alpha: Box<[u8]>,
}

pub struct GlyphCache {
    glyphs: DeferredCache<GlyphKey, CachedGlyph, GlyphScale>,
    rasterizer: Rasterizer,
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
            rasterizer: Rasterizer::default(),
            phase_scale,
        }
    }

    pub fn glyph(
        &mut self,
        text: &ResolvedTextLayout<'_>,
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
        let Self {
            glyphs,
            rasterizer,
            phase_scale,
        } = self;
        let (_, index) = glyphs.get_or_insert(key, |key| {
            let face = text
                .font_face(key.face)
                .expect("text backend returned an unknown font");
            let (metrics, alpha) = rasterizer.rasterize(
                face,
                key.glyph,
                f32::from_bits(key.size),
                key.phase as f32 * *phase_scale,
            );
            (
                key,
                CachedGlyph {
                    metrics,
                    alpha: alpha.into_boxed_slice(),
                },
            )
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
