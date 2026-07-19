use std::mem::size_of;

use blit::{
    paint::{FontId, HorizontalAlign, TextOverflow, TextRequest, TextWrap, VerticalAlign},
    resource::StringId,
};
use blit_cache::{Cache, Scale};
use blit_font::{Layout, LayoutSettings};

use super::font::FontCache;

/// stores measured and rasterized paragraphs between frames
pub struct ParagraphCache {
    cache: Cache<ParagraphKey, Paragraph, ParagraphScale>,
    /// reusable layout scratch state
    layout: Layout,
}

struct ParagraphScale;

impl Scale<ParagraphKey, Paragraph> for ParagraphScale {
    fn weight(&self, _key: &ParagraphKey, paragraph: &Paragraph) -> usize {
        size_of::<Paragraph>()
            + paragraph
                .rendered
                .as_ref()
                .map_or(0, |rendered| rendered.alpha.len() + rendered.carets.len() * size_of::<Caret>())
    }
}

impl ParagraphCache {
    pub fn new(capacity: usize, metric_cache_capacity: usize) -> Self {
        Self {
            cache: Cache::new(ParagraphScale, capacity),
            layout: Layout::with_metric_cache_capacity(metric_cache_capacity),
        }
    }

    /// gets wrapped height
    pub fn measure_height(
        &mut self,
        key: ParagraphKey,
        request: &TextRequest,
        text: &str,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> f32 {
        let Self { cache, layout } = self;
        match cache.get_or_insert_with(key, || Paragraph {
            layout_height: Self::layout_height(layout, request, text, scale_factor, fonts),
            rendered: None,
        }) {
            Ok(paragraph) => paragraph.layout_height / scale_factor,
            Err(paragraph) => paragraph.layout_height / scale_factor,
        }
    }

    /// gets and rasterizes an entry without evicting until the frame ends
    pub fn prepare(
        &mut self,
        key: ParagraphKey,
        request: &TextRequest,
        text: &str,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> usize {
        let Self { cache, layout } = self;
        let Ok((_, index)) = cache.get_or_insert_deferred_with(key, || Paragraph {
            layout_height: Self::layout_height(layout, request, text, scale_factor, fonts),
            rendered: None,
        }) else {
            panic!("paragraph cache capacity must fit an entry");
        };
        cache.update_index(index, |paragraph| {
            if paragraph.rendered.is_none() {
                paragraph.rendered = Some(Self::render(layout, request, text, scale_factor, fonts));
            }
        });
        index
    }

    pub fn get(&self, index: usize) -> &Paragraph { self.cache.get_index(index) }

    pub fn finish_frame(&mut self) { self.cache.trim_to_weight() }

    pub fn retain_strings(&mut self, mut live: impl FnMut(StringId) -> bool) {
        self.cache.retain(|(key, _)| live(key.string));
    }

    /// builds a position independent cache key
    pub fn key(request: &TextRequest, scale_factor: f32) -> ParagraphKey {
        let area = request.area.to_physical(scale_factor);
        ParagraphKey {
            string: request.text,
            width: area.width,
            height: if request.intrinsic_height { 0 } else { area.height },
            offset_x: (request.offset_x * scale_factor).round() as i32,
            font: request.style.font,
            size: (request.style.size * scale_factor).to_bits(),
            weight: request.style.weight,
            wrap: request.options.wrap,
            overflow: request.options.overflow,
            horizontal_align: request.options.horizontal_align,
            vertical_align: request.options.vertical_align,
            max_lines: request.options.max_lines,
            intrinsic_height: request.intrinsic_height,
        }
    }
}

pub struct Paragraph {
    /// layout height exists before rasterization
    pub layout_height: f32,
    pub rendered: Option<RenderedParagraph>,
}

/// raster and caret data independent of screen position
pub struct RenderedParagraph {
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub layout_width: f32,
    pub layout_height: f32,
    pub alpha: Box<[u8]>,
    pub carets: Box<[Caret]>,
}

#[derive(Clone, Copy)]
pub struct Caret {
    pub byte_offset: usize,
    pub x: f32,
    pub y: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParagraphKey {
    string: StringId,
    width: i32,
    height: i32,
    offset_x: i32,
    font: FontId,
    size: u32,
    weight: u16,
    wrap: TextWrap,
    overflow: TextOverflow,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
    max_lines: Option<u16>,
    intrinsic_height: bool,
}

impl ParagraphCache {
    fn layout_height(
        layout: &mut Layout,
        request: &TextRequest,
        text: &str,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> f32 {
        let Some((_, font)) = fonts.font(request.style.font, request.style.weight) else {
            return request.style.size * scale_factor;
        };
        let size = request.style.size * scale_factor;
        let area = request.area.to_physical(scale_factor);
        layout.layout(
            font,
            text,
            size,
            LayoutSettings {
                max_width: (request.options.wrap != TextWrap::None).then_some(area.width.max(0) as f32),
                wrap: request.options.wrap,
                ..LayoutSettings::default()
            },
        );
        layout
            .lines()
            .and_then(|lines| {
                lines.iter().take(request.options.max_lines.map_or(usize::MAX, usize::from)).next_back()
            })
            .map_or_else(
                || font.horizontal_line_metrics(size).new_line_size.ceil(),
                |line| line.baseline_y - line.max_ascent + line.max_new_line_size,
            )
    }

    fn render(
        layout: &mut Layout,
        request: &TextRequest,
        text: &str,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> RenderedParagraph {
        let area = request.area.to_physical(scale_factor);
        let Some((face, font)) = fonts.font(request.style.font, request.style.weight) else {
            return RenderedParagraph {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                layout_width: 0.0,
                layout_height: 0.0,
                alpha: Box::new([]),
                carets: Box::new([]),
            };
        };

        let size = request.style.size * scale_factor;
        let max_width = (request.options.wrap != TextWrap::None).then_some(area.width.max(0) as f32);
        layout.layout(
            font,
            text,
            size,
            LayoutSettings { max_width, wrap: request.options.wrap, ..LayoutSettings::default() },
        );

        let mut rendered = None;
        if let Some(lines) = layout.lines() {
            let mut visible_lines = lines.len();
            if let Some(max_lines) = request.options.max_lines {
                visible_lines = visible_lines.min(max_lines as usize);
            }
            if request.options.overflow == TextOverflow::Ellipsis && !request.intrinsic_height {
                let mut height = 0.0;
                let mut fitting_lines = 0;
                for line in lines {
                    height += line.max_new_line_size;
                    if height > area.height.max(0) as f32 {
                        break;
                    }
                    fitting_lines += 1;
                }
                visible_lines = visible_lines.min(fitting_lines);
            }

            let glyphs = layout.glyphs();
            let lines_truncated = visible_lines < lines.len();
            let line_overflows = visible_lines != 0 && request.options.wrap == TextWrap::None && {
                let line = lines[visible_lines - 1];
                let start = line.glyph_start.min(glyphs.len());
                let end = line.glyph_end.saturating_add(1).min(glyphs.len());
                glyphs[start..end].iter().any(|glyph| glyph.x + glyph.width as f32 > area.width as f32)
            };

            if visible_lines == 0 && !lines.is_empty() {
                rendered = Some(String::new());
            } else if visible_lines != 0 && (lines_truncated || line_overflows) {
                let line = lines[visible_lines - 1];
                let start = line.glyph_start.min(glyphs.len());
                let end = line.glyph_end.saturating_add(1).min(glyphs.len());
                let glyphs = &glyphs[start..end];
                let mut end = glyphs.last().map_or(0, |glyph| glyph.byte_offset + glyph.parent.len_utf8());
                if request.options.overflow == TextOverflow::Ellipsis {
                    let available = area.width.max(0) as f32
                        - font.metrics(font.glyph_id('…'), size).advance_width.ceil();
                    end = glyphs
                        .iter()
                        .take_while(|glyph| glyph.x + glyph.width as f32 <= available)
                        .last()
                        .map_or_else(
                            || glyphs.first().map_or(0, |glyph| glyph.byte_offset),
                            |glyph| glyph.byte_offset + glyph.parent.len_utf8(),
                        );
                    let mut rendered_text = text[..end].trim_end().to_owned();
                    rendered_text.push('…');
                    rendered = Some(rendered_text);
                } else {
                    rendered = Some(text[..end].trim_end_matches(['\r', '\n']).to_owned());
                }
            }
        }

        layout.layout(
            font,
            rendered.as_deref().unwrap_or(text),
            size,
            LayoutSettings {
                max_width,
                max_height: (!request.intrinsic_height).then_some(area.height.max(0) as f32),
                horizontal_align: request.options.horizontal_align,
                vertical_align: request.options.vertical_align,
                wrap: request.options.wrap,
            },
        );
        let layout_height = layout.lines().map_or_else(
            || font.horizontal_line_metrics(size).new_line_size.ceil(),
            |_| layout.height(),
        );
        let glyphs = layout.glyphs();
        let natural_width = glyphs.iter().map(|glyph| glyph.x + glyph.width as f32).fold(0.0, f32::max);
        let offset_x = if request.options.wrap == TextWrap::None {
            match request.options.horizontal_align {
                HorizontalAlign::Left => 0.0,
                HorizontalAlign::Center => (area.width as f32 - natural_width) / 2.0,
                HorizontalAlign::Right => area.width as f32 - natural_width,
            }
        } else {
            0.0
        };
        let paint_offset_x = offset_x - request.offset_x * scale_factor;
        let mut carets = Vec::with_capacity(glyphs.len() + 1);
        let mut layout_width = 0.0f32;
        if let Some(lines) = layout.lines() {
            for line in lines {
                let start = line.glyph_start.min(glyphs.len());
                let end = line.glyph_end.saturating_add(1).min(glyphs.len());
                let mut line_left = f32::INFINITY;
                let mut line_right = f32::NEG_INFINITY;
                let mut last = None;
                for glyph in &glyphs[start..end] {
                    let x = glyph.pen_x + paint_offset_x;
                    let end = x + glyph.advance;
                    line_left = line_left.min(x);
                    line_right = line_right.max(end);
                    last = Some((glyph.byte_offset + glyph.parent.len_utf8(), end));
                    carets.push(Caret {
                        byte_offset: glyph.byte_offset,
                        x,
                        y: line.baseline_y - line.max_ascent,
                        height: line.max_new_line_size,
                    });
                }
                if line_left.is_finite() && line_right.is_finite() {
                    layout_width = layout_width.max(line_right - line_left);
                }
                if let Some((byte_offset, x)) = last {
                    carets.push(Caret {
                        byte_offset,
                        x,
                        y: line.baseline_y - line.max_ascent,
                        height: line.max_new_line_size,
                    });
                }
            }
        }
        if carets.is_empty() {
            let line = font.horizontal_line_metrics(size);
            let height = line.new_line_size.ceil();
            let y = match request.options.vertical_align {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Center => (area.height as f32 - height) / 2.0,
                VerticalAlign::Bottom => area.height as f32 - height,
            };
            carets.push(Caret { byte_offset: 0, x: paint_offset_x, y, height });
        }
        let mut left = area.width;
        let bounds_height = if request.intrinsic_height { i32::MAX } else { area.height };
        let mut top = bounds_height;
        let mut right = 0;
        let mut bottom = 0;
        for glyph in glyphs {
            if glyph.char_data.is_control() || glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            let x = (glyph.x + paint_offset_x).round() as i32;
            let y = glyph.y.round() as i32;
            left = left.min(x.max(0).min(area.width));
            top = top.min(y.max(0).min(bounds_height));
            right = right.max((x + glyph.width as i32).max(0).min(area.width));
            bottom = bottom.max((y + glyph.height as i32).max(0).min(bounds_height));
        }
        let width = (right - left).max(0) as usize;
        let height = (bottom - top).max(0) as usize;
        let mut alpha = vec![0u8; width * height];
        for glyph in glyphs {
            if glyph.char_data.is_control() {
                continue;
            }
            let cached = fonts.glyph(face, glyph.key);
            let cached = match &cached {
                Ok(cached) => *cached,
                Err(cached) => cached,
            };
            let x = (glyph.x + paint_offset_x).round() as i32;
            let y = glyph.y.round() as i32;
            let source_left = (left - x).max(0) as usize;
            let source_top = (top - y).max(0) as usize;
            let source_right = (right - x).min(cached.metrics.width as i32).max(0) as usize;
            let source_bottom = (bottom - y).min(cached.metrics.height as i32).max(0) as usize;
            for source_y in source_top..source_bottom {
                for source_x in source_left..source_right {
                    let destination_x = (x + source_x as i32 - left) as usize;
                    let destination_y = (y + source_y as i32 - top) as usize;
                    let source = cached.alpha[source_y * cached.metrics.width + source_x] as u16;
                    let destination = &mut alpha[destination_y * width + destination_x];
                    *destination = (source + *destination as u16 * (255 - source) / 255) as u8;
                }
            }
        }
        RenderedParagraph {
            x: left,
            y: top,
            width,
            height,
            layout_width,
            layout_height,
            alpha: alpha.into_boxed_slice(),
            carets: carets.into_boxed_slice(),
        }
    }
}

#[cfg(test)]
mod tests {
    use blit::{
        color::Color,
        geometry::LogicalRect,
        paint::{FontId, HorizontalAlign, TextOptions, TextOverflow, TextRequest, TextStyle, TextWrap},
    };

    use super::*;
    use crate::{Font, FontFace};

    fn prepare(
        paragraphs: &mut ParagraphCache,
        request: &TextRequest,
        text: &str,
        fonts: &mut FontCache,
    ) -> usize {
        paragraphs.prepare(ParagraphCache::key(request, 1.0), request, text, 1.0, fonts)
    }

    #[test]
    fn font_lookup_and_overflow_are_exact() {
        let font =
            Font::from_static(include_bytes!("../../../resources/fonts/Montserrat-Regular.ttf")).unwrap();
        let mut fonts =
            FontCache::new(vec![FontFace { id: FontId::default(), weight: 400, font }], 1024 * 1024);
        let mut paragraphs = ParagraphCache::new(1024 * 1024, 256);
        let area = LogicalRect { x: 0.0, y: 0.0, width: 100.0, height: 50.0 };
        let request = TextRequest {
            text: StringId(1),
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle { font: FontId(9), ..TextStyle::default() },
            options: TextOptions::default(),
            intrinsic_height: false,
        };
        let index = prepare(&mut paragraphs, &request, "missing", &mut fonts);
        let missing = paragraphs.get(index).rendered.as_ref().unwrap();
        assert_eq!((missing.width, missing.height), (0, 0));

        let request = TextRequest {
            text: StringId(2),
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions { max_lines: Some(1), ..TextOptions::default() },
            intrinsic_height: false,
        };
        let one_line = prepare(&mut paragraphs, &request, "first\nsecond", &mut fonts);
        let request = TextRequest { text: StringId(3), options: TextOptions::default(), ..request };
        let first = prepare(&mut paragraphs, &request, "first", &mut fonts);
        let one_line = paragraphs.get(one_line).rendered.as_ref().unwrap();
        let first = paragraphs.get(first).rendered.as_ref().unwrap();
        assert_eq!(
            (one_line.x, one_line.y, one_line.width, one_line.height, &one_line.alpha),
            (first.x, first.y, first.width, first.height, &first.alpha),
        );

        let narrow = LogicalRect { width: 12.0, ..area };
        let request = TextRequest {
            text: StringId(4),
            area: narrow,
            options: TextOptions { overflow: TextOverflow::Ellipsis, ..TextOptions::default() },
            ..request
        };
        let truncated = prepare(&mut paragraphs, &request, "WWWW", &mut fonts);
        let request = TextRequest { text: StringId(5), options: TextOptions::default(), ..request };
        let ellipsis = prepare(&mut paragraphs, &request, "…", &mut fonts);
        let truncated = paragraphs.get(truncated).rendered.as_ref().unwrap();
        let ellipsis = paragraphs.get(ellipsis).rendered.as_ref().unwrap();
        assert_eq!(
            (truncated.x, truncated.y, truncated.width, truncated.height, &truncated.alpha),
            (ellipsis.x, ellipsis.y, ellipsis.width, ellipsis.height, &ellipsis.alpha),
        );

        for (id, text) in ["", "first", "first\nsecond", "first\n"].into_iter().enumerate() {
            let request = TextRequest {
                text: StringId(id as u64 + 6),
                area,
                offset_x: 0.0,
                color: Color::WHITE,
                style: TextStyle::default(),
                options: TextOptions::default(),
                intrinsic_height: true,
            };
            let index = prepare(&mut paragraphs, &request, text, &mut fonts);
            let paragraph = paragraphs.get(index);
            assert_eq!(paragraph.layout_height, paragraph.rendered.as_ref().unwrap().layout_height, "{text:?}");
        }
    }

    #[test]
    fn control_glyphs_are_not_rasterized() {
        let font =
            Font::from_static(include_bytes!("../../../resources/fonts/Montserrat-Medium.ttf")).unwrap();
        let mut fonts =
            FontCache::new(vec![FontFace { id: FontId::default(), weight: 500, font }], 1024 * 1024);
        let mut paragraphs = ParagraphCache::new(1024 * 1024, 256);
        let area = LogicalRect { x: 0.0, y: 0.0, width: 384.0, height: 36.0 };
        let request = |text| TextRequest {
            text: StringId(text),
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle { size: 20.0, weight: 500, ..TextStyle::default() },
            options: TextOptions {
                wrap: TextWrap::Word,
                horizontal_align: HorizontalAlign::Center,
                ..TextOptions::default()
            },
            intrinsic_height: false,
        };
        let request = request(1);
        let multiline = prepare(&mut paragraphs, &request, "4 failed attempts\n", &mut fonts);
        let request = TextRequest { text: StringId(2), ..request };
        let single = prepare(&mut paragraphs, &request, "4 failed attempts", &mut fonts);
        let multiline = paragraphs.get(multiline).rendered.as_ref().unwrap();
        let single = paragraphs.get(single).rendered.as_ref().unwrap();
        assert_eq!(
            (multiline.x, multiline.y, multiline.width, multiline.height, &multiline.alpha),
            (single.x, single.y, single.width, single.height, &single.alpha),
        );
    }
}
