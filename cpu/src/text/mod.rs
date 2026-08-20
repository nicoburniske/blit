mod font;
mod paragraph;

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    mem::size_of,
};

use blit::{
    color::Color,
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    paint::{TextLayoutRequest, TextRequest, TextRunId, TextStyle},
};
use blit_cache::{DeferredCache, Scale};
use blit_font::{Layout, TextRun};
use font::FontCache;
use paragraph::ParagraphCache;

use crate::{Pixel, PixelSpan, RendererConfig};

pub struct TextRenderer {
    fonts: FontCache,
    runs: DeferredCache<RunKey, CachedRun, RunScale>,
    run_builder: Layout,
    next_run: u32,
    paragraphs: ParagraphCache,
    prepared: Vec<PreparedGlyph>,
}

struct CachedRun {
    id: TextRunId,
    face: usize,
    run: TextRun,
}

struct RunScale;

impl Scale<RunKey, CachedRun> for RunScale {
    fn weight(&self, _key: &RunKey, run: &CachedRun) -> usize {
        size_of::<CachedRun>() + run.run.allocated_bytes()
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct RunKey {
    digest: u64,
    len: usize,
    font: blit::paint::FontId,
    size: u32,
    weight: u16,
}

#[derive(Clone, Copy, Hash)]
struct RunQuery {
    digest: u64,
    len: usize,
    font: blit::paint::FontId,
    size: u32,
    weight: u16,
}

pub struct PreparedGlyph {
    pub cache: usize,
    pub x: i32,
    pub y: i32,
}

impl TextRenderer {
    pub fn new(config: RendererConfig) -> Self {
        Self {
            fonts: FontCache::new(config.fonts, config.glyph_cache_capacity),
            runs: DeferredCache::new(RunScale, config.paragraph_cache_capacity),
            run_builder: Layout::with_metric_cache_capacity(config.font_metric_cache_capacity),
            next_run: 1,
            paragraphs: ParagraphCache::new(config.paragraph_cache_capacity),
            prepared: Vec::new(),
        }
    }

    pub fn text_run(&mut self, text: &str, style: TextStyle, scale_factor: f32) -> TextRunId {
        let Some((face, font)) = self.fonts.font(style.font, style.weight) else {
            return TextRunId::default();
        };
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let query = RunQuery {
            digest: hasher.finish(),
            len: text.len(),
            font: style.font,
            size: (style.size * scale_factor).to_bits(),
            weight: style.weight,
        };
        let next_run = self.next_run;
        let Ok((_, index)) = self.runs.get_or_insert_by(
            &query,
            |key, cached| {
                key.digest == query.digest
                    && key.len == query.len
                    && key.font == query.font
                    && key.size == query.size
                    && key.weight == query.weight
                    && cached.run.matches(text)
            },
            || {
                (
                    RunKey {
                        digest: query.digest,
                        len: query.len,
                        font: query.font,
                        size: query.size,
                        weight: query.weight,
                    },
                    CachedRun {
                        id: TextRunId(u64::from(next_run) << 32),
                        face,
                        run: self
                            .run_builder
                            .text_run(font, text, style.size * scale_factor),
                    },
                )
            },
        ) else {
            unreachable!("deferred text run cache accepts an oversized frame entry")
        };
        if self.runs.get_index(index).id.0 as u32 == 0 {
            let slot = u32::try_from(index + 1).expect("too many cached text runs");
            self.runs
                .update_index(index, |run| run.id.0 |= u64::from(slot));
            self.next_run = self.next_run.checked_add(1).expect("too many text runs");
        }
        self.runs.get_index(index).id
    }

    pub fn prepare(
        &mut self,
        request: &TextRequest,
        scale_factor: f32,
    ) -> (u32, u32, PhysicalRect) {
        let area = request.area.to_physical(scale_factor);
        let Some(index) = (request.text.0 as u32)
            .checked_sub(1)
            .map(|index| index as usize)
        else {
            return (0, 0, PhysicalRect::default());
        };
        let cached = self.runs.get_index(index);
        assert_eq!(cached.id, request.text, "expired text run");
        let face = cached.face;
        let run = &cached.run;
        let font = self.fonts.get_font(face);
        let paragraph = self.paragraphs.prepare(
            ParagraphCache::paint_key(request, scale_factor),
            request,
            run,
            face,
            font,
            scale_factor,
        );
        let paragraph = self.paragraphs.get(paragraph);
        if paragraph.width == 0 || paragraph.height == 0 {
            return (0, 0, PhysicalRect::default());
        }
        let start = u32::try_from(self.prepared.len()).expect("too many prepared glyphs");
        for glyph in &paragraph.glyphs {
            self.prepared.push(PreparedGlyph {
                cache: self.fonts.glyph(paragraph.face, glyph.key),
                x: glyph.x,
                y: glyph.y,
            });
        }
        let end = u32::try_from(self.prepared.len()).expect("too many prepared glyphs");
        (
            start,
            end,
            PhysicalRect {
                x: area.x.saturating_add(paragraph.x),
                y: area.y.saturating_add(paragraph.y),
                width: paragraph.width as i32,
                height: paragraph.height as i32,
            },
        )
    }

    pub fn draw_line<P: Pixel>(
        &self,
        glyph_start: u32,
        glyph_end: u32,
        area: PhysicalRect,
        color: Color,
        line: i32,
        row: PixelSpan<'_, P>,
        clip: PhysicalRect,
    ) {
        if line < clip.y || line >= clip.y.saturating_add(clip.height) {
            return;
        }
        let row_end = row.x.saturating_add(row.pixels.len() as i32);
        for glyph in &self.prepared[glyph_start as usize..glyph_end as usize] {
            let cached = self.fonts.get(glyph.cache);
            let x = area.x.saturating_add(glyph.x);
            let y = area.y.saturating_add(glyph.y);
            if line < y || line >= y.saturating_add(cached.metrics.height as i32) {
                continue;
            }
            let left = x.max(row.x).max(clip.x);
            let right = x
                .saturating_add(cached.metrics.width as i32)
                .min(row_end)
                .min(clip.x.saturating_add(clip.width));
            if left >= right {
                continue;
            }
            let source_x = (left - x) as usize;
            let source_y = (line - y) as usize;
            let len = (right - left) as usize;
            let source = source_y * cached.metrics.width + source_x;
            P::blend_alpha_slice(
                &mut row.pixels[(left - row.x) as usize..][..len],
                color,
                &cached.alpha[source..source + len],
            );
        }
    }

    pub fn finish_frame(&mut self) {
        self.paragraphs.finish_frame();
        self.runs.trim_to_weight();
        self.fonts.finish_frame();
        self.prepared.clear();
    }

    pub fn offset_at_position(
        &mut self,
        request: &TextRequest,
        position: LogicalPoint,
        scale_factor: f32,
    ) -> usize {
        let Some(index) = (request.text.0 as u32)
            .checked_sub(1)
            .map(|index| index as usize)
        else {
            return 0;
        };
        let cached = self.runs.get_index(index);
        let face = cached.face;
        let paragraph = self.paragraphs.prepare(
            ParagraphCache::paint_key(request, scale_factor),
            request,
            &cached.run,
            face,
            self.fonts.get_font(face),
            scale_factor,
        );
        let paragraph = self.paragraphs.get(paragraph);
        let x = (position.x - request.area.x) * scale_factor;
        let y = (position.y - request.area.y) * scale_factor;
        paragraph
            .carets
            .iter()
            .min_by(|left, right| {
                let left_distance = (left.x - x).powi(2) + (left.y + left.height / 2.0 - y).powi(2);
                let right_distance =
                    (right.x - x).powi(2) + (right.y + right.height / 2.0 - y).powi(2);
                left_distance.total_cmp(&right_distance)
            })
            .map_or(0, |caret| caret.byte_offset.min(cached.run.len()))
    }

    pub fn measure(&mut self, request: &TextLayoutRequest, scale_factor: f32) -> LogicalSize {
        let Some(index) = (request.text.0 as u32)
            .checked_sub(1)
            .map(|index| index as usize)
        else {
            return LogicalSize::default();
        };
        let cached = self.runs.get_index(index);
        assert_eq!(cached.id, request.text, "expired text run");
        self.paragraphs.measure(
            ParagraphCache::layout_key(request, scale_factor),
            request,
            &cached.run,
            scale_factor,
        )
    }

    pub fn cursor_rect(
        &mut self,
        request: &TextRequest,
        byte_offset: usize,
        scale_factor: f32,
    ) -> LogicalRect {
        let Some(index) = (request.text.0 as u32)
            .checked_sub(1)
            .map(|index| index as usize)
        else {
            return LogicalRect {
                x: request.area.x,
                y: request.area.y,
                width: 1.0,
                height: request.style.size,
            };
        };
        let cached = self.runs.get_index(index);
        let face = cached.face;
        let paragraph = self.paragraphs.prepare(
            ParagraphCache::paint_key(request, scale_factor),
            request,
            &cached.run,
            face,
            self.fonts.get_font(face),
            scale_factor,
        );
        let paragraph = self.paragraphs.get(paragraph);
        let Some(caret) = paragraph
            .carets
            .iter()
            .min_by_key(|caret| caret.byte_offset.abs_diff(byte_offset))
        else {
            return LogicalRect {
                x: request.area.x,
                y: request.area.y,
                width: 1.0,
                height: request.style.size,
            };
        };
        LogicalRect {
            x: request.area.x + caret.x / scale_factor,
            y: request.area.y + caret.y / scale_factor,
            width: 1.0,
            height: caret.height / scale_factor,
        }
    }
}
