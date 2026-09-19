use std::{mem::size_of, ptr::NonNull};

use crate::{Pixel, PixelSpan, RendererConfig, glyph::GlyphCache, strategy::command::PreparedText};
use blit::{PhysicalRect, Scale2};
use blit_cache::{DeferredCache, Scale};
use blit_gui::{
    TextLayoutId, TextSystem,
    color::Color,
    text::{HorizontalAlign, TextRequest, VerticalAlign},
};
use blit_text::FontFaceId;

pub struct TextRenderer {
    paints: DeferredCache<PaintKey, CachedPaint, PaintScale>,
    glyphs: GlyphCache,
    prepared: Vec<PreparedGlyph>,
    runs: Vec<PreparedRun>,
    coverage: Vec<u8>,
}

struct CachedPaint {
    bounds: PhysicalRect,
    glyphs: Vec<PaintGlyph>,
    runs: Vec<PaintRun>,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct PaintKey {
    layout: TextLayoutId,
    scale: u32,
    offset_x: u32,
    width: u32,
    height: u32,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
}

#[derive(Clone, Copy)]
struct PaintGlyph {
    face: FontFaceId,
    glyph: u16,
    size: u32,
    x: i32,
    y: i32,
}

struct PaintScale;

impl Scale<PaintKey, CachedPaint> for PaintScale {
    fn weight(&self, _key: &PaintKey, paint: &CachedPaint) -> usize {
        size_of::<PaintKey>()
            + size_of::<CachedPaint>()
            + paint.glyphs.capacity() * size_of::<PaintGlyph>()
            + paint.runs.capacity() * size_of::<PaintRun>()
    }
}

pub struct PreparedGlyph {
    alpha: NonNull<u8>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

struct PaintRun {
    glyph_start: u32,
    glyph_end: u32,
    top: i32,
    bottom: i32,
    span: u32,
}

struct PreparedRun {
    glyph_start: u32,
    glyph_end: u32,
    top: i32,
    bottom: i32,
    color: Color,
}

#[derive(Clone, Copy)]
pub struct PreparedRuns {
    start: u32,
    end: u32,
}

impl PreparedRuns {
    const NONE: Self = Self {
        start: u32::MAX,
        end: u32::MAX,
    };
}

impl TextRenderer {
    pub fn new(config: &RendererConfig) -> Self {
        Self {
            paints: DeferredCache::new(PaintScale, config.paint_cache_capacity),
            glyphs: GlyphCache::new(config.glyph_cache_capacity),
            prepared: Vec::new(),
            runs: Vec::new(),
            coverage: Vec::new(),
        }
    }

    pub fn prepare(
        &mut self,
        text: &mut TextSystem,
        request: &TextRequest,
        colors: &[Option<Color>],
        scale_factor: f32,
    ) -> (u32, u32, PreparedRuns, PhysicalRect) {
        let area = request.area.to_physical(Scale2::uniform(scale_factor));
        let resolved = text.paint_layout(request);
        let key = PaintKey {
            layout: resolved.id,
            scale: scale_factor.to_bits(),
            offset_x: request.offset_x.to_bits(),
            width: request.area.width.to_bits(),
            height: request.area.height.to_bits(),
            horizontal_align: request.options.horizontal_align,
            vertical_align: request.options.vertical_align,
        };
        let glyphs = &mut self.glyphs;
        let (_, paint_index) = self.paints.get_or_insert(key, |key| {
            let mut paint = CachedPaint {
                bounds: PhysicalRect::default(),
                glyphs: Vec::new(),
                runs: Vec::new(),
            };
            let mut has_bounds = false;
            let width = area.width.max(0);
            let height = area.height.max(0);
            for run in &resolved.layout.runs {
                let offset = resolved.line_offset(run.line as usize);
                let start = u32::try_from(paint.glyphs.len()).expect("too many paint glyphs");
                let mut top = i32::MAX;
                let mut bottom = i32::MIN;
                let size = (run.size * scale_factor).to_bits();
                for glyph in
                    &resolved.layout.glyphs[run.glyphs.start as usize..run.glyphs.end as usize]
                {
                    let cached = glyphs.glyph(&resolved, run.face, glyph.id, size);
                    let cached = glyphs.get(cached);
                    let x = ((glyph.position.x + offset.x - request.offset_x) * scale_factor
                        + cached.metrics.bounds.xmin.floor())
                    .round() as i32;
                    let y = ((glyph.position.y + offset.y) * scale_factor
                        + (-cached.metrics.bounds.height - cached.metrics.bounds.ymin).floor())
                    .round() as i32;
                    let glyph_width =
                        i32::try_from(cached.metrics.width).expect("glyph is too wide");
                    let glyph_height =
                        i32::try_from(cached.metrics.height).expect("glyph is too tall");
                    let right = x.saturating_add(glyph_width);
                    let glyph_bottom = y.saturating_add(glyph_height);
                    if glyph_width == 0
                        || glyph_height == 0
                        || x >= width
                        || right <= 0
                        || y >= height
                        || glyph_bottom <= 0
                    {
                        continue;
                    }
                    paint.glyphs.push(PaintGlyph {
                        face: run.face,
                        glyph: glyph.id,
                        size,
                        x,
                        y,
                    });
                    top = top.min(y);
                    bottom = bottom.max(glyph_bottom);
                    let glyph_bounds = PhysicalRect {
                        x: x.max(0),
                        y: y.max(0),
                        width: right.min(width) - x.max(0),
                        height: glyph_bottom.min(height) - y.max(0),
                    };
                    paint.bounds = if has_bounds {
                        paint.bounds.union(glyph_bounds)
                    } else {
                        has_bounds = true;
                        glyph_bounds
                    };
                }
                let end = u32::try_from(paint.glyphs.len()).expect("too many paint glyphs");
                if start != end {
                    paint.runs.push(PaintRun {
                        glyph_start: start,
                        glyph_end: end,
                        top,
                        bottom,
                        span: u32::try_from(run.span).expect("too many text spans"),
                    });
                }
            }
            (key, paint)
        });

        let paint = self.paints.get_index(paint_index);
        let glyph_start = u32::try_from(self.prepared.len()).expect("too many prepared glyphs");
        for glyph in &paint.glyphs {
            let cached = self
                .glyphs
                .glyph(&resolved, glyph.face, glyph.glyph, glyph.size);
            let cached = self.glyphs.get(cached);
            self.prepared.push(PreparedGlyph {
                alpha: NonNull::new(cached.alpha.as_ptr().cast_mut()).unwrap(),
                x: glyph.x,
                y: glyph.y,
                width: u32::try_from(cached.metrics.width).expect("glyph is too wide"),
                height: u32::try_from(cached.metrics.height).expect("glyph is too tall"),
            });
        }
        let glyph_end = u32::try_from(self.prepared.len()).expect("too many prepared glyphs");
        let color = |run: &PaintRun| colors.get(run.span as usize).copied().flatten();
        let runs =
            if paint.runs.len() > 1 || paint.runs.first().is_some_and(|run| color(run).is_some()) {
                let start = u32::try_from(self.runs.len()).expect("too many prepared runs");
                for run in &paint.runs {
                    self.runs.push(PreparedRun {
                        glyph_start: glyph_start
                            .checked_add(run.glyph_start)
                            .expect("too many prepared glyphs"),
                        glyph_end: glyph_start
                            .checked_add(run.glyph_end)
                            .expect("too many prepared glyphs"),
                        top: run.top,
                        bottom: run.bottom,
                        color: color(run).unwrap_or(request.color),
                    });
                }
                PreparedRuns {
                    start,
                    end: u32::try_from(self.runs.len()).expect("too many prepared runs"),
                }
            } else {
                PreparedRuns::NONE
            };
        let bounds = PhysicalRect {
            x: area.x.saturating_add(paint.bounds.x),
            y: area.y.saturating_add(paint.bounds.y),
            width: paint.bounds.width,
            height: paint.bounds.height,
        };
        (glyph_start, glyph_end, runs, bounds)
    }

    pub fn draw_line<P: Pixel>(
        &mut self,
        command: &PreparedText,
        clip_coverage: u8,
        line: i32,
        row: PixelSpan<'_, P>,
        clip: PhysicalRect,
    ) {
        let PreparedText {
            glyph_start,
            glyph_end,
            runs,
            area,
            color,
        } = *command;
        if line < clip.y || line >= clip.y.saturating_add(clip.height) {
            return;
        }
        let row_end = row.x.saturating_add(row.pixels.len() as i32);
        if self.coverage.len() < row.pixels.len() {
            self.coverage.resize(row.pixels.len(), 0);
        }
        let coverage = &mut self.coverage[..row.pixels.len()];
        let clear_start = (clip.x - row.x).max(0).min(row.pixels.len() as i32) as usize;
        let clear_end = (clip.x.saturating_add(clip.width) - row.x)
            .max(0)
            .min(row.pixels.len() as i32) as usize;
        let mut draw = |glyph_start: u32, glyph_end: u32, mut color: Color| {
            color.alpha = (color.alpha as u16 * clip_coverage as u16 / 255) as u8;
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
        };
        if runs.start == PreparedRuns::NONE.start {
            draw(glyph_start, glyph_end, color);
        } else {
            for run in &self.runs[runs.start as usize..runs.end as usize] {
                if line >= area.y.saturating_add(run.top)
                    && line < area.y.saturating_add(run.bottom)
                {
                    draw(run.glyph_start, run.glyph_end, run.color);
                }
            }
        }
    }

    pub fn finish_frame(&mut self) {
        self.prepared.clear();
        self.runs.clear();
        self.paints.trim_to_weight();
        self.glyphs.finish_frame();
    }
}
