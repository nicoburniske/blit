use std::mem::size_of;

use blit::{
    paint::{
        FontId, HorizontalAlign, TextLayoutRequest, TextOverflow, TextRequest, TextWrap,
        VerticalAlign,
    },
    resource::{StringId, TextSource},
};
use blit_cache::{DeferredCache, Scale};
use blit_font::{Layout, LayoutSettings};

use super::font::FontCache;

/// stores measured and rasterized paragraphs between frames
pub struct ParagraphCache {
    layouts: DeferredCache<LayoutKey, ParagraphLayout, LayoutScale>,
    rasters: DeferredCache<RasterKey, ParagraphRaster, RasterScale>,
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

struct RasterScale;

impl Scale<RasterKey, ParagraphRaster> for RasterScale {
    fn weight(&self, _key: &RasterKey, paragraph: &ParagraphRaster) -> usize {
        size_of::<ParagraphRaster>()
            + paragraph.alpha.len()
            + paragraph.carets.len() * size_of::<Caret>()
    }
}

impl ParagraphCache {
    pub fn new(capacity: usize, metric_cache_capacity: usize) -> Self {
        Self {
            layouts: DeferredCache::new(LayoutScale, capacity),
            rasters: DeferredCache::new(RasterScale, capacity),
            layout: Layout::with_metric_cache_capacity(metric_cache_capacity),
        }
    }

    pub fn measure(
        &mut self,
        key: LayoutKey,
        request: &TextLayoutRequest,
        text: &str,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> blit::geometry::LogicalSize {
        let Self {
            layouts, layout, ..
        } = self;
        let size = match layouts.get_or_insert(key, || {
            Self::layout(layout, request, text, scale_factor, fonts)
        }) {
            Ok((paragraph, _)) => paragraph.size,
            Err(paragraph) => paragraph.size,
        };
        blit::geometry::LogicalSize {
            width: size.width / scale_factor,
            height: size.height / scale_factor,
        }
    }

    /// gets and rasterizes an entry without evicting until the frame ends
    pub fn prepare(
        &mut self,
        key: RasterKey,
        request: &TextRequest,
        text: &str,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> usize {
        let Self {
            layouts,
            rasters,
            layout,
        } = self;
        let Ok((_, index)) = rasters.get_or_insert(key, || {
            let layout_key = LayoutKey::paint(request, scale_factor);
            let Ok((_, layout_index)) = layouts.get_or_insert(layout_key, || {
                Self::layout_for_paint(layout, request, text, scale_factor, fonts)
            }) else {
                panic!("paragraph cache capacity must fit a layout");
            };
            let paragraph = layouts.get_index(layout_index);
            Self::render(layout, paragraph, request, text, scale_factor, fonts)
        }) else {
            panic!("paragraph cache capacity must fit an entry");
        };
        index
    }

    pub fn get(&self, index: usize) -> &ParagraphRaster {
        self.rasters.get_index(index)
    }

    pub fn finish_frame(&mut self) {
        self.layouts.trim_to_weight();
        self.rasters.trim_to_weight();
    }

    pub fn retain_strings(&mut self, mut live: impl FnMut(StringId) -> bool) {
        self.layouts.retain(|(key, _)| match key.text {
            TextSource::Resource(string) => live(string),
            TextSource::Static(_) => true,
        });
        self.rasters.retain(|(key, _)| match key.layout.text {
            TextSource::Resource(string) => live(string),
            TextSource::Static(_) => true,
        });
    }

    pub fn raster_key(request: &TextRequest, scale_factor: f32) -> RasterKey {
        let area = request.area.to_physical(scale_factor);
        RasterKey {
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
}

pub struct ParagraphLayout {
    size: blit::geometry::LogicalSize,
    glyphs: Box<[blit_font::GlyphPosition]>,
    lines: Box<[blit_font::LinePosition]>,
}

/// raster and caret data independent of screen position
pub struct ParagraphRaster {
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
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
pub struct LayoutKey {
    text: TextSource,
    max_width: Option<i32>,
    font: FontId,
    size: u32,
    weight: u16,
    wrap: TextWrap,
    max_lines: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RasterKey {
    layout: LayoutKey,
    width: i32,
    height: i32,
    offset_x: i32,
    overflow: TextOverflow,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
}

impl ParagraphCache {
    fn layout(
        layout: &mut Layout,
        request: &TextLayoutRequest,
        text: &str,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> ParagraphLayout {
        let Some((_, font)) = fonts.font(request.style.font, request.style.weight) else {
            return ParagraphLayout {
                size: blit::geometry::LogicalSize {
                    width: 0.0,
                    height: request.style.size * scale_factor,
                },
                glyphs: Box::new([]),
                lines: Box::new([]),
            };
        };
        let size = request.style.size * scale_factor;
        layout.layout(
            font,
            text,
            size,
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
            size: Self::layout_size(layout, font, size, request.max_lines),
            glyphs: layout.glyphs().into(),
            lines: layout
                .lines()
                .map_or_else(Box::<[blit_font::LinePosition]>::default, Into::into),
        }
    }

    fn layout_for_paint(
        layout: &mut Layout,
        request: &TextRequest,
        text: &str,
        scale_factor: f32,
        fonts: &mut FontCache,
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
            text,
            scale_factor,
            fonts,
        )
    }

    fn layout_size(
        layout: &Layout,
        font: &blit_font::Font,
        size: f32,
        max_lines: Option<u16>,
    ) -> blit::geometry::LogicalSize {
        let lines = layout.lines().map_or(&[][..], |lines| lines);
        let lines = &lines[..lines.len().min(max_lines.map_or(usize::MAX, usize::from))];
        blit::geometry::LogicalSize {
            width: lines.iter().map(|line| line.width).fold(0.0, f32::max),
            height: lines.last().map_or_else(
                || font.horizontal_line_metrics(size).new_line_size.ceil(),
                |line| line.baseline_y - line.max_ascent + line.max_new_line_size,
            ),
        }
    }

    fn render(
        layout: &mut Layout,
        paragraph: &ParagraphLayout,
        request: &TextRequest,
        text: &str,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> ParagraphRaster {
        let area = request.area.to_physical(scale_factor);
        let Some((face, font)) = fonts.font(request.style.font, request.style.weight) else {
            return ParagraphRaster {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                alpha: Box::new([]),
                carets: Box::new([]),
            };
        };

        let size = request.style.size * scale_factor;
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
            let start = line.glyph_start.min(paragraph.glyphs.len());
            let end = line.glyph_end.saturating_add(1).min(paragraph.glyphs.len());
            paragraph.glyphs[start..end]
                .iter()
                .any(|glyph| glyph.x + glyph.width as f32 > area.width as f32)
        };
        let rendered = if request.options.overflow == TextOverflow::Ellipsis
            && (lines_truncated || line_overflows)
        {
            if visible_lines == 0 {
                Some(String::new())
            } else {
                let line = paragraph.lines[visible_lines - 1];
                let start = line.glyph_start.min(paragraph.glyphs.len());
                let end = line.glyph_end.saturating_add(1).min(paragraph.glyphs.len());
                let glyphs = &paragraph.glyphs[start..end];
                let available = area.width.max(0) as f32
                    - font.metrics(font.glyph_id('…'), size).advance_width.ceil();
                let end = glyphs
                    .iter()
                    .take_while(|glyph| glyph.x + glyph.width as f32 <= available)
                    .last()
                    .map_or_else(
                        || glyphs.first().map_or(0, |glyph| glyph.byte_offset),
                        |glyph| glyph.byte_offset + glyph.parent.len_utf8(),
                    );
                let mut rendered = text[..end].trim_end().to_owned();
                rendered.push('…');
                Some(rendered)
            }
        } else {
            None
        };

        if let Some(rendered) = &rendered {
            layout.layout(
                font,
                rendered,
                size,
                LayoutSettings {
                    max_width: (request.options.wrap != TextWrap::None)
                        .then_some((request.area.width.max(0.0) * scale_factor).ceil()),
                    wrap: request.options.wrap,
                    ..LayoutSettings::default()
                },
            );
        }
        let (glyphs, lines) = rendered.as_ref().map_or_else(
            || (&paragraph.glyphs[..], &paragraph.lines[..visible_lines]),
            |_| {
                (
                    layout.glyphs(),
                    layout.lines().map_or(&[][..], |lines| lines),
                )
            },
        );
        let natural_width = lines
            .iter()
            .flat_map(|line| &glyphs[line.glyph_start..=line.glyph_end])
            .map(|glyph| glyph.x + glyph.width as f32)
            .fold(0.0, f32::max);
        let unwrapped_offset = match request.options.horizontal_align {
            HorizontalAlign::Left => 0.0,
            HorizontalAlign::Center => (area.width as f32 - natural_width) / 2.0,
            HorizontalAlign::Right => area.width as f32 - natural_width,
        };
        let content_height = lines.last().map_or(0.0, |line| {
            line.baseline_y - line.max_ascent + line.max_new_line_size
        });
        let vertical_offset = match request.options.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Center => ((area.height as f32 - content_height) / 2.0).floor(),
            VerticalAlign::Bottom => (area.height as f32 - content_height).floor(),
        };
        let paint_offset_x = request.offset_x * scale_factor;
        let offsets = |line: &blit_font::LinePosition| {
            let horizontal = if request.options.wrap == TextWrap::None {
                unwrapped_offset
            } else {
                match request.options.horizontal_align {
                    HorizontalAlign::Left => 0.0,
                    HorizontalAlign::Center => ((area.width as f32 - line.width) / 2.0).floor(),
                    HorizontalAlign::Right => (area.width as f32 - line.width).floor(),
                }
            };
            (horizontal - paint_offset_x, vertical_offset)
        };
        let mut carets = Vec::with_capacity(glyphs.len() + 1);
        for line in lines {
            let start = line.glyph_start.min(glyphs.len());
            let end = line.glyph_end.saturating_add(1).min(glyphs.len());
            let (offset_x, offset_y) = offsets(line);
            let mut last = None;
            for glyph in &glyphs[start..end] {
                let x = glyph.pen_x + offset_x;
                let end = x + glyph.advance;
                last = Some((glyph.byte_offset + glyph.parent.len_utf8(), end));
                carets.push(Caret {
                    byte_offset: glyph.byte_offset,
                    x,
                    y: line.baseline_y - line.max_ascent + offset_y,
                    height: line.max_new_line_size,
                });
            }
            if let Some((byte_offset, x)) = last {
                carets.push(Caret {
                    byte_offset,
                    x,
                    y: line.baseline_y - line.max_ascent + offset_y,
                    height: line.max_new_line_size,
                });
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
            carets.push(Caret {
                byte_offset: 0,
                x: -paint_offset_x,
                y,
                height,
            });
        }
        let mut left = area.width;
        let bounds_height = area.height;
        let mut top = bounds_height;
        let mut right = 0;
        let mut bottom = 0;
        for line in lines {
            let (offset_x, offset_y) = offsets(line);
            for glyph in &glyphs[line.glyph_start..=line.glyph_end] {
                if glyph.char_data.is_control() || glyph.width == 0 || glyph.height == 0 {
                    continue;
                }
                let x = (glyph.x + offset_x).round() as i32;
                let y = (glyph.y + offset_y).round() as i32;
                left = left.min(x.max(0).min(area.width));
                top = top.min(y.max(0).min(bounds_height));
                right = right.max((x + glyph.width as i32).max(0).min(area.width));
                bottom = bottom.max((y + glyph.height as i32).max(0).min(bounds_height));
            }
        }
        let width = (right - left).max(0) as usize;
        let height = (bottom - top).max(0) as usize;
        let mut alpha = vec![0u8; width * height];
        for line in lines {
            let (offset_x, offset_y) = offsets(line);
            for glyph in &glyphs[line.glyph_start..=line.glyph_end] {
                if glyph.char_data.is_control() {
                    continue;
                }
                let cached = fonts.glyph(face, glyph.key);
                let cached = match &cached {
                    Ok(cached) => *cached,
                    Err(cached) => cached,
                };
                let x = (glyph.x + offset_x).round() as i32;
                let y = (glyph.y + offset_y).round() as i32;
                let source_left = (left - x).max(0) as usize;
                let source_top = (top - y).max(0) as usize;
                let source_right = (right - x).min(cached.metrics.width as i32).max(0) as usize;
                let source_bottom = (bottom - y).min(cached.metrics.height as i32).max(0) as usize;
                for source_y in source_top..source_bottom {
                    for source_x in source_left..source_right {
                        let destination_x = (x + source_x as i32 - left) as usize;
                        let destination_y = (y + source_y as i32 - top) as usize;
                        let source =
                            cached.alpha[source_y * cached.metrics.width + source_x] as u16;
                        let destination = &mut alpha[destination_y * width + destination_x];
                        *destination = (source + *destination as u16 * (255 - source) / 255) as u8;
                    }
                }
            }
        }
        ParagraphRaster {
            x: left,
            y: top,
            width,
            height,
            alpha: alpha.into_boxed_slice(),
            carets: carets.into_boxed_slice(),
        }
    }
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

#[cfg(test)]
mod tests {
    use blit::{
        color::Color,
        geometry::LogicalRect,
        paint::{
            FontId, HorizontalAlign, TextLayoutRequest, TextOptions, TextOverflow, TextRequest,
            TextStyle, TextWrap,
        },
    };

    use super::*;
    use crate::{Font, FontFace};

    fn prepare(
        paragraphs: &mut ParagraphCache,
        request: &TextRequest,
        text: &str,
        fonts: &mut FontCache,
    ) -> usize {
        paragraphs.prepare(
            ParagraphCache::raster_key(request, 1.0),
            request,
            text,
            1.0,
            fonts,
        )
    }

    #[test]
    fn measurement_and_render_share_layout() {
        let font = Font::from_static(include_bytes!(env!("BLIT_TEST_FONT"))).unwrap();
        let mut fonts = FontCache::new(
            vec![FontFace {
                id: FontId::default(),
                weight: 400,
                font,
            }],
            1024 * 1024,
        );
        let mut paragraphs = ParagraphCache::new(1024 * 1024, 256);
        let request = TextRequest {
            text: StringId(1).into(),
            area: LogicalRect {
                x: 0.5,
                y: 0.0,
                width: 100.0,
                height: 50.0,
            },
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions {
                wrap: TextWrap::Word,
                ..TextOptions::default()
            },
        };
        let layout_request = TextLayoutRequest {
            text: request.text,
            style: request.style,
            wrap: request.options.wrap,
            max_width: Some(request.area.width),
            max_lines: request.options.max_lines,
        };
        let key = ParagraphCache::layout_key(&layout_request, 1.0);
        assert_eq!(key, ParagraphCache::raster_key(&request, 1.0).layout);

        paragraphs.measure(key, &layout_request, "hello world", 1.0, &mut fonts);
        let glyphs = paragraphs.layouts.get(&key).unwrap().glyphs.as_ptr();
        prepare(&mut paragraphs, &request, "hello world", &mut fonts);

        assert_eq!(
            paragraphs.layouts.get(&key).unwrap().glyphs.as_ptr(),
            glyphs
        );
    }

    #[test]
    fn font_lookup_and_overflow_are_exact() {
        let font = Font::from_static(include_bytes!(env!("BLIT_TEST_FONT"))).unwrap();
        let mut fonts = FontCache::new(
            vec![FontFace {
                id: FontId::default(),
                weight: 400,
                font,
            }],
            1024 * 1024,
        );
        let mut paragraphs = ParagraphCache::new(1024 * 1024, 256);
        let area = LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };
        let request = TextRequest {
            text: StringId(1).into(),
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle {
                font: FontId(9),
                ..TextStyle::default()
            },
            options: TextOptions::default(),
        };
        let index = prepare(&mut paragraphs, &request, "missing", &mut fonts);
        let missing = paragraphs.get(index);
        assert_eq!((missing.width, missing.height), (0, 0));

        let request = TextRequest {
            text: StringId(2).into(),
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions {
                max_lines: Some(1),
                ..TextOptions::default()
            },
        };
        let one_line = prepare(&mut paragraphs, &request, "first\nsecond", &mut fonts);
        let request = TextRequest {
            text: StringId(3).into(),
            options: TextOptions::default(),
            ..request
        };
        let first = prepare(&mut paragraphs, &request, "first", &mut fonts);
        let one_line = paragraphs.get(one_line);
        let first = paragraphs.get(first);
        assert_eq!(
            (
                one_line.x,
                one_line.y,
                one_line.width,
                one_line.height,
                &one_line.alpha
            ),
            (first.x, first.y, first.width, first.height, &first.alpha),
        );

        let narrow = LogicalRect {
            width: 12.0,
            ..area
        };
        let request = TextRequest {
            text: StringId(4).into(),
            area: narrow,
            options: TextOptions {
                overflow: TextOverflow::Ellipsis,
                ..TextOptions::default()
            },
            ..request
        };
        let truncated = prepare(&mut paragraphs, &request, "WWWW", &mut fonts);
        let request = TextRequest {
            text: StringId(5).into(),
            options: TextOptions::default(),
            ..request
        };
        let ellipsis = prepare(&mut paragraphs, &request, "…", &mut fonts);
        let truncated = paragraphs.get(truncated);
        let ellipsis = paragraphs.get(ellipsis);
        assert_eq!(
            (
                truncated.x,
                truncated.y,
                truncated.width,
                truncated.height,
                &truncated.alpha
            ),
            (
                ellipsis.x,
                ellipsis.y,
                ellipsis.width,
                ellipsis.height,
                &ellipsis.alpha
            ),
        );
    }

    #[test]
    fn control_glyphs_are_not_rasterized() {
        let font = Font::from_static(include_bytes!(env!("BLIT_TEST_FONT"))).unwrap();
        let mut fonts = FontCache::new(
            vec![FontFace {
                id: FontId::default(),
                weight: 500,
                font,
            }],
            1024 * 1024,
        );
        let mut paragraphs = ParagraphCache::new(1024 * 1024, 256);
        let area = LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 384.0,
            height: 36.0,
        };
        let request = |text| TextRequest {
            text: StringId(text).into(),
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle {
                size: 20.0,
                weight: 500,
                ..TextStyle::default()
            },
            options: TextOptions {
                wrap: TextWrap::Word,
                horizontal_align: HorizontalAlign::Center,
                ..TextOptions::default()
            },
        };
        let request = request(1);
        let multiline = prepare(&mut paragraphs, &request, "4 failed attempts\n", &mut fonts);
        let request = TextRequest {
            text: StringId(2).into(),
            ..request
        };
        let single = prepare(&mut paragraphs, &request, "4 failed attempts", &mut fonts);
        let multiline = paragraphs.get(multiline);
        let single = paragraphs.get(single);
        assert_eq!(
            (
                multiline.x,
                multiline.y,
                multiline.width,
                multiline.height,
                &multiline.alpha
            ),
            (
                single.x,
                single.y,
                single.width,
                single.height,
                &single.alpha
            ),
        );
    }
}
