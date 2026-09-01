use std::mem::size_of;

use blit_cache::{DeferredCache, Scale};
use blit_text::{FontId, TextSystem};

use crate::raster::{Metrics, Rasterizer};

pub struct CachedGlyph {
    pub metrics: Metrics,
    pub alpha: Box<[u8]>,
}

pub struct GlyphCache {
    glyphs: DeferredCache<GlyphKey, CachedGlyph, GlyphScale>,
    rasterizer: Rasterizer,
}

struct GlyphScale;

impl Scale<GlyphKey, CachedGlyph> for GlyphScale {
    fn weight(&self, _key: &GlyphKey, glyph: &CachedGlyph) -> usize {
        size_of::<CachedGlyph>() + glyph.alpha.len()
    }
}

impl GlyphCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            glyphs: DeferredCache::new(GlyphScale, capacity),
            rasterizer: Rasterizer::default(),
        }
    }

    pub fn glyph(&mut self, text: &TextSystem, font: FontId, glyph: u16, size: u32) -> usize {
        let key = GlyphKey { font, glyph, size };
        let Self { glyphs, rasterizer } = self;
        let (_, index) = glyphs.get_or_insert(key, || {
            let face = text
                .font(key.font)
                .expect("text backend returned an unknown font");
            let (metrics, alpha) = rasterizer.rasterize(&face, key.glyph, f32::from_bits(key.size));
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
    font: FontId,
    glyph: u16,
    size: u32,
}
