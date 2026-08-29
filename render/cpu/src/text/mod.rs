mod font;
mod paragraph;

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    mem::size_of,
    ptr::NonNull,
};

use blit::{
    color::Color,
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    text::{TextLayoutRequest, TextRequest, TextRunId, TextStyle},
};
use blit_cache::{DeferredCache, Scale};
use blit_font::{Layout, TextRun};
use font::FontCache;
use paragraph::{Caret, PaintGlyph, ParagraphCache};

use crate::{Pixel, PixelSpan, RendererConfig};

pub struct TextRenderer {
    fonts: FontCache,
    runs: DeferredCache<RunKey, CachedRun, RunScale>,
    run_builder: Layout,
    next_run: u32,
    paragraphs: ParagraphCache,
    paint_glyphs: Vec<PaintGlyph>,
    carets: Vec<Caret>,
    prepared: Vec<PreparedGlyph>,
    coverage: Vec<u8>,
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
    font: blit::text::FontId,
    size: u32,
    weight: u16,
}

#[derive(Clone, Copy, Hash)]
struct RunQuery {
    digest: u64,
    len: usize,
    font: blit::text::FontId,
    size: u32,
    weight: u16,
}

pub struct PreparedGlyph {
    alpha: NonNull<u8>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy)]
pub struct PreparedLines(u32);

impl PreparedLines {
    // rasterize the full glyph range without cached multiline filtering
    const NONE: Self = Self(u32::MAX);
}

impl TextRenderer {
    pub fn new(config: RendererConfig) -> Self {
        Self {
            fonts: FontCache::new(config.fonts, config.glyph_cache_capacity),
            runs: DeferredCache::new(RunScale, config.paragraph_cache_capacity),
            run_builder: Layout::with_metric_cache_capacity(config.font_metric_cache_capacity),
            next_run: 1,
            paragraphs: ParagraphCache::new(config.paragraph_cache_capacity),
            paint_glyphs: Vec::new(),
            carets: Vec::new(),
            prepared: Vec::new(),
            coverage: Vec::new(),
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
        let (_, index) = self.runs.get_or_insert_by(
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
        );
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
    ) -> (u32, u32, PreparedLines, PhysicalRect) {
        let area = request.area.to_physical(scale_factor);
        let Some(index) = (request.text.0 as u32)
            .checked_sub(1)
            .map(|index| index as usize)
        else {
            return (0, 0, PreparedLines::NONE, PhysicalRect::default());
        };
        let cached = self.runs.get_index(index);
        assert_eq!(cached.id, request.text, "expired text run");
        let face = cached.face;
        let run = &cached.run;
        let font = self.fonts.get_font(face);
        let paint_index = self
            .paragraphs
            .prepare_paint(request, run, font, scale_factor);
        let paint = self.paragraphs.get_paint(paint_index);
        if paint.bounds.width == 0 || paint.bounds.height == 0 {
            return (0, 0, PreparedLines::NONE, PhysicalRect::default());
        }
        let start = u32::try_from(self.prepared.len()).expect("too many prepared glyphs");
        for glyph in &paint.glyphs {
            let cached = self.fonts.glyph(face, glyph.key);
            let cached = self.fonts.get(cached);
            self.prepared.push(PreparedGlyph {
                alpha: NonNull::new(cached.alpha.as_ptr().cast_mut()).unwrap(),
                x: glyph.x,
                y: glyph.y,
                width: u32::try_from(cached.metrics.width).expect("glyph is too wide"),
                height: u32::try_from(cached.metrics.height).expect("glyph is too tall"),
            });
        }
        let end = u32::try_from(self.prepared.len()).expect("too many prepared glyphs");
        let lines = if paint.lines.len() > 1 {
            PreparedLines(u32::try_from(paint_index).expect("too many cached paragraphs"))
        } else {
            PreparedLines::NONE
        };
        (
            start,
            end,
            lines,
            PhysicalRect {
                x: area.x.saturating_add(paint.bounds.x),
                y: area.y.saturating_add(paint.bounds.y),
                width: paint.bounds.width as i32,
                height: paint.bounds.height as i32,
            },
        )
    }

    pub fn draw_line<P: Pixel>(
        &mut self,
        glyph_start: u32,
        glyph_end: u32,
        lines: PreparedLines,
        area: PhysicalRect,
        color: Color,
        line: i32,
        row: PixelSpan<'_, P>,
        clip: PhysicalRect,
    ) {
        if line < clip.y || line >= clip.y.saturating_add(clip.height) {
            return;
        }
        let (glyph_start, glyph_end) = if lines.0 == PreparedLines::NONE.0 {
            (glyph_start, glyph_end)
        } else {
            let mut start = glyph_end;
            let mut end = glyph_start;
            for prepared_line in &self.paragraphs.get_paint(lines.0 as usize).lines {
                if line >= area.y.saturating_add(prepared_line.top)
                    && line < area.y.saturating_add(prepared_line.bottom)
                {
                    start = start.min(
                        glyph_start
                            .checked_add(prepared_line.glyph_start)
                            .expect("too many prepared glyphs"),
                    );
                    end = end.max(
                        glyph_start
                            .checked_add(prepared_line.glyph_end)
                            .expect("too many prepared glyphs"),
                    );
                }
            }
            if start >= end {
                return;
            }
            (start, end)
        };
        let row_end = row.x.saturating_add(row.pixels.len() as i32);
        if self.coverage.len() < row.pixels.len() {
            self.coverage.resize(row.pixels.len(), 0);
        }
        let coverage = &mut self.coverage[..row.pixels.len()];
        let clear_start = (clip.x - row.x).max(0).min(row.pixels.len() as i32) as usize;
        let clear_end = (clip.x.saturating_add(clip.width) - row.x)
            .max(0)
            .min(row.pixels.len() as i32) as usize;
        coverage[clear_start..clear_end].fill(0);
        let mut touched_start = row.pixels.len();
        let mut touched_end = 0usize;
        for glyph in &self.prepared[glyph_start as usize..glyph_end as usize] {
            let x = area.x.saturating_add(glyph.x);
            let y = area.y.saturating_add(glyph.y);
            if line < y || line >= y.saturating_add(glyph.height as i32) {
                continue;
            }
            let left = x.max(row.x).max(clip.x);
            let right = x
                .saturating_add(glyph.width as i32)
                .min(row_end)
                .min(clip.x.saturating_add(clip.width));
            if left >= right {
                continue;
            }
            let source_x = (left - x) as usize;
            let source_y = (line - y) as usize;
            let len = (right - left) as usize;
            let source = source_y * glyph.width as usize + source_x;
            // safety: glyph alpha allocations remain live until finish_frame
            let alpha =
                unsafe { std::slice::from_raw_parts(glyph.alpha.as_ptr().add(source), len) };
            let destination_start = (left - row.x) as usize;
            let destination_end = destination_start + len;
            let overlap = touched_end.saturating_sub(destination_start).min(len);
            let destination = &mut coverage[destination_start..destination_end];
            for (destination, source) in destination[..overlap].iter_mut().zip(alpha) {
                *destination =
                    (*source as u16 + *destination as u16 * (255 - *source as u16) / 255) as u8;
            }
            destination[overlap..].copy_from_slice(&alpha[overlap..]);
            touched_start = touched_start.min(destination_start);
            touched_end = touched_end.max(destination_end);
        }
        if touched_start < touched_end {
            P::blend_alpha_slice(
                &mut row.pixels[touched_start..touched_end],
                color,
                &coverage[touched_start..touched_end],
            );
        }
    }

    pub fn finish_frame(&mut self) {
        self.prepared.clear();
        self.paragraphs.finish_frame();
        self.runs.trim_to_weight();
        self.fonts.finish_frame();
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
        let paragraph = self.paragraphs.prepare(request, &cached.run, scale_factor);
        paragraph::resolve(
            self.paragraphs.get(paragraph),
            request,
            &cached.run,
            self.fonts.get_font(face),
            scale_factor,
            &mut self.paint_glyphs,
            None,
            &mut self.carets,
            false,
            true,
        );
        let x = (position.x - request.area.x) * scale_factor;
        let y = (position.y - request.area.y) * scale_factor;
        self.carets
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
        let width = scale_factor.recip();
        let Some(index) = (request.text.0 as u32)
            .checked_sub(1)
            .map(|index| index as usize)
        else {
            return LogicalRect {
                x: request.area.x,
                y: request.area.y,
                width,
                height: request.style.size,
            };
        };
        let cached = self.runs.get_index(index);
        let face = cached.face;
        let paragraph = self.paragraphs.prepare(request, &cached.run, scale_factor);
        paragraph::resolve(
            self.paragraphs.get(paragraph),
            request,
            &cached.run,
            self.fonts.get_font(face),
            scale_factor,
            &mut self.paint_glyphs,
            None,
            &mut self.carets,
            false,
            true,
        );
        let Some(caret) = self
            .carets
            .iter()
            .min_by_key(|caret| caret.byte_offset.abs_diff(byte_offset))
        else {
            return LogicalRect {
                x: request.area.x,
                y: request.area.y,
                width,
                height: request.style.size,
            };
        };
        LogicalRect {
            x: request.area.x + caret.x / scale_factor,
            y: request.area.y + caret.y / scale_factor,
            width,
            height: caret.height / scale_factor,
        }
    }
}
