use std::mem::size_of;

use crate::text_types::{
    HorizontalAlign, TextLayoutRequest, TextOverflow, TextRequest, TextRunId, TextWrap,
    VerticalAlign,
};
use blit::Scale2;
use blit_cache::{DeferredCache, Scale};
use blit_font::{Layout, LayoutSettings, LinePosition, TextRun};
use blit_text::Glyph;

pub struct ParagraphCache {
    layouts: DeferredCache<LayoutKey, ParagraphLayout, LayoutScale>,
    paints: DeferredCache<PaintKey, ParagraphPaint, PaintScale>,
    layout: Layout,
    // reusable paragraph resolution scratch buffers
    glyphs: Vec<Glyph>,
    lines: Vec<PaintLine>,
    carets: Vec<Caret>,
}

struct LayoutScale;
struct PaintScale;

impl Scale<LayoutKey, ParagraphLayout> for LayoutScale {
    fn weight(&self, _key: &LayoutKey, paragraph: &ParagraphLayout) -> usize {
        size_of::<ParagraphLayout>() + paragraph.lines.len() * size_of::<LinePosition>()
    }
}

impl Scale<PaintKey, ParagraphPaint> for PaintScale {
    fn weight(&self, _key: &PaintKey, paragraph: &ParagraphPaint) -> usize {
        size_of::<ParagraphPaint>()
            + paragraph.glyphs.len() * size_of::<Glyph>()
            + paragraph.lines.len() * size_of::<PaintLine>()
            + paragraph.carets.len() * size_of::<Caret>()
    }
}

impl ParagraphCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            layouts: DeferredCache::new(LayoutScale, capacity),
            paints: DeferredCache::new(PaintScale, capacity),
            layout: Layout::with_metric_cache_capacity(0),
            glyphs: Vec::new(),
            lines: Vec::new(),
            carets: Vec::new(),
        }
    }

    pub fn measure(
        &mut self,
        key: LayoutKey,
        request: &TextLayoutRequest,
        run: &TextRun,
        scale_factor: f32,
    ) -> blit::geometry::LogicalSize {
        let size = self.layout(key, request.wrap, request.max_width, run, scale_factor);
        let paragraph = self.layouts.get_index(size);
        let lines = &paragraph.lines[..paragraph
            .lines
            .len()
            .min(request.max_lines.map_or(usize::MAX, usize::from))];
        blit::geometry::LogicalSize {
            width: lines.iter().map(|line| line.width).fold(0.0, f32::max) / scale_factor,
            height: lines.last().map_or_else(
                || run.line_metrics().new_line_size,
                |line| line.baseline_y - line.max_ascent + line.max_new_line_size,
            ) / scale_factor,
        }
    }

    pub fn prepare(&mut self, request: &TextRequest, run: &TextRun, scale_factor: f32) -> usize {
        self.layout(
            LayoutKey::paint(request, scale_factor),
            request.options.wrap,
            (request.options.wrap != TextWrap::None).then_some(request.area.width.max(0.0)),
            run,
            scale_factor,
        )
    }

    pub fn prepare_paint(
        &mut self,
        request: &TextRequest,
        run: &TextRun,
        font: &blit_font::Font,
        scale_factor: f32,
    ) -> usize {
        let layout = self.prepare(request, run, scale_factor);
        let Self {
            layouts,
            paints,
            glyphs,
            lines,
            carets,
            ..
        } = self;
        let (_, index) = paints.get_or_insert(PaintKey::new(request, scale_factor), || {
            let resolved = resolve(
                layouts.get_index(layout),
                request,
                run,
                font,
                scale_factor,
                glyphs,
                Some(lines),
                carets,
                true,
                true,
            );
            ParagraphPaint {
                bounds: resolved,
                glyphs: glyphs.as_slice().into(),
                lines: lines.as_slice().into(),
                carets: carets.as_slice().into(),
            }
        });
        index
    }

    pub fn get_paint(&self, index: usize) -> &ParagraphPaint {
        self.paints.get_index(index)
    }

    pub fn finish_frame(&mut self) {
        self.layouts.trim_to_weight();
        self.paints.trim_to_weight();
    }

    pub fn layout_key(request: &TextLayoutRequest, scale_factor: f32) -> LayoutKey {
        LayoutKey {
            text: request.text,
            max_width: (request.wrap != TextWrap::None).then(|| {
                request.max_width.map_or(i32::MAX, |width| {
                    (width.max(0.0) * scale_factor).ceil() as i32
                })
            }),
            wrap: request.wrap,
        }
    }

    fn layout(
        &mut self,
        key: LayoutKey,
        wrap: TextWrap,
        max_width: Option<f32>,
        run: &TextRun,
        scale_factor: f32,
    ) -> usize {
        let layout = &mut self.layout;
        let (_, index) = self.layouts.get_or_insert(key, || {
            layout.layout_lines(
                run,
                LayoutSettings {
                    max_width: (wrap != TextWrap::None).then_some(
                        max_width.map_or(f32::MAX, |width| (width.max(0.0) * scale_factor).ceil()),
                    ),
                    wrap,
                    ..LayoutSettings::default()
                },
            );
            ParagraphLayout {
                lines: layout
                    .lines()
                    .map_or_else(Box::<[LinePosition]>::default, Into::into),
            }
        });
        index
    }
}

pub struct ParagraphLayout {
    pub lines: Box<[LinePosition]>,
}

pub struct ParagraphPaint {
    pub bounds: ResolvedParagraph,
    pub glyphs: Box<[Glyph]>,
    pub lines: Box<[PaintLine]>,
    pub carets: Box<[Caret]>,
}

#[derive(Clone, Copy)]
pub struct PaintLine {
    pub glyph_start: u32,
    pub glyph_end: u32,
    pub top: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy)]
pub struct ResolvedParagraph {
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
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
    wrap: TextWrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PaintKey {
    layout: LayoutKey,
    width: i32,
    height: i32,
    offset_x: i32,
    overflow: TextOverflow,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
    max_lines: Option<u16>,
}

impl PaintKey {
    fn new(request: &TextRequest, scale_factor: f32) -> Self {
        let area = request.area.to_physical(Scale2::uniform(scale_factor));
        Self {
            layout: LayoutKey::paint(request, scale_factor),
            width: area.width,
            height: area.height,
            offset_x: (request.offset_x * scale_factor).round() as i32,
            overflow: request.options.overflow,
            horizontal_align: request.options.horizontal_align,
            vertical_align: request.options.vertical_align,
            max_lines: request.options.max_lines,
        }
    }
}

impl LayoutKey {
    fn paint(request: &TextRequest, scale_factor: f32) -> Self {
        Self {
            text: request.text,
            max_width: (request.options.wrap != TextWrap::None)
                .then_some((request.area.width.max(0.0) * scale_factor).ceil() as i32),
            wrap: request.options.wrap,
        }
    }
}

pub fn resolve(
    paragraph: &ParagraphLayout,
    request: &TextRequest,
    run: &TextRun,
    font: &blit_font::Font,
    scale_factor: f32,
    glyphs: &mut Vec<Glyph>,
    mut lines: Option<&mut Vec<PaintLine>>,
    carets: &mut Vec<Caret>,
    resolve_glyphs: bool,
    resolve_carets: bool,
) -> ResolvedParagraph {
    glyphs.clear();
    if let Some(lines) = &mut lines {
        lines.clear();
    }
    carets.clear();
    let area = request.area.to_physical(Scale2::uniform(scale_factor));
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

    let source_glyphs = run.glyphs();
    let lines_truncated = visible_lines < paragraph.lines.len();
    let line_overflows = visible_lines != 0 && request.options.wrap == TextWrap::None && {
        let line = paragraph.lines[visible_lines - 1];
        let mut pen = 0.0;
        source_glyphs[line.glyph_start..=line.glyph_end]
            .iter()
            .any(|glyph| {
                let overflows = pen + glyph.x + glyph.width as f32 > area.width as f32;
                pen += glyph.advance;
                overflows
            })
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
    if resolve_glyphs {
        glyphs.reserve(source_glyphs.len());
    }
    if resolve_carets {
        carets.reserve(source_glyphs.len() + 1);
    }
    let mut left = area.width;
    let mut top = area.height;
    let mut right = 0;
    let mut bottom = 0;

    for (line_index, line) in paragraph.lines[..visible_lines].iter().enumerate() {
        let glyph_start = glyphs.len();
        let mut line_top = i32::MAX;
        let mut line_bottom = i32::MIN;
        let source = &source_glyphs[line.glyph_start..=line.glyph_end];
        let final_ellipsis_line = ellipsize && line_index + 1 == visible_lines;
        let mut source_end = source.len();
        let mut line_width = line.width;
        if final_ellipsis_line {
            let available = area.width.max(0) as f32 - ellipsis_advance;
            let mut pen = 0.0;
            source_end = 0;
            for glyph in source {
                if pen + glyph.x + glyph.width as f32 > available {
                    break;
                }
                source_end += 1;
                pen += glyph.advance;
            }
            while source_end != 0 && source[source_end - 1].parent.is_whitespace() {
                source_end -= 1;
            }
            line_width = source[..source_end]
                .iter()
                .map(|glyph| glyph.advance)
                .sum::<f32>()
                + ellipsis_advance;
        }
        let horizontal_offset = match request.options.horizontal_align {
            HorizontalAlign::Left => 0.0,
            HorizontalAlign::Center => ((area.width as f32 - line_width) / 2.0).floor(),
            HorizontalAlign::Right => (area.width as f32 - line_width).floor(),
        } - paint_offset_x;
        let mut pen = 0.0;
        let mut last = None;
        for glyph in &source[..source_end] {
            let caret_x = pen + horizontal_offset;
            let end = caret_x + glyph.advance;
            last = Some((glyph.byte_offset + glyph.parent.len_utf8(), end));
            if resolve_carets {
                carets.push(Caret {
                    byte_offset: glyph.byte_offset,
                    x: caret_x,
                    y: line.baseline_y - line.max_ascent + vertical_offset,
                    height: line.max_new_line_size,
                });
            }
            if resolve_glyphs
                && !glyph.char_data.is_control()
                && glyph.width != 0
                && glyph.height != 0
            {
                let x = (pen + glyph.x + horizontal_offset).round() as i32;
                let y = (glyph.y + line.baseline_y + vertical_offset).round() as i32;
                let glyph_right = x.saturating_add(glyph.width as i32);
                let glyph_bottom = y.saturating_add(glyph.height as i32);
                if x < area.width && glyph_right > 0 && y < area.height && glyph_bottom > 0 {
                    left = left.min(x.max(0));
                    top = top.min(y.max(0));
                    right = right.max(glyph_right.min(area.width));
                    bottom = bottom.max(glyph_bottom.min(area.height));
                    line_top = line_top.min(y);
                    line_bottom = line_bottom.max(glyph_bottom);
                    glyphs.push(Glyph {
                        id: u32::from(glyph.key.glyph_id.0),
                        position: blit::LogicalPoint::new(
                            pen + horizontal_offset,
                            line.baseline_y + vertical_offset,
                        ),
                        advance: glyph.advance,
                        cluster: u32::try_from(glyph.byte_offset).expect("text is too long"),
                    });
                }
            }
            pen += glyph.advance;
        }
        if final_ellipsis_line && resolve_glyphs {
            let x = (pen + ellipsis_metrics.bounds.xmin.floor() + horizontal_offset).round() as i32;
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
                line_top = line_top.min(y);
                line_bottom = line_bottom.max(glyph_bottom);
                glyphs.push(Glyph {
                    id: u32::from(ellipsis_id.0),
                    position: blit::LogicalPoint::new(
                        pen + horizontal_offset,
                        line.baseline_y + vertical_offset,
                    ),
                    advance: ellipsis_metrics.advance_width,
                    cluster: u32::try_from(run.len()).expect("text is too long"),
                });
            }
        }
        if glyph_start != glyphs.len()
            && let Some(lines) = &mut lines
        {
            lines.push(PaintLine {
                glyph_start: u32::try_from(glyph_start).expect("too many paragraph glyphs"),
                glyph_end: u32::try_from(glyphs.len()).expect("too many paragraph glyphs"),
                top: line_top,
                bottom: line_bottom,
            });
        }
        if resolve_carets && let Some((byte_offset, x)) = last {
            carets.push(Caret {
                byte_offset,
                x,
                y: line.baseline_y - line.max_ascent + vertical_offset,
                height: line.max_new_line_size,
            });
        }
    }

    if resolve_carets && carets.is_empty() {
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

    ResolvedParagraph {
        x: left,
        y: top,
        width: (right - left).max(0) as usize,
        height: (bottom - top).max(0) as usize,
    }
}
