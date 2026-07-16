use std::{hash::Hash, mem::size_of};

use blit::paint::{HorizontalAlign, TextWrap, VerticalAlign};
use unicode_linebreak::{linebreaks, BreakOpportunity};

use crate::{Font, GlyphId, LineMetrics, Metrics, UnscaledMetrics};

const DEFAULT_METRIC_CACHE_CAPACITY: usize = 256;

pub struct Layout {
    glyphs: Vec<GlyphPosition>,
    lines: Vec<LinePosition>,
    metrics: Vec<Option<CachedMetrics>>,
    metric_cache_capacity: usize,
    height: f32,
}

impl Default for Layout {
    fn default() -> Self { Self::with_metric_cache_capacity(DEFAULT_METRIC_CACHE_CAPACITY) }
}

impl Layout {
    pub fn with_metric_cache_capacity(metric_cache_capacity: usize) -> Self {
        Self {
            glyphs: Vec::new(),
            lines: Vec::new(),
            metrics: Vec::new(),
            metric_cache_capacity,
            height: 0.0,
        }
    }

    pub fn layout(&mut self, font: &Font, text: &str, size: f32, settings: LayoutSettings) {
        self.glyphs.clear();
        self.lines.clear();
        self.height = 0.0;
        if text.is_empty() || !size.is_finite() || size <= 0.0 {
            return;
        }

        let face = font.face();
        let metrics = Font::line_metrics_from_face(&face, size);
        let metrics = LineMetrics {
            ascent: metrics.ascent.ceil(),
            descent: metrics.descent.ceil(),
            line_gap: metrics.line_gap.ceil(),
            new_line_size: metrics.new_line_size.ceil().max(0.0),
        };
        let max_width = settings.max_width.unwrap_or(f32::MAX).max(0.0);
        let mut breaks = linebreaks(text).peekable();
        let mut pen = 0.0;
        let mut line_start = 0;
        let mut line_start_pen = 0.0;
        let mut last_break = None;

        for (byte_offset, character) in text.char_indices() {
            let byte_end = byte_offset + character.len_utf8();
            let mut opportunity = None;
            while let Some(&(offset, next)) = breaks.peek() {
                if offset > byte_end {
                    break;
                }
                breaks.next();
                if offset == byte_end {
                    opportunity = Some(next);
                }
            }

            let char_data = CharacterData::new(character);
            let slot = if self.metric_cache_capacity == 0 {
                None
            } else {
                if self.metrics.is_empty() {
                    self.metrics.resize(self.metric_cache_capacity, None);
                }
                let hash = (character as usize)
                    .wrapping_mul(0x9E37_79B9usize)
                    .wrapping_add(font.id().wrapping_mul(0x85EB_CA6Busize));
                Some(hash % self.metrics.len())
            };
            let cached = slot
                .and_then(|slot| self.metrics[slot])
                .filter(|cached| cached.font == font.id() && cached.character == character)
                .unwrap_or_else(|| {
                    let glyph = face.glyph_index(character).map_or(GlyphId(0), |id| GlyphId(id.0));
                    let cached = CachedMetrics {
                        font: font.id(),
                        character,
                        glyph,
                        metrics: Font::unscaled_metrics_from_face(&face, glyph),
                    };
                    if let Some(slot) = slot {
                        self.metrics[slot] = Some(cached);
                    }
                    cached
                });
            let glyph_id = cached.glyph;
            let glyph_metrics = if char_data.is_control() {
                Metrics::default()
            } else {
                Font::scale_metrics(cached.metrics, face.units_per_em(), size)
            };
            let advance = glyph_metrics.advance_width.ceil();
            let glyph_index = self.glyphs.len();
            if settings.wrap != TextWrap::None
                && glyph_index > line_start
                && pen - line_start_pen + advance > max_width
            {
                let wrap_at = if settings.wrap == TextWrap::Word {
                    last_break.filter(|index| *index > line_start).unwrap_or(glyph_index)
                } else {
                    glyph_index
                };
                let wrap_pen = if wrap_at == glyph_index { pen } else { self.glyphs[wrap_at].pen_x };
                self.push_line(line_start, wrap_at, line_start_pen, wrap_pen, metrics);
                line_start = wrap_at;
                line_start_pen = wrap_pen;
                last_break = None;
            }

            self.glyphs.push(GlyphPosition {
                key: GlyphRasterConfig { glyph_id, size },
                parent: character,
                byte_offset,
                x: (pen + glyph_metrics.bounds.xmin).floor(),
                y: (-glyph_metrics.bounds.height - glyph_metrics.bounds.ymin).floor(),
                width: glyph_metrics.width,
                height: glyph_metrics.height,
                pen_x: pen,
                advance,
                char_data,
            });
            pen += advance;

            let hard_break = opportunity == Some(BreakOpportunity::Mandatory);
            if hard_break {
                self.push_line(line_start, self.glyphs.len(), line_start_pen, pen, metrics);
                line_start = self.glyphs.len();
                line_start_pen = pen;
                last_break = None;
            } else if settings.wrap == TextWrap::Character
                || settings.wrap == TextWrap::Word && opportunity == Some(BreakOpportunity::Allowed)
            {
                last_break = Some(self.glyphs.len());
            }
        }

        if line_start < self.glyphs.len() {
            self.push_line(line_start, self.glyphs.len(), line_start_pen, pen, metrics);
        }

        self.height = self.lines.len() as f32 * metrics.new_line_size;
        let vertical_align = match settings.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Center => 0.5,
            VerticalAlign::Bottom => 1.0,
        };
        let vertical_offset =
            settings.max_height.map_or(0.0, |height| ((height - self.height) * vertical_align).floor());
        let horizontal_align = match settings.horizontal_align {
            HorizontalAlign::Left => 0.0,
            HorizontalAlign::Center => 0.5,
            HorizontalAlign::Right => 1.0,
        };

        for (index, line) in self.lines.iter_mut().enumerate() {
            let baseline = vertical_offset + metrics.ascent + index as f32 * metrics.new_line_size;
            let horizontal_offset =
                settings.max_width.map_or(0.0, |width| ((width - line.width) * horizontal_align).floor());
            line.baseline_y = baseline;
            for glyph in &mut self.glyphs[line.glyph_start..=line.glyph_end] {
                let offset = horizontal_offset - line.start_x;
                glyph.x += offset;
                glyph.pen_x += offset;
                glyph.y += baseline;
            }
        }
    }

    pub fn glyphs(&self) -> &[GlyphPosition] { &self.glyphs }

    pub fn lines(&self) -> Option<&[LinePosition]> { (!self.lines.is_empty()).then_some(&self.lines) }

    pub fn height(&self) -> f32 { self.height }

    pub fn allocated_bytes(&self) -> usize {
        self.glyphs.capacity() * size_of::<GlyphPosition>()
            + self.lines.capacity() * size_of::<LinePosition>()
            + self.metrics.capacity() * size_of::<Option<CachedMetrics>>()
    }

    fn push_line(&mut self, start: usize, end: usize, start_x: f32, end_x: f32, metrics: LineMetrics) {
        if start == end {
            return;
        }
        self.lines.push(LinePosition {
            baseline_y: 0.0,
            max_ascent: metrics.ascent,
            min_descent: metrics.descent,
            max_new_line_size: metrics.new_line_size,
            glyph_start: start,
            glyph_end: end - 1,
            start_x,
            width: end_x - start_x,
        });
    }
}

#[derive(Clone, Copy)]
struct CachedMetrics {
    font: usize,
    character: char,
    glyph: GlyphId,
    metrics: UnscaledMetrics,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutSettings {
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
    pub wrap: TextWrap,
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphPosition {
    pub key: GlyphRasterConfig,
    pub parent: char,
    pub byte_offset: usize,
    pub x: f32,
    pub y: f32,
    pub width: usize,
    pub height: usize,
    pub pen_x: f32,
    pub advance: f32,
    pub char_data: CharacterData,
}

#[derive(Clone, Copy, Debug)]
pub struct LinePosition {
    pub baseline_y: f32,
    pub max_ascent: f32,
    pub min_descent: f32,
    pub max_new_line_size: f32,
    pub glyph_start: usize,
    pub glyph_end: usize,
    start_x: f32,
    width: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphRasterConfig {
    pub glyph_id: GlyphId,
    pub size: f32,
}

impl PartialEq for GlyphRasterConfig {
    fn eq(&self, other: &Self) -> bool {
        self.glyph_id == other.glyph_id && self.size.to_bits() == other.size.to_bits()
    }
}

impl Eq for GlyphRasterConfig {}

impl Hash for GlyphRasterConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.glyph_id.hash(state);
        self.size.to_bits().hash(state);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CharacterData {
    control: bool,
}

impl CharacterData {
    fn new(character: char) -> Self { Self { control: matches!(character, '\0'..='\u{1f}' | '\u{7f}') } }

    pub fn is_control(self) -> bool { self.control }
}
