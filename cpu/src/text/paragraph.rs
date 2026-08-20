use std::{mem::size_of, vec::Vec};

use blit::paint::{
    FontId, HorizontalAlign, TextLayoutRequest, TextOverflow, TextRequest, TextRunId, TextWrap,
    VerticalAlign,
};
use blit_cache::{DeferredCache, Scale};
use blit_font::{GlyphRasterConfig, Layout, LayoutSettings, TextRun};

pub struct ParagraphCache {
    layouts: DeferredCache<LayoutKey, ParagraphLayout, LayoutScale>,
    paints: DeferredCache<PaintKey, ParagraphPaint, PaintScale>,
    layout: Layout,
}

struct LayoutScale;

impl Scale<LayoutKey, ParagraphLayout> for LayoutScale {
    fn weight(&self, _key: &LayoutKey, paragraph: &ParagraphLayout) -> usize {
        size_of::<ParagraphLayout>()
            + paragraph.glyphs.len() * size_of::<blit_font::GlyphPosition>()
            + paragraph.lines.len() * size_of::<blit_font::LinePosition>()
    }
}

struct PaintScale;

impl Scale<PaintKey, ParagraphPaint> for PaintScale {
    fn weight(&self, _key: &PaintKey, paragraph: &ParagraphPaint) -> usize {
        size_of::<ParagraphPaint>()
            + paragraph.glyphs.len() * size_of::<PaintGlyph>()
            + paragraph.carets.len() * size_of::<Caret>()
    }
}

impl ParagraphCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            layouts: DeferredCache::new(LayoutScale, capacity),
            paints: DeferredCache::new(PaintScale, capacity),
            layout: Layout::with_metric_cache_capacity(0),
        }
    }

    pub fn measure(
        &mut self,
        key: LayoutKey,
        request: &TextLayoutRequest,
        run: &TextRun,
        scale_factor: f32,
    ) -> blit::geometry::LogicalSize {
        let Self {
            layouts, layout, ..
        } = self;
        let size =
            match layouts.get_or_insert(key, || Self::layout(layout, request, run, scale_factor)) {
                Ok((paragraph, _)) => paragraph.size,
                Err(paragraph) => paragraph.size,
            };
        blit::geometry::LogicalSize {
            width: size.width / scale_factor,
            height: size.height / scale_factor,
        }
    }

    pub fn prepare(
        &mut self,
        key: PaintKey,
        request: &TextRequest,
        run: &TextRun,
        face: usize,
        font: &blit_font::Font,
        scale_factor: f32,
    ) -> usize {
        let Self {
            layouts,
            paints,
            layout,
        } = self;
        let Ok((_, index)) = paints.get_or_insert(key, || {
            let layout_key = LayoutKey::paint(request, scale_factor);
            let Ok((_, layout_index)) = layouts.get_or_insert(layout_key, || {
                Self::layout_for_paint(layout, request, run, scale_factor)
            }) else {
                panic!("paragraph cache capacity must fit a layout");
            };
            Self::paint(
                layouts.get_index(layout_index),
                request,
                face,
                font,
                scale_factor,
            )
        }) else {
            panic!("paragraph cache capacity must fit positioned text");
        };
        index
    }

    pub fn get(&self, index: usize) -> &ParagraphPaint {
        self.paints.get_index(index)
    }

    pub fn finish_frame(&mut self) {
        self.layouts.trim_to_weight();
        self.paints.trim_to_weight();
    }

    pub fn paint_key(request: &TextRequest, scale_factor: f32) -> PaintKey {
        let area = request.area.to_physical(scale_factor);
        PaintKey {
            layout: LayoutKey::paint(request, scale_factor),
            width: area.width,
            height: area.height,
            offset_x: (request.offset_x * scale_factor).round() as i32,
            overflow: request.options.overflow,
            horizontal_align: request.options.horizontal_align,
            vertical_align: request.options.vertical_align,
        }
    }

    pub fn layout_key(request: &TextLayoutRequest, scale_factor: f32) -> LayoutKey {
        LayoutKey {
            text: request.text,
            max_width: (request.wrap != TextWrap::None).then(|| {
                request.max_width.map_or(i32::MAX, |width| {
                    (width.max(0.0) * scale_factor).ceil() as i32
                })
            }),
            font: request.style.font,
            size: (request.style.size * scale_factor).to_bits(),
            weight: request.style.weight,
            wrap: request.wrap,
            max_lines: request.max_lines,
        }
    }

    fn layout(
        layout: &mut Layout,
        request: &TextLayoutRequest,
        run: &TextRun,
        scale_factor: f32,
    ) -> ParagraphLayout {
        layout.layout_run(
            run,
            LayoutSettings {
                max_width: (request.wrap != TextWrap::None).then_some(
                    request
                        .max_width
                        .map_or(f32::MAX, |width| (width.max(0.0) * scale_factor).ceil()),
                ),
                wrap: request.wrap,
                ..LayoutSettings::default()
            },
        );
        ParagraphLayout {
            size: Self::layout_size(layout, run, request.max_lines),
            glyphs: layout.glyphs().into(),
            lines: layout
                .lines()
                .map_or_else(Box::<[blit_font::LinePosition]>::default, Into::into),
        }
    }

    fn layout_for_paint(
        layout: &mut Layout,
        request: &TextRequest,
        run: &TextRun,
        scale_factor: f32,
    ) -> ParagraphLayout {
        Self::layout(
            layout,
            &TextLayoutRequest {
                text: request.text,
                style: request.style,
                wrap: request.options.wrap,
                max_width: (request.options.wrap != TextWrap::None)
                    .then_some(request.area.width.max(0.0)),
                max_lines: request.options.max_lines,
            },
            run,
            scale_factor,
        )
    }

    fn layout_size(
        layout: &Layout,
        run: &TextRun,
        max_lines: Option<u16>,
    ) -> blit::geometry::LogicalSize {
        let lines = layout.lines().map_or(&[][..], |lines| lines);
        let lines = &lines[..lines.len().min(max_lines.map_or(usize::MAX, usize::from))];
        blit::geometry::LogicalSize {
            width: lines.iter().map(|line| line.width).fold(0.0, f32::max),
            height: lines.last().map_or_else(
                || run.line_metrics().new_line_size,
                |line| line.baseline_y - line.max_ascent + line.max_new_line_size,
            ),
        }
    }

    fn paint(
        paragraph: &ParagraphLayout,
        request: &TextRequest,
        face: usize,
        font: &blit_font::Font,
        scale_factor: f32,
    ) -> ParagraphPaint {
        let area = request.area.to_physical(scale_factor);
        let mut visible_lines = paragraph
            .lines
            .len()
            .min(request.options.max_lines.map_or(usize::MAX, usize::from));
        if request.options.overflow == TextOverflow::Ellipsis {
            let mut height = 0.0;
            let mut fitting_lines = 0;
            for line in &paragraph.lines {
                height += line.max_new_line_size;
                if height > area.height.max(0) as f32 {
                    break;
                }
                fitting_lines += 1;
            }
            visible_lines = visible_lines.min(fitting_lines);
        }

        let lines_truncated = visible_lines < paragraph.lines.len();
        let line_overflows = visible_lines != 0 && request.options.wrap == TextWrap::None && {
            let line = paragraph.lines[visible_lines - 1];
            paragraph.glyphs[line.glyph_start..=line.glyph_end]
                .iter()
                .any(|glyph| glyph.x + glyph.width as f32 > area.width as f32)
        };
        let ellipsize = request.options.overflow == TextOverflow::Ellipsis
            && visible_lines != 0
            && (lines_truncated || line_overflows);
        let size = request.style.size * scale_factor;
        let ellipsis_id = font.glyph_id('…');
        let ellipsis_metrics = font.metrics(ellipsis_id, size);
        let ellipsis_advance = ellipsis_metrics.advance_width.ceil();
        let content_height = paragraph.lines[..visible_lines].last().map_or(0.0, |line| {
            line.baseline_y - line.max_ascent + line.max_new_line_size
        });
        let vertical_offset = match request.options.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Center => ((area.height as f32 - content_height) / 2.0).floor(),
            VerticalAlign::Bottom => (area.height as f32 - content_height).floor(),
        };
        let paint_offset_x = request.offset_x * scale_factor;
        let mut glyphs = Vec::with_capacity(paragraph.glyphs.len());
        let mut carets = Vec::with_capacity(paragraph.glyphs.len() + 1);
        let mut left = area.width;
        let mut top = area.height;
        let mut right = 0;
        let mut bottom = 0;

        for (line_index, line) in paragraph.lines[..visible_lines].iter().enumerate() {
            let source = &paragraph.glyphs[line.glyph_start..=line.glyph_end];
            let final_ellipsis_line = ellipsize && line_index + 1 == visible_lines;
            let mut source_end = source.len();
            let mut line_width = line.width;
            if final_ellipsis_line {
                let available = area.width.max(0) as f32 - ellipsis_advance;
                source_end = source
                    .iter()
                    .take_while(|glyph| glyph.x + glyph.width as f32 <= available)
                    .count();
                while source_end != 0 && source[source_end - 1].parent.is_whitespace() {
                    source_end -= 1;
                }
                line_width = source[..source_end]
                    .last()
                    .map_or(ellipsis_advance, |glyph| {
                        glyph.pen_x + glyph.advance + ellipsis_advance
                    });
            }
            let natural_width = if request.options.wrap == TextWrap::None {
                line_width
            } else {
                line.width
            };
            let horizontal_offset = match request.options.horizontal_align {
                HorizontalAlign::Left => 0.0,
                HorizontalAlign::Center => ((area.width as f32 - natural_width) / 2.0).floor(),
                HorizontalAlign::Right => (area.width as f32 - natural_width).floor(),
            } - paint_offset_x;
            let mut last = None;
            for glyph in &source[..source_end] {
                let x = glyph.pen_x + horizontal_offset;
                let end = x + glyph.advance;
                last = Some((glyph.byte_offset + glyph.parent.len_utf8(), end));
                carets.push(Caret {
                    byte_offset: glyph.byte_offset,
                    x,
                    y: line.baseline_y - line.max_ascent + vertical_offset,
                    height: line.max_new_line_size,
                });
                if !glyph.char_data.is_control() && glyph.width != 0 && glyph.height != 0 {
                    let x = (glyph.x + horizontal_offset).round() as i32;
                    let y = (glyph.y + vertical_offset).round() as i32;
                    let glyph_right = x.saturating_add(glyph.width as i32);
                    let glyph_bottom = y.saturating_add(glyph.height as i32);
                    if x < area.width && glyph_right > 0 && y < area.height && glyph_bottom > 0 {
                        left = left.min(x.max(0));
                        top = top.min(y.max(0));
                        right = right.max(glyph_right.min(area.width));
                        bottom = bottom.max(glyph_bottom.min(area.height));
                        glyphs.push(PaintGlyph {
                            key: glyph.key,
                            x,
                            y,
                        });
                    }
                }
            }
            if final_ellipsis_line {
                let pen_x = source[..source_end]
                    .last()
                    .map_or(0.0, |glyph| glyph.pen_x + glyph.advance);
                let x = (pen_x + ellipsis_metrics.bounds.xmin.floor() + horizontal_offset).round()
                    as i32;
                let y = (line.baseline_y
                    + (-ellipsis_metrics.bounds.height - ellipsis_metrics.bounds.ymin).floor()
                    + vertical_offset)
                    .round() as i32;
                let glyph_right = x.saturating_add(ellipsis_metrics.width as i32);
                let glyph_bottom = y.saturating_add(ellipsis_metrics.height as i32);
                if x < area.width && glyph_right > 0 && y < area.height && glyph_bottom > 0 {
                    left = left.min(x.max(0));
                    top = top.min(y.max(0));
                    right = right.max(glyph_right.min(area.width));
                    bottom = bottom.max(glyph_bottom.min(area.height));
                    glyphs.push(PaintGlyph {
                        key: GlyphRasterConfig {
                            glyph_id: ellipsis_id,
                            size,
                        },
                        x,
                        y,
                    });
                }
            }
            if let Some((byte_offset, x)) = last {
                carets.push(Caret {
                    byte_offset,
                    x,
                    y: line.baseline_y - line.max_ascent + vertical_offset,
                    height: line.max_new_line_size,
                });
            }
        }

        if carets.is_empty() {
            let height = font.horizontal_line_metrics(size).new_line_size.ceil();
            let y = match request.options.vertical_align {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Center => (area.height as f32 - height) / 2.0,
                VerticalAlign::Bottom => area.height as f32 - height,
            };
            carets.push(Caret {
                byte_offset: 0,
                x: -paint_offset_x,
                y,
                height,
            });
        }

        ParagraphPaint {
            face,
            x: left,
            y: top,
            width: (right - left).max(0) as usize,
            height: (bottom - top).max(0) as usize,
            glyphs: glyphs.into_boxed_slice(),
            carets: carets.into_boxed_slice(),
        }
    }
}

pub struct ParagraphLayout {
    size: blit::geometry::LogicalSize,
    glyphs: Box<[blit_font::GlyphPosition]>,
    lines: Box<[blit_font::LinePosition]>,
}

pub struct ParagraphPaint {
    pub face: usize,
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub glyphs: Box<[PaintGlyph]>,
    pub carets: Box<[Caret]>,
}

#[derive(Clone, Copy)]
pub struct PaintGlyph {
    pub key: GlyphRasterConfig,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy)]
pub struct Caret {
    pub byte_offset: usize,
    pub x: f32,
    pub y: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayoutKey {
    text: TextRunId,
    max_width: Option<i32>,
    font: FontId,
    size: u32,
    weight: u16,
    wrap: TextWrap,
    max_lines: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaintKey {
    layout: LayoutKey,
    width: i32,
    height: i32,
    offset_x: i32,
    overflow: TextOverflow,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
}

impl LayoutKey {
    fn paint(request: &TextRequest, scale_factor: f32) -> Self {
        Self {
            text: request.text,
            max_width: (request.options.wrap != TextWrap::None)
                .then_some((request.area.width.max(0.0) * scale_factor).ceil() as i32),
            font: request.style.font,
            size: (request.style.size * scale_factor).to_bits(),
            weight: request.style.weight,
            wrap: request.options.wrap,
            max_lines: request.options.max_lines,
        }
    }
}
