use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    mem::size_of,
    ptr::NonNull,
};

use crate::{
    Pixel, PixelSpan, RendererConfig,
    color::Color,
    glyph::GlyphCache,
    text_types::{TextLayoutRequest, TextRequest, TextRunId, TextStyle},
};
use blit::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect, Scale2};
use blit_cache::{DeferredCache, Scale};
use blit_text::{FontId, LayoutRequest, TextLayout, TextSystem};

pub struct TextRenderer {
    text: TextSystem,
    families: Box<[ConfiguredFace]>,
    texts: DeferredCache<TextKey, CachedText, TextScale>,
    layouts: DeferredCache<LayoutKey, CachedLayout, LayoutScale>,
    next_text: u32,
    glyphs: GlyphCache,
    prepared: Vec<PreparedGlyph>,
    coverage: Vec<u8>,
}

#[derive(Clone, Copy)]
struct ConfiguredFace {
    family: crate::text_types::FontId,
    weight: u16,
    font: FontId,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct TextKey {
    digest: u64,
    len: usize,
    font: FontId,
    size: u32,
    weight: u16,
}

struct CachedText {
    id: TextRunId,
    text: Box<str>,
    style: blit_text::TextStyle,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct LayoutKey {
    text: TextRunId,
    max_width: Option<u32>,
    max_height: Option<u32>,
    offset_x: u32,
    max_lines: Option<u16>,
    wrap: blit_text::TextWrap,
    overflow: blit_text::TextOverflow,
    horizontal_align: blit_text::HorizontalAlign,
    vertical_align: blit_text::VerticalAlign,
}

struct TextScale;

impl Scale<TextKey, CachedText> for TextScale {
    fn weight(&self, _key: &TextKey, text: &CachedText) -> usize {
        size_of::<TextKey>() + size_of::<CachedText>() + text.text.len()
    }
}

struct CachedLayout {
    layout: TextLayout,
    paint: Option<CachedPaint>,
}

struct CachedPaint {
    scale: u32,
    bounds: PhysicalRect,
    glyphs: Vec<PaintGlyph>,
    lines: Vec<PreparedLine>,
}

#[derive(Clone, Copy)]
struct PaintGlyph {
    font: FontId,
    glyph: u16,
    size: u32,
    x: i32,
    y: i32,
}

struct LayoutScale;

impl Scale<LayoutKey, CachedLayout> for LayoutScale {
    fn weight(&self, _key: &LayoutKey, cached: &CachedLayout) -> usize {
        size_of::<LayoutKey>()
            + size_of::<CachedLayout>()
            + cached.layout.allocated_bytes()
            + cached.paint.as_ref().map_or(0, |paint| {
                paint.glyphs.capacity() * size_of::<PaintGlyph>()
                    + paint.lines.capacity() * size_of::<PreparedLine>()
            })
    }
}

pub struct PreparedGlyph {
    alpha: NonNull<u8>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

struct PreparedLine {
    glyph_start: u32,
    glyph_end: u32,
    top: i32,
    bottom: i32,
}

#[derive(Clone, Copy)]
pub struct PreparedLines(u32);

impl PreparedLines {
    const NONE: Self = Self(u32::MAX);
}

impl TextRenderer {
    pub fn new(config: RendererConfig, text: TextSystem) -> Self {
        Self {
            text,
            families: config
                .fonts
                .into_iter()
                .map(|face| ConfiguredFace {
                    family: face.id,
                    weight: face.weight,
                    font: face.font,
                })
                .collect(),
            texts: DeferredCache::new(TextScale, config.text_cache_capacity),
            layouts: DeferredCache::new(LayoutScale, config.layout_cache_capacity),
            next_text: 1,
            glyphs: GlyphCache::new(config.glyph_cache_capacity),
            prepared: Vec::new(),
            coverage: Vec::new(),
        }
    }

    pub fn text_run(&mut self, text: &str, style: TextStyle, _scale_factor: f32) -> TextRunId {
        let Some(face) = self
            .families
            .iter()
            .filter(|face| face.family == style.font)
            .min_by_key(|face| {
                (
                    face.weight.abs_diff(style.weight),
                    std::cmp::Reverse(face.weight),
                )
            })
        else {
            return TextRunId::default();
        };
        let style = blit_text::TextStyle {
            font: face.font,
            size: style.size,
            weight: style.weight,
        };
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let key = TextKey {
            digest: hasher.finish(),
            len: text.len(),
            font: style.font,
            size: style.size.to_bits(),
            weight: style.weight,
        };
        let next_text = self.next_text;
        let (_, index) = self.texts.get_or_insert_by(
            &key,
            |candidate, cached| {
                *candidate == key && cached.text.as_ref() == text && cached.style == style
            },
            || {
                (
                    key,
                    CachedText {
                        id: TextRunId(u64::from(next_text) << 32),
                        text: text.into(),
                        style,
                    },
                )
            },
        );
        if self.texts.get_index(index).id.0 as u32 == 0 {
            let slot = u32::try_from(index + 1).expect("too many cached texts");
            self.texts
                .update_index(index, |text| text.id.0 |= u64::from(slot));
            self.next_text = self.next_text.checked_add(1).expect("too many texts");
        }
        self.texts.get_index(index).id
    }

    fn layout(&mut self, text: TextRunId, request: LayoutRequest) -> usize {
        let text_index = (text.0 as u32).checked_sub(1).expect("invalid text") as usize;
        self.texts.update_index(text_index, |cached| {
            assert_eq!(cached.id, text, "expired text")
        });
        let cached = self.texts.get_index(text_index);
        let key = LayoutKey {
            text,
            max_width: request.max_width.map(f32::to_bits),
            max_height: request.max_height.map(f32::to_bits),
            offset_x: request.offset_x.to_bits(),
            max_lines: request.max_lines,
            wrap: request.wrap,
            overflow: request.overflow,
            horizontal_align: request.horizontal_align,
            vertical_align: request.vertical_align,
        };
        let text_system = &mut self.text;
        let (_, index) = self.layouts.get_or_insert(key, || CachedLayout {
            layout: text_system.layout(&cached.text, cached.style, request),
            paint: None,
        });
        index
    }

    pub fn prepare(
        &mut self,
        request: &TextRequest,
        scale_factor: f32,
    ) -> (u32, u32, PreparedLines, PhysicalRect) {
        let area = request.area.to_physical(Scale2::uniform(scale_factor));
        let layout_index = self.layout(request.text, Self::paint_request(request));
        let scale = scale_factor.to_bits();
        let rebuild = self
            .layouts
            .get_index(layout_index)
            .paint
            .as_ref()
            .is_none_or(|paint| paint.scale != scale);
        if rebuild {
            let mut paint = self
                .layouts
                .update_index(layout_index, |cached| cached.paint.take())
                .unwrap_or_else(|| CachedPaint {
                    scale,
                    bounds: PhysicalRect::default(),
                    glyphs: Vec::new(),
                    lines: Vec::new(),
                });
            paint.scale = scale;
            paint.bounds = PhysicalRect::default();
            paint.glyphs.clear();
            paint.lines.clear();
            let mut has_bounds = false;
            let width = area.width.max(0);
            let height = area.height.max(0);
            let text = &self.text;
            let layout = &self.layouts.get_index(layout_index).layout;
            let glyphs = &mut self.glyphs;
            for run in &layout.runs {
                let start = u32::try_from(paint.glyphs.len()).expect("too many paint glyphs");
                let mut line_top = i32::MAX;
                let mut line_bottom = i32::MIN;
                let size = (run.size * scale_factor).to_bits();
                for glyph in &layout.glyphs[run.glyphs.start as usize..run.glyphs.end as usize] {
                    let glyph_id = u16::try_from(glyph.id).expect("glyph id is too large");
                    let cached = glyphs.glyph(text, run.font, glyph_id, size);
                    let cached = glyphs.get(cached);
                    let x = (glyph.position.x * scale_factor + cached.metrics.bounds.xmin.floor())
                        .round() as i32;
                    let y = (glyph.position.y * scale_factor
                        + (-cached.metrics.bounds.height - cached.metrics.bounds.ymin).floor())
                    .round() as i32;
                    let glyph_width =
                        i32::try_from(cached.metrics.width).expect("glyph is too wide");
                    let glyph_height =
                        i32::try_from(cached.metrics.height).expect("glyph is too tall");
                    let right = x.saturating_add(glyph_width);
                    let bottom = y.saturating_add(glyph_height);
                    if glyph_width == 0
                        || glyph_height == 0
                        || x >= width
                        || right <= 0
                        || y >= height
                        || bottom <= 0
                    {
                        continue;
                    }
                    paint.glyphs.push(PaintGlyph {
                        font: run.font,
                        glyph: glyph_id,
                        size,
                        x,
                        y,
                    });
                    line_top = line_top.min(y);
                    line_bottom = line_bottom.max(bottom);
                    let glyph_bounds = PhysicalRect {
                        x: x.max(0),
                        y: y.max(0),
                        width: right.min(width) - x.max(0),
                        height: bottom.min(height) - y.max(0),
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
                    paint.lines.push(PreparedLine {
                        glyph_start: start,
                        glyph_end: end,
                        top: line_top,
                        bottom: line_bottom,
                    });
                }
            }
            self.layouts
                .update_index(layout_index, |cached| cached.paint = Some(paint));
        }

        let paint = self.layouts.get_index(layout_index).paint.as_ref().unwrap();
        let glyph_start = u32::try_from(self.prepared.len()).expect("too many prepared glyphs");
        for glyph in &paint.glyphs {
            let cached = self
                .glyphs
                .glyph(&self.text, glyph.font, glyph.glyph, glyph.size);
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
        let lines = if paint.lines.len() > 1 {
            let index = u32::try_from(layout_index).expect("too many cached layouts");
            assert_ne!(index, PreparedLines::NONE.0, "too many cached layouts");
            PreparedLines(index)
        } else {
            PreparedLines::NONE
        };
        let bounds = PhysicalRect {
            x: area.x.saturating_add(paint.bounds.x),
            y: area.y.saturating_add(paint.bounds.y),
            width: paint.bounds.width,
            height: paint.bounds.height,
        };
        (glyph_start, glyph_end, lines, bounds)
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
            let paint = self
                .layouts
                .get_index(lines.0 as usize)
                .paint
                .as_ref()
                .unwrap();
            for prepared_line in &paint.lines {
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
        self.layouts.trim_to_weight();
        self.texts.trim_to_weight();
        self.glyphs.finish_frame();
    }

    pub fn offset_at_position(
        &mut self,
        request: &TextRequest,
        position: LogicalPoint,
        _scale_factor: f32,
    ) -> usize {
        let layout = self.layout(request.text, Self::paint_request(request));
        self.layouts
            .get_index(layout)
            .layout
            .hit_test(LogicalPoint {
                x: position.x - request.area.x,
                y: position.y - request.area.y,
            })
    }

    pub fn measure(&mut self, request: &TextLayoutRequest, _scale_factor: f32) -> LogicalSize {
        let layout = self.layout(
            request.text,
            LayoutRequest {
                max_width: request.max_width,
                max_height: None,
                offset_x: 0.0,
                max_lines: request.max_lines,
                wrap: match request.wrap {
                    crate::text_types::TextWrap::None => blit_text::TextWrap::None,
                    crate::text_types::TextWrap::Word => blit_text::TextWrap::Word,
                    crate::text_types::TextWrap::Character => blit_text::TextWrap::Glyph,
                },
                overflow: blit_text::TextOverflow::Clip,
                horizontal_align: blit_text::HorizontalAlign::Start,
                vertical_align: blit_text::VerticalAlign::Start,
            },
        );
        self.layouts.get_index(layout).layout.size
    }

    pub fn cursor_rect(
        &mut self,
        request: &TextRequest,
        byte_offset: usize,
        scale_factor: f32,
    ) -> LogicalRect {
        let layout = self.layout(request.text, Self::paint_request(request));
        let rect = self
            .layouts
            .get_index(layout)
            .layout
            .cursor_rect(byte_offset);
        LogicalRect {
            x: request.area.x + rect.x,
            y: request.area.y + rect.y,
            width: scale_factor.recip(),
            height: rect.height,
        }
    }

    fn paint_request(request: &TextRequest) -> LayoutRequest {
        LayoutRequest {
            max_width: Some(request.area.width.max(0.0)),
            max_height: Some(request.area.height.max(0.0)),
            offset_x: request.offset_x,
            max_lines: request.options.max_lines,
            wrap: match request.options.wrap {
                crate::text_types::TextWrap::None => blit_text::TextWrap::None,
                crate::text_types::TextWrap::Word => blit_text::TextWrap::Word,
                crate::text_types::TextWrap::Character => blit_text::TextWrap::Glyph,
            },
            overflow: match request.options.overflow {
                crate::text_types::TextOverflow::Clip => blit_text::TextOverflow::Clip,
                crate::text_types::TextOverflow::Ellipsis => blit_text::TextOverflow::Ellipsis,
            },
            horizontal_align: match request.options.horizontal_align {
                crate::text_types::HorizontalAlign::Left => blit_text::HorizontalAlign::Start,
                crate::text_types::HorizontalAlign::Center => blit_text::HorizontalAlign::Center,
                crate::text_types::HorizontalAlign::Right => blit_text::HorizontalAlign::End,
            },
            vertical_align: match request.options.vertical_align {
                crate::text_types::VerticalAlign::Top => blit_text::VerticalAlign::Start,
                crate::text_types::VerticalAlign::Center => blit_text::VerticalAlign::Center,
                crate::text_types::VerticalAlign::Bottom => blit_text::VerticalAlign::End,
            },
        }
    }
}
