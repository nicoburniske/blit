use std::{
    collections::{hash_map::DefaultHasher, hash_map::RandomState},
    hash::{Hash, Hasher},
    num::NonZeroUsize,
};

use bullseye::{
    FontId, FontWeight, HorizontalAlign, TextOverflow, TextRequest, TextWrap, VerticalAlign,
};
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
    pub alpha: Box<[u8]>,
    text: Box<str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ParagraphKey {
    text_hash: u64,
    text_len: usize,
    width: i32,
    height: i32,
    font: FontId,
    size: u32,
    weight: FontWeight,
    wrap: TextWrap,
    overflow: TextOverflow,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
    max_lines: Option<u16>,
}

struct ParagraphWeight;

impl WeightScale<ParagraphKey, Paragraph> for ParagraphWeight {
    fn weight(&self, _: &ParagraphKey, paragraph: &Paragraph) -> usize {
        paragraph.alpha.len() + paragraph.text.len()
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
        let area = request.area.to_physical(scale_factor);
        let mut hasher = DefaultHasher::new();
        request.text.hash(&mut hasher);
        let key = ParagraphKey {
            text_hash: hasher.finish(),
            text_len: request.text.len(),
            width: area.width,
            height: area.height,
            font: request.style.font,
            size: (request.style.size * scale_factor).to_bits(),
            weight: request.style.weight,
            wrap: request.options.wrap,
            overflow: request.options.overflow,
            horizontal_align: request.options.horizontal_align,
            vertical_align: request.options.vertical_align,
            max_lines: request.options.max_lines,
        };
        let cached = self
            .cache
            .peek(&key)
            .is_some_and(|paragraph| paragraph.text.as_ref() == request.text);
        if cached {
            return Ok(self.cache.get(&key).unwrap());
        }

        let size = request.style.size * scale_factor;
        let line_height = fonts
            .font()
            .horizontal_line_metrics(size)
            .map_or(size, |metrics| metrics.new_line_size);
        let max_height = request
            .options
            .max_lines
            .map_or(area.height as f32, |lines| {
                (area.height as f32).min(line_height * lines as f32)
            });
        self.layout.reset(&LayoutSettings {
            x: 0.0,
            y: 0.0,
            max_width: (request.options.wrap != TextWrap::None).then_some(area.width as f32),
            max_height: Some(max_height),
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
            wrap_style: match request.options.wrap {
                TextWrap::Character => WrapStyle::Letter,
                TextWrap::None | TextWrap::Word => WrapStyle::Word,
            },
            ..LayoutSettings::default()
        });
        self.layout
            .append(&[fonts.font()], &FontTextStyle::new(request.text, size, 0));
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
        let mut left = area.width;
        let mut top = area.height;
        let mut right = 0;
        let mut bottom = 0;
        for glyph in glyphs {
            let x = (glyph.x + offset_x).round() as i32;
            let y = glyph.y.round() as i32;
            left = left.min(x.max(0).min(area.width));
            top = top.min(y.max(0).min(area.height));
            right = right.max((x + glyph.width as i32).max(0).min(area.width));
            bottom = bottom.max((y + glyph.height as i32).max(0).min(area.height));
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
            let x = (glyph.x + offset_x).round() as i32;
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
            alpha: alpha.into_boxed_slice(),
            text: request.text.into(),
        };
        match self.cache.put_with_weight(key, paragraph) {
            Ok(_) => Ok(self.cache.get(&key).unwrap()),
            Err((_, paragraph)) => Err(paragraph),
        }
    }
}
