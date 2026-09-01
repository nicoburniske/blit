use std::{collections::HashMap, mem::size_of};

use blit_cache::{DeferredCache, Scale};
use blit_font::{GlyphId, Metrics, Rasterizer};
use blit_text::{FontData, FontId, TextSystem};

use crate::Font;

pub struct CachedGlyph {
    pub metrics: Metrics,
    pub alpha: Box<[u8]>,
}

pub struct GlyphCache {
    fonts: HashMap<FontId, Font>,
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
            fonts: HashMap::new(),
            glyphs: DeferredCache::new(GlyphScale, capacity),
            rasterizer: Rasterizer::default(),
        }
    }

    pub fn glyph(&mut self, text: &TextSystem, font: FontId, glyph: u32, size: f32) -> usize {
        let key = GlyphKey {
            font,
            glyph: u16::try_from(glyph).expect("glyph id is too large"),
            size: size.to_bits(),
        };
        let Self {
            fonts,
            glyphs,
            rasterizer,
        } = self;
        let (_, index) = glyphs.get_or_insert(key, || {
            let font = fonts.entry(key.font).or_insert_with(|| {
                let face = text
                    .font(key.font)
                    .expect("text backend returned an unknown font");
                match face.data {
                    FontData::Static(data) => Font::from_static_face(data, face.face_index),
                    FontData::Shared(data) => Font::from_shared_face(data, face.face_index),
                }
                .expect("text backend returned invalid font")
            });
            let (metrics, alpha) =
                rasterizer.rasterize(font, GlyphId(key.glyph), f32::from_bits(key.size));
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
