use std::{
    collections::{hash_map::DefaultHasher, hash_map::RandomState},
    hash::{Hash, Hasher},
    mem::size_of,
    num::NonZeroUsize,
};

use blit::{FontId, HorizontalAlign, TextOverflow, TextRequest, TextWrap, VerticalAlign};
use clru::{CLruCache, CLruCacheConfig, WeightScale};
use fontdue::layout::{
    CoordinateSystem, HorizontalAlign as FontHorizontalAlign, Layout, LayoutSettings,
    TextStyle as FontTextStyle, VerticalAlign as FontVerticalAlign, WrapStyle,
};

use super::font::FontCache;

pub struct Paragraph {
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub layout_width: f32,
    pub layout_height: f32,
    pub alpha: Box<[u8]>,
    pub carets: Box<[Caret]>,
    text: Box<str>,
}

impl Paragraph {
    pub(super) fn matches(&self, text: &str) -> bool {
        self.text.as_ref() == text
    }
}

#[derive(Clone, Copy)]
pub struct Caret {
    pub byte_offset: usize,
    pub x: f32,
    pub y: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ParagraphKey {
    text_hash: u64,
    text_len: usize,
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

struct ParagraphWeight;

impl WeightScale<ParagraphKey, Paragraph> for ParagraphWeight {
    fn weight(&self, _: &ParagraphKey, paragraph: &Paragraph) -> usize {
        paragraph.alpha.len() + paragraph.text.len() + paragraph.carets.len() * size_of::<Caret>()
    }
}

type Cache = CLruCache<ParagraphKey, Paragraph, RandomState, ParagraphWeight>;

pub struct ParagraphCache {
    cache: Cache,
    layout: Layout,
}

impl ParagraphCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: CLruCache::with_config(
                CLruCacheConfig::new(
                    NonZeroUsize::new(capacity).expect("paragraph cache capacity must be non-zero"),
                )
                .with_scale(ParagraphWeight),
            ),
            layout: Layout::new(CoordinateSystem::PositiveYDown),
        }
    }

    pub fn get(
        &mut self,
        request: &TextRequest<'_>,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> Result<&Paragraph, Paragraph> {
        let key = Self::key(request, scale_factor);
        let area = request.area.to_physical(scale_factor);
        let cached = self
            .cache
            .peek(&key)
            .is_some_and(|paragraph| paragraph.text.as_ref() == request.text);
        if cached {
            return Ok(self.cache.get(&key).unwrap());
        }

        self.create(key, request, area, scale_factor, fonts)
    }

    pub fn take(
        &mut self,
        request: &TextRequest<'_>,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> (ParagraphKey, Paragraph) {
        let key = Self::key(request, scale_factor);
        let paragraph = self.get(request, scale_factor, fonts);
        let paragraph = match paragraph {
            Ok(_) => self.cache.pop(&key).unwrap(),
            Err(paragraph) => paragraph,
        };
        (key, paragraph)
    }

    pub fn restore(&mut self, key: ParagraphKey, paragraph: Paragraph) {
        let _ = self.cache.put_with_weight(key, paragraph);
    }

    pub(super) fn key(request: &TextRequest<'_>, scale_factor: f32) -> ParagraphKey {
        let area = request.area.to_physical(scale_factor);
        let mut hasher = DefaultHasher::new();
        request.text.hash(&mut hasher);
        ParagraphKey {
            text_hash: hasher.finish(),
            text_len: request.text.len(),
            width: area.width,
            height: if request.intrinsic_height {
                0
            } else {
                area.height
            },
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

    fn create(
        &mut self,
        key: ParagraphKey,
        request: &TextRequest<'_>,
        area: blit::PhysicalRect,
        scale_factor: f32,
        fonts: &mut FontCache,
    ) -> Result<&Paragraph, Paragraph> {
        let Some(font) = fonts.font(request.style.font, request.style.weight) else {
            let paragraph = Paragraph {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                layout_width: 0.0,
                layout_height: 0.0,
                alpha: Box::new([]),
                carets: Box::new([]),
                text: request.text.into(),
            };
            return match self.cache.put_with_weight(key, paragraph) {
                Ok(_) => Ok(self.cache.get(&key).unwrap()),
                Err((_, paragraph)) => Err(paragraph),
            };
        };

        let size = request.style.size * scale_factor;
        let max_width =
            (request.options.wrap != TextWrap::None).then_some(area.width.max(0) as f32);
        let wrap_style = match request.options.wrap {
            TextWrap::Character => WrapStyle::Letter,
            TextWrap::None | TextWrap::Word => WrapStyle::Word,
        };
        self.layout.reset(&LayoutSettings {
            x: 0.0,
            y: 0.0,
            max_width,
            max_height: None,
            horizontal_align: FontHorizontalAlign::Left,
            vertical_align: FontVerticalAlign::Top,
            wrap_style,
            ..LayoutSettings::default()
        });
        self.layout
            .append(&[font], &FontTextStyle::new(request.text, size, 0));

        let mut rendered = None;
        if let Some(lines) = self.layout.lines() {
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

            let glyphs = self.layout.glyphs();
            let lines_truncated = visible_lines < lines.len();
            let line_overflows = visible_lines != 0 && request.options.wrap == TextWrap::None && {
                let line = lines[visible_lines - 1];
                let start = line.glyph_start.min(glyphs.len());
                let end = line.glyph_end.saturating_add(1).min(glyphs.len());
                glyphs[start..end]
                    .iter()
                    .any(|glyph| glyph.x + glyph.width as f32 > area.width as f32)
            };

            if visible_lines == 0 && !lines.is_empty() {
                rendered = Some(String::new());
            } else if visible_lines != 0 && (lines_truncated || line_overflows) {
                let line = lines[visible_lines - 1];
                let start = line.glyph_start.min(glyphs.len());
                let end = line.glyph_end.saturating_add(1).min(glyphs.len());
                let glyphs = &glyphs[start..end];
                let mut end = glyphs
                    .last()
                    .map_or(0, |glyph| glyph.byte_offset + glyph.parent.len_utf8());
                if request.options.overflow == TextOverflow::Ellipsis {
                    let available =
                        area.width.max(0) as f32 - font.metrics('…', size).advance_width.ceil();
                    end = glyphs
                        .iter()
                        .take_while(|glyph| glyph.x + glyph.width as f32 <= available)
                        .last()
                        .map_or_else(
                            || glyphs.first().map_or(0, |glyph| glyph.byte_offset),
                            |glyph| glyph.byte_offset + glyph.parent.len_utf8(),
                        );
                    let mut text = request.text[..end].trim_end().to_owned();
                    text.push('…');
                    rendered = Some(text);
                } else {
                    rendered = Some(
                        request.text[..end]
                            .trim_end_matches(['\r', '\n'])
                            .to_owned(),
                    );
                }
            }
        }

        self.layout.reset(&LayoutSettings {
            x: 0.0,
            y: 0.0,
            max_width,
            max_height: (!request.intrinsic_height).then_some(area.height.max(0) as f32),
            horizontal_align: match request.options.horizontal_align {
                HorizontalAlign::Left => FontHorizontalAlign::Left,
                HorizontalAlign::Center => FontHorizontalAlign::Center,
                HorizontalAlign::Right => FontHorizontalAlign::Right,
            },
            vertical_align: match request.options.vertical_align {
                VerticalAlign::Top => FontVerticalAlign::Top,
                VerticalAlign::Center => FontVerticalAlign::Middle,
                VerticalAlign::Bottom => FontVerticalAlign::Bottom,
            },
            wrap_style,
            ..LayoutSettings::default()
        });
        self.layout.append(
            &[font],
            &FontTextStyle::new(rendered.as_deref().unwrap_or(request.text), size, 0),
        );
        let layout_height = self.layout.lines().map_or_else(
            || {
                font.horizontal_line_metrics(size)
                    .map_or(size, |line| line.new_line_size.ceil())
            },
            |_| self.layout.height(),
        );
        let glyphs = self.layout.glyphs();
        let natural_width = glyphs
            .iter()
            .map(|glyph| glyph.x + glyph.width as f32)
            .fold(0.0, f32::max);
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
        if let Some(lines) = self.layout.lines() {
            for line in lines {
                let start = line.glyph_start.min(glyphs.len());
                let end = line.glyph_end.saturating_add(1).min(glyphs.len());
                let mut line_left = f32::INFINITY;
                let mut line_right = f32::NEG_INFINITY;
                let mut last = None;
                for glyph in &glyphs[start..end] {
                    let metrics = if glyph.char_data.is_control() {
                        None
                    } else {
                        Some(font.metrics_indexed(glyph.key.glyph_index, size))
                    };
                    let x = glyph.x - metrics.as_ref().map_or(0.0, |metrics| metrics.xmin as f32)
                        + paint_offset_x;
                    let end = x + metrics
                        .as_ref()
                        .map_or(0.0, |metrics| metrics.advance_width.ceil());
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
            let height = line.map_or(size, |line| line.new_line_size.ceil());
            let y = match request.options.vertical_align {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Center => (area.height as f32 - height) / 2.0,
                VerticalAlign::Bottom => area.height as f32 - height,
            };
            carets.push(Caret {
                byte_offset: 0,
                x: paint_offset_x,
                y,
                height,
            });
        }
        let mut left = area.width;
        let bounds_height = if request.intrinsic_height {
            i32::MAX
        } else {
            area.height
        };
        let mut top = bounds_height;
        let mut right = 0;
        let mut bottom = 0;
        for glyph in glyphs {
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
            let cached = fonts.glyph(glyph.key);
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
        let paragraph = Paragraph {
            x: left,
            y: top,
            width,
            height,
            layout_width,
            layout_height,
            alpha: alpha.into_boxed_slice(),
            carets: carets.into_boxed_slice(),
            text: request.text.into(),
        };
        match self.cache.put_with_weight(key, paragraph) {
            Ok(_) => Ok(self.cache.get(&key).unwrap()),
            Err((_, paragraph)) => Err(paragraph),
        }
    }
}

#[cfg(test)]
mod tests {
    use blit::{Color, FontId, LogicalRect, TextOptions, TextOverflow, TextRequest, TextStyle};

    use super::*;
    use crate::{Font, FontFace, FontSettings};

    #[test]
    fn font_lookup_and_overflow_are_exact() {
        let font = Font::from_bytes(
            include_bytes!("../../../example/assets/Montserrat-Regular.ttf") as &[u8],
            FontSettings::default(),
        )
        .unwrap();
        let mut fonts = FontCache::new(
            vec![FontFace {
                id: FontId::default(),
                weight: 400,
                font,
            }],
            1024 * 1024,
        );
        let mut paragraphs = ParagraphCache::new(1024 * 1024);
        let area = LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };
        let missing = paragraphs.get(
            &TextRequest {
                text: "missing",
                area,
                offset_x: 0.0,
                color: Color::WHITE,
                style: TextStyle {
                    font: FontId(9),
                    ..TextStyle::default()
                },
                options: TextOptions::default(),
                intrinsic_height: false,
            },
            1.0,
            &mut fonts,
        );
        let missing = match &missing {
            Ok(paragraph) => *paragraph,
            Err(paragraph) => paragraph,
        };
        assert_eq!((missing.width, missing.height), (0, 0));

        let snapshot = |paragraph: Result<&Paragraph, Paragraph>| {
            let paragraph = match &paragraph {
                Ok(paragraph) => *paragraph,
                Err(paragraph) => paragraph,
            };
            (
                paragraph.x,
                paragraph.y,
                paragraph.width,
                paragraph.height,
                paragraph.alpha.to_vec(),
            )
        };
        let one_line = snapshot(paragraphs.get(
            &TextRequest {
                text: "first\nsecond",
                area,
                offset_x: 0.0,
                color: Color::WHITE,
                style: TextStyle::default(),
                options: TextOptions {
                    max_lines: Some(1),
                    ..TextOptions::default()
                },
                intrinsic_height: false,
            },
            1.0,
            &mut fonts,
        ));
        let first = snapshot(paragraphs.get(
            &TextRequest {
                text: "first",
                area,
                offset_x: 0.0,
                color: Color::WHITE,
                style: TextStyle::default(),
                options: TextOptions::default(),
                intrinsic_height: false,
            },
            1.0,
            &mut fonts,
        ));
        assert_eq!(one_line, first);

        let narrow = LogicalRect {
            width: 12.0,
            ..area
        };
        let truncated = snapshot(paragraphs.get(
            &TextRequest {
                text: "WWWW",
                area: narrow,
                offset_x: 0.0,
                color: Color::WHITE,
                style: TextStyle::default(),
                options: TextOptions {
                    overflow: TextOverflow::Ellipsis,
                    ..TextOptions::default()
                },
                intrinsic_height: false,
            },
            1.0,
            &mut fonts,
        ));
        let ellipsis = snapshot(paragraphs.get(
            &TextRequest {
                text: "…",
                area: narrow,
                offset_x: 0.0,
                color: Color::WHITE,
                style: TextStyle::default(),
                options: TextOptions::default(),
                intrinsic_height: false,
            },
            1.0,
            &mut fonts,
        ));
        assert_eq!(truncated, ellipsis);
    }
}
