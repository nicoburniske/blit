use std::{collections::HashMap, mem::size_of};

use blit_cache::{DeferredCache, Scale};
use blit_font::{GlyphId, GlyphRasterConfig, Metrics, Rasterizer};
use blit_text::{FontData, FontFace as RegisteredFace, FontId, TextSystem};

use crate::Font;

pub struct FontStore {
    faces: Vec<StoredFace>,
}

struct StoredFace {
    font: Font,
    data: FontData,
    face_index: u32,
}

impl FontStore {
    pub fn new() -> Self {
        Self { faces: Vec::new() }
    }

    pub fn font(&self, id: FontId) -> Option<(usize, &Font)> {
        let index = usize::try_from(id.0).ok()?.checked_sub(1)?;
        self.faces.get(index).map(|face| (index, &face.font))
    }

    pub fn face(&self, id: FontId) -> Option<RegisteredFace> {
        let index = usize::try_from(id.0).ok()?.checked_sub(1)?;
        self.faces.get(index).map(|face| RegisteredFace {
            data: face.data.clone(),
            face_index: face.face_index,
        })
    }

    pub fn register(&mut self, data: FontData, face_index: u32) -> Option<FontId> {
        let font = match &data {
            FontData::Static(data) => Font::from_static_face(data, face_index).ok()?,
            FontData::Shared(data) => Font::from_shared_face(data.clone(), face_index).ok()?,
        };
        let id = FontId(u64::try_from(self.faces.len() + 1).ok()?);
        self.faces.push(StoredFace {
            font,
            data,
            face_index,
        });
        Some(id)
    }
}

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
            glyph: GlyphRasterConfig {
                glyph_id: GlyphId(u16::try_from(glyph).expect("glyph id is too large")),
                size,
            },
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
    font: FontId,
    glyph: GlyphRasterConfig,
}
