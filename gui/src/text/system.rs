use std::{
    fmt::Write as _,
    hash::{BuildHasher, Hash, Hasher},
    mem::size_of,
};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use blit_arena::Arena;
use blit_cache::{DeferredCache, Equivalent, Scale};
use blit_text::{
    Caret, Family, FontError, FontSettings, FontStackId, GlyphImage, GlyphRequest, LayoutRequest, OpticalSize,
    PreparedGlyph, TextLayout, TextLayoutEngine,
};

use super::{
    FontId, HorizontalAlign, Span, TextLayoutRequest, TextOptions, TextOverflow, TextRun, TextSource, TextStyle,
    TextWrap, VerticalAlign,
};
use crate::paint::TextRequest;

pub struct TextSystem {
    engine: Box<dyn ErasedTextEngine>,
    fonts: Box<[ConfiguredFont]>,
    sources: DeferredCache<SourceKey, CachedSource, SourceScale>,
    layouts: DeferredCache<LayoutKey, CachedLayout, LayoutScale>,
    formatted_text: String,
    next_source: u32,
}

impl TextSystem {
    pub fn new<E: TextLayoutEngine>(config: TextConfig, mut engine: E) -> Result<Self, FontError> {
        let mut fonts = Vec::new();
        for configured in config.fonts {
            if fonts.iter().any(|font: &ConfiguredFont| font.id == configured.id) {
                return Err(FontError::InvalidData);
            }
            let font = engine.register_stack(&configured.families)?;
            fonts.push(ConfiguredFont {
                id: configured.id,
                font,
            });
        }
        Ok(Self {
            engine: Box::new(ErasedEngine {
                engine,
                shapes: Vec::new(),
                glyphs: hashbrown::HashTable::new(),
                glyph_keys: Arena::new(),
                hash_builder: Default::default(),
            }),
            fonts: fonts.into_boxed_slice(),
            sources: DeferredCache::new(SourceScale, config.text_cache_capacity),
            layouts: DeferredCache::new(LayoutScale, config.layout_cache_capacity),
            formatted_text: String::new(),
            next_source: 1,
        })
    }

    /// registers geometry using the font attributes in style
    pub fn text_run<'a>(&mut self, source: impl Into<TextSource<'a>>, style: TextStyle) -> TextRun {
        let arguments = match source.into() {
            TextSource::Borrowed(text) => return self.rich_text(text, &[], style),
            TextSource::Formatted(arguments) => arguments,
        };
        let mut text = std::mem::take(&mut self.formatted_text);
        text.clear();
        text.write_fmt(arguments).expect("text formatting failed");
        let run = self.rich_text(&text, &[], style);
        self.formatted_text = text;
        run
    }

    /// registers geometry using span ranges and font attributes
    pub fn rich_text(&mut self, text: &str, spans: &[Span<'_>], style: TextStyle) -> TextRun {
        let Some(font) = self.fonts.iter().find(|font| font.id == style.font) else {
            return TextRun::default();
        };
        let default = [Span::new(0..text.len())];
        let spans = if spans.is_empty() { &default } else { spans };
        let mut end = 0;
        for span in spans {
            assert_eq!(span.range.start, end, "rich text spans must be contiguous");
            assert!(span.range.end >= end, "rich text spans must be ordered");
            assert!(
                text.is_char_boundary(span.range.end),
                "rich text spans must lie on character boundaries"
            );
            end = span.range.end;
            if span
                .style
                .font
                .is_some_and(|id| !self.fonts.iter().any(|font| font.id == id))
            {
                return TextRun::default();
            }
        }
        assert_eq!(end, text.len(), "rich text spans must cover the text");
        let query = SourceQuery {
            text,
            spans,
            style: blit_text::TextStyle {
                font: font.font,
                size: style.size,
                weight: style.weight,
                stretch: style.stretch,
                style: style.style,
                settings: style.settings,
            },
            fonts: &self.fonts,
        };
        let mut inserted = false;
        let (_, source_index) = self.sources.get_or_insert(query, |query| {
            inserted = true;
            let spans = if query.spans.len() == 1 && query.resolve(&query.spans[0]) == query.style {
                Box::default()
            } else {
                let mut resolved = Vec::with_capacity(query.spans.len());
                for span in query.spans {
                    resolved.push(CachedSpan {
                        range: span.range.clone(),
                        style: CachedStyle::new(query.resolve(span)),
                    });
                }
                resolved.into_boxed_slice()
            };
            (
                SourceKey {
                    text: query.text.into(),
                    style: CachedStyle::new(query.style),
                    spans,
                },
                CachedSource {
                    id: u64::from(self.next_source) << 32,
                    shape: None,
                },
            )
        });
        if inserted {
            let slot = u32::try_from(source_index + 1).expect("too many cached text sources");
            self.sources
                .update_index(source_index, |source| source.id |= u64::from(slot));
            self.next_source = self.next_source.checked_add(1).expect("too many text sources");
        }
        TextRun(self.sources.get_index(source_index).id)
    }

    pub fn paint_layout(&mut self, area: LogicalRect, request: &TextRequest<'_>) -> ResolvedTextLayout<'_> {
        let (layout_request, placement) = paint_request(area, request);
        let (index, shape) = self.layout(request.text, layout_request);
        let cached = self.layouts.get_index(index);
        ResolvedTextLayout {
            layout: &cached.layout,
            engine: self.engine.as_mut(),
            shape,
            placement,
        }
    }

    pub fn remove_glyph(&mut self, id: GlyphImageId) {
        self.engine.remove_glyph(id);
    }

    pub fn measure(&mut self, request: &TextLayoutRequest) -> LogicalSize {
        let wrap = match request.wrap {
            TextWrap::None => blit_text::TextWrap::None,
            TextWrap::Word => blit_text::TextWrap::Word,
            TextWrap::Character => blit_text::TextWrap::Character,
        };
        let (index, _) = self.layout(
            request.text,
            LayoutRequest {
                max_width: if wrap == blit_text::TextWrap::None {
                    None
                } else {
                    request.max_width
                },
                max_height: None,
                max_lines: request.max_lines,
                wrap,
                overflow: blit_text::TextOverflow::Clip,
            },
        );
        self.layouts.get_index(index).layout.size
    }

    pub fn offset_at_position(
        &mut self,
        area: LogicalRect,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize {
        let (layout_request, placement) = paint_request(area, request);
        let (layout_index, shape_index) = self.layout(request.text, layout_request);
        let position = LogicalPoint {
            x: position.x - area.x + request.offset_x,
            y: position.y - area.y,
        };
        let Some(line) = ({
            let layout = &self.layouts.get_index(layout_index).layout;
            layout
                .lines
                .iter()
                .enumerate()
                .min_by(|(left_index, left), (right_index, right)| {
                    let distance = |index, line: &blit_text::LayoutLine| {
                        let y = line.bounds.y + placement_offset(placement, layout, index).y;
                        if position.y < y {
                            y - position.y
                        } else if position.y > y + line.bounds.height {
                            position.y - y - line.bounds.height
                        } else {
                            0.0
                        }
                    };
                    distance(*left_index, left).total_cmp(&distance(*right_index, right))
                })
                .map(|(index, _)| index)
        }) else {
            return 0;
        };
        self.ensure_carets(shape_index, layout_index, layout_request, line);
        let cached = self.layouts.get_index(layout_index);
        let offset = placement_offset(placement, &cached.layout, line);
        cached
            .carets
            .iter()
            .find(|cached| cached.line as usize == line)
            .unwrap()
            .carets
            .iter()
            .min_by(|left, right| {
                (left.position.x + offset.x - position.x)
                    .abs()
                    .total_cmp(&(right.position.x + offset.x - position.x).abs())
            })
            .map_or(0, |caret| caret.byte_offset as usize)
    }

    pub fn cursor_rect(
        &mut self,
        area: LogicalRect,
        request: &TextRequest<'_>,
        byte_offset: usize,
        width: f32,
    ) -> LogicalRect {
        let (layout_request, placement) = paint_request(area, request);
        let (layout_index, shape_index) = self.layout(request.text, layout_request);
        let Some(line) = ({
            let layout = &self.layouts.get_index(layout_index).layout;
            layout
                .lines
                .iter()
                .enumerate()
                .min_by_key(|(_, line)| {
                    let start = line.text.start as usize;
                    let end = line.text.end as usize;
                    if byte_offset < start {
                        start - byte_offset
                    } else {
                        byte_offset.saturating_sub(end)
                    }
                })
                .map(|(index, _)| index)
        }) else {
            return LogicalRect {
                x: area.x - request.offset_x,
                y: area.y,
                width,
                height: 0.0,
            };
        };
        self.ensure_carets(shape_index, layout_index, layout_request, line);
        let cached = self.layouts.get_index(layout_index);
        let caret = cached
            .carets
            .iter()
            .find(|cached| cached.line as usize == line)
            .unwrap()
            .carets
            .iter()
            .min_by_key(|caret| (caret.byte_offset as usize).abs_diff(byte_offset));
        let offset = placement_offset(placement, &cached.layout, line);
        LogicalRect {
            x: area.x + caret.map_or(0.0, |caret| caret.position.x) + offset.x - request.offset_x,
            y: area.y + caret.map_or(0.0, |caret| caret.position.y) + offset.y,
            width,
            height: caret.map_or(0.0, |caret| caret.height),
        }
    }

    pub fn finish_frame(&mut self) {
        self.layouts.trim_to_weight();
        self.sources.trim_to_weight_if(|_, source| {
            if source.shape.is_some() {
                let index = (source.id as u32 - 1) as usize;
                self.engine.remove_shape(index);
            }
            true
        });
    }

    fn layout(&mut self, text: TextRun, request: LayoutRequest) -> (usize, usize) {
        let id = text.0;
        let index = (id as u32).checked_sub(1).expect("invalid text") as usize;
        let unshaped = self.sources.update_index(index, |cached| {
            assert_eq!(cached.id, id, "expired text source");
            cached.shape.is_none()
        });
        if unshaped {
            let cached = self.sources.get_key_index(index);
            let one;
            let many;
            let spans = if cached.spans.is_empty() {
                one = blit_text::TextSpan {
                    range: 0..cached.text.len(),
                    style: cached.style.as_style(),
                };
                std::slice::from_ref(&one)
            } else {
                many = cached
                    .spans
                    .iter()
                    .map(|span| blit_text::TextSpan {
                        range: span.range.clone(),
                        style: span.style.as_style(),
                    })
                    .collect::<Vec<_>>();
                many.as_slice()
            };
            let shape = self.engine.shape(
                index,
                blit_text::Text {
                    text: &cached.text,
                    style: cached.style.as_style(),
                    spans,
                },
            );
            self.sources.update_index(index, |cached| cached.shape = Some(shape));
        }
        let key = LayoutKey {
            text: id,
            max_width: request.max_width.map(f32::to_bits),
            max_height: request.max_height.map(f32::to_bits),
            max_lines: request.max_lines,
            wrap: request.wrap,
            overflow: request.overflow,
        };
        let text = &self.sources.get_key_index(index).text;
        let (_, layout) = self.layouts.get_or_insert(key, |_| {
            let layout = self.engine.layout(ShapedText { index, text }, request);
            (
                key,
                CachedLayout {
                    layout,
                    carets: Vec::new(),
                },
            )
        });
        (layout, index)
    }

    fn ensure_carets(&mut self, index: usize, layout: usize, request: LayoutRequest, line: usize) {
        if self
            .layouts
            .get_index(layout)
            .carets
            .iter()
            .any(|cached| cached.line as usize == line)
        {
            return;
        }
        let text = &self.sources.get_key_index(index).text;
        let carets = self.engine.carets(ShapedText { index, text }, request, line);
        self.layouts.update_index(layout, |cached| {
            cached.carets.push(LineCarets {
                line: u32::try_from(line).expect("too many text lines"),
                carets,
            });
        });
    }
}

pub struct TextConfig {
    pub fonts: Vec<FontStack>,
    /// maximum retained source and shaping bytes
    pub text_cache_capacity: usize,
    pub layout_cache_capacity: usize,
}

impl TextConfig {
    pub fn new() -> Self {
        Self {
            fonts: Vec::new(),
            text_cache_capacity: 1024 * 1024,
            layout_cache_capacity: 1024 * 1024,
        }
    }

    pub fn font(mut self, id: FontId, families: impl IntoIterator<Item = Family>) -> Self {
        self.fonts.push(FontStack {
            id,
            families: families.into_iter().collect(),
        });
        self
    }
}

impl Default for TextConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FontStack {
    pub id: FontId,
    pub families: Vec<Family>,
}

pub struct ResolvedTextLayout<'a> {
    pub layout: &'a TextLayout,
    engine: &'a mut dyn ErasedTextEngine,
    shape: usize,
    placement: Placement,
}

impl ResolvedTextLayout<'_> {
    pub fn prepare_glyph(&mut self, request: GlyphRequest) -> PreparedGlyph<GlyphImageId> {
        self.engine.prepare_glyph(self.shape, request)
    }

    pub fn rasterize_glyph(&mut self, id: GlyphImageId) -> Result<GlyphImage, FontError> {
        let image = self.engine.rasterize_glyph(id)?;
        let channels = if image.pixels.is_color() { 4 } else { 1 };
        if usize::from(image.metrics.width) * usize::from(image.metrics.height) * channels != image.pixels.data().len()
        {
            return Err(FontError::InvalidData);
        }
        Ok(image)
    }

    pub fn remove_glyph(&mut self, id: GlyphImageId) {
        self.engine.remove_glyph(id);
    }

    pub fn line_offset(&self, line: usize) -> LogicalPoint {
        placement_offset(self.placement, self.layout, line)
    }
}

/// identifies cached glyph pixels
///
/// ids are owned by a single renderer, and must not be shared between renderers
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphImageId(u64);

struct ConfiguredFont {
    id: FontId,
    font: FontStackId,
}

struct SourceKey {
    text: Box<str>,
    style: CachedStyle,
    // styled sections of rich text with base style defaults already applied
    // empty means plain text using only the base style
    spans: Box<[CachedSpan]>,
}

struct CachedSource {
    id: u64,
    shape: Option<usize>,
}

struct CachedSpan {
    range: std::ops::Range<usize>,
    style: CachedStyle,
}

struct CachedStyle {
    font: FontStackId,
    size: f32,
    weight: u16,
    stretch: u16,
    style: blit_text::FontStyle,
    features: Box<[([u8; 4], u32)]>,
    variations: Box<[([u8; 4], f32)]>,
    language: Option<Box<str>>,
    optical_size: OpticalSize,
}

impl CachedStyle {
    fn new(style: blit_text::TextStyle<'_>) -> Self {
        Self {
            font: style.font,
            size: style.size,
            weight: style.weight,
            stretch: style.stretch,
            style: style.style,
            features: style.settings.features.into(),
            variations: style.settings.variations.into(),
            language: style.settings.language.map(Into::into),
            optical_size: style.settings.optical_size,
        }
    }

    fn as_style(&self) -> blit_text::TextStyle<'_> {
        blit_text::TextStyle {
            font: self.font,
            size: self.size,
            weight: self.weight,
            stretch: self.stretch,
            style: self.style,
            settings: FontSettings {
                features: &self.features,
                variations: &self.variations,
                language: self.language.as_deref(),
                optical_size: self.optical_size,
            },
        }
    }

    fn heap_weight(&self) -> usize {
        self.features.len() * size_of::<([u8; 4], u32)>()
            + self.variations.len() * size_of::<([u8; 4], f32)>()
            + self.language.as_ref().map_or(0, |language| language.len())
    }
}

struct SourceQuery<'a> {
    text: &'a str,
    spans: &'a [Span<'a>],
    style: blit_text::TextStyle<'a>,
    fonts: &'a [ConfiguredFont],
}

impl<'a> SourceQuery<'a> {
    fn resolve(&self, span: &Span<'a>) -> blit_text::TextStyle<'a> {
        blit_text::TextStyle {
            font: span.style.font.map_or(self.style.font, |id| {
                self.fonts.iter().find(|font| font.id == id).unwrap().font
            }),
            size: span.style.size.unwrap_or(self.style.size),
            weight: span.style.weight.unwrap_or(self.style.weight),
            stretch: span.style.stretch.unwrap_or(self.style.stretch),
            style: span.style.style.unwrap_or(self.style.style),
            settings: span.style.settings.unwrap_or(self.style.settings),
        }
    }
}

impl Hash for SourceQuery<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_style(self.style, state);
        for span in self.spans {
            self.text[span.range.clone()].hash(state);
            hash_style(self.resolve(span), state);
        }
    }
}

impl Equivalent<SourceKey> for SourceQuery<'_> {
    fn equivalent(&self, key: &SourceKey) -> bool {
        if key.text.as_ref() != self.text || key.style.as_style() != self.style {
            return false;
        }
        if key.spans.is_empty() {
            self.spans.len() == 1 && self.resolve(&self.spans[0]) == self.style
        } else {
            key.spans.len() == self.spans.len()
                && key.spans.iter().zip(self.spans).all(|(cached_span, span)| {
                    cached_span.range == span.range && cached_span.style.as_style() == self.resolve(span)
                })
        }
    }
}

impl Hash for SourceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_style(self.style.as_style(), state);
        if self.spans.is_empty() {
            self.text.hash(state);
            hash_style(self.style.as_style(), state);
        } else {
            for span in &self.spans {
                self.text[span.range.clone()].hash(state);
                hash_style(span.style.as_style(), state);
            }
        }
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct LayoutKey {
    text: u64,
    max_width: Option<u32>,
    max_height: Option<u32>,
    max_lines: Option<u16>,
    wrap: blit_text::TextWrap,
    overflow: blit_text::TextOverflow,
}

struct CachedLayout {
    layout: TextLayout,
    carets: Vec<LineCarets>,
}

struct LineCarets {
    line: u32,
    carets: Box<[Caret]>,
}

#[derive(Clone, Copy)]
struct Placement {
    width: f32,
    height: f32,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
}

trait ErasedTextEngine {
    fn prepare_glyph(&mut self, shape: usize, request: GlyphRequest) -> PreparedGlyph<GlyphImageId>;

    fn rasterize_glyph(&mut self, id: GlyphImageId) -> Result<GlyphImage, FontError>;

    fn remove_glyph(&mut self, id: GlyphImageId);

    fn shape(&mut self, index: usize, text: blit_text::Text<'_>) -> usize;

    fn layout(&mut self, text: ShapedText<'_>, request: LayoutRequest) -> TextLayout;

    fn carets(&mut self, text: ShapedText<'_>, request: LayoutRequest, line: usize) -> Box<[Caret]>;

    fn remove_shape(&mut self, index: usize);
}

struct ShapedText<'a> {
    index: usize,
    text: &'a str,
}

struct ErasedEngine<E: TextLayoutEngine> {
    engine: E,
    shapes: Vec<Option<E::Shape>>,
    glyphs: hashbrown::HashTable<GlyphImageId>,
    glyph_keys: Arena<E::GlyphKey>,
    hash_builder: hashbrown::DefaultHashBuilder,
}

impl<E: TextLayoutEngine> ErasedTextEngine for ErasedEngine<E> {
    fn prepare_glyph(&mut self, shape: usize, request: GlyphRequest) -> PreparedGlyph<GlyphImageId> {
        let prepared = self.engine.prepare_glyph(self.shapes[shape].as_ref().unwrap(), request);
        let hash = self.hash_builder.hash_one(&prepared.key);
        let id = if let Some(id) = self
            .glyphs
            .find(hash, |id| self.glyph_keys.get(id.0.into()).unwrap() == &prepared.key)
        {
            *id
        } else {
            let id = GlyphImageId(self.glyph_keys.insert(prepared.key).into());
            self.glyphs.insert_unique(hash, id, |id| {
                self.hash_builder.hash_one(self.glyph_keys.get(id.0.into()).unwrap())
            });
            id
        };
        PreparedGlyph {
            key: id,
            origin: prepared.origin,
        }
    }

    fn rasterize_glyph(&mut self, id: GlyphImageId) -> Result<GlyphImage, FontError> {
        self.engine
            .rasterize_glyph(self.glyph_keys.get(id.0.into()).expect("expired glyph image"))
    }

    fn remove_glyph(&mut self, id: GlyphImageId) {
        let key = self.glyph_keys.remove(id.0.into()).expect("expired glyph image");
        let hash = self.hash_builder.hash_one(&key);
        self.glyphs
            .find_entry(hash, |candidate| *candidate == id)
            .expect("glyph table missing live entry")
            .remove();
    }

    fn shape(&mut self, index: usize, text: blit_text::Text<'_>) -> usize {
        if self.shapes.len() <= index {
            self.shapes.resize_with(index + 1, || None);
        }
        let (shape, heap_weight) = self.engine.shape(text);
        assert!(self.shapes[index].replace(shape).is_none());
        size_of::<E::Shape>() + heap_weight
    }

    fn layout(&mut self, ShapedText { index, text }: ShapedText<'_>, request: LayoutRequest) -> TextLayout {
        self.engine.layout(self.shapes[index].as_mut().unwrap(), text, request)
    }

    fn carets(
        &mut self,
        ShapedText { index, text }: ShapedText<'_>,
        request: LayoutRequest,
        line: usize,
    ) -> Box<[Caret]> {
        self.engine
            .carets(self.shapes[index].as_mut().unwrap(), text, request, line)
    }

    fn remove_shape(&mut self, index: usize) {
        self.shapes[index] = None;
    }
}

struct SourceScale;

impl Scale<SourceKey, CachedSource> for SourceScale {
    fn weight(&self, key: &SourceKey, source: &CachedSource) -> usize {
        let spans = key.spans.len() * size_of::<CachedSpan>()
            + key.spans.iter().map(|span| span.style.heap_weight()).sum::<usize>();
        size_of::<SourceKey>()
            + size_of::<CachedSource>()
            + key.text.len()
            + key.style.heap_weight()
            + spans
            + source.shape.unwrap_or(0)
    }
}

struct LayoutScale;

impl Scale<LayoutKey, CachedLayout> for LayoutScale {
    fn weight(&self, _key: &LayoutKey, cached: &CachedLayout) -> usize {
        size_of::<LayoutKey>()
            + size_of::<CachedLayout>()
            + cached.layout.glyphs.len() * size_of::<blit_text::Glyph>()
            + cached.layout.runs.len() * size_of::<blit_text::LayoutRun>()
            + cached.layout.lines.len() * size_of::<blit_text::LayoutLine>()
            + cached.carets.capacity() * size_of::<LineCarets>()
            + cached
                .carets
                .iter()
                .map(|line| line.carets.len() * size_of::<Caret>())
                .sum::<usize>()
    }
}

fn paint_request(area: LogicalRect, request: &TextRequest<'_>) -> (LayoutRequest, Placement) {
    let TextOptions {
        wrap,
        overflow,
        horizontal_align,
        vertical_align,
        max_lines,
    } = request.options;
    let wrap = match wrap {
        TextWrap::None => blit_text::TextWrap::None,
        TextWrap::Word => blit_text::TextWrap::Word,
        TextWrap::Character => blit_text::TextWrap::Character,
    };
    let overflow = match overflow {
        TextOverflow::Clip => blit_text::TextOverflow::Clip,
        TextOverflow::Ellipsis => blit_text::TextOverflow::Ellipsis,
    };
    let width = area.width.max(0.0);
    let height = area.height.max(0.0);
    (
        LayoutRequest {
            max_width: (wrap != blit_text::TextWrap::None || overflow == blit_text::TextOverflow::Ellipsis)
                .then_some(width),
            max_height: (overflow == blit_text::TextOverflow::Ellipsis).then_some(height),
            max_lines,
            wrap,
            overflow,
        },
        Placement {
            width,
            height,
            horizontal_align,
            vertical_align,
        },
    )
}

fn placement_offset(placement: Placement, layout: &TextLayout, line: usize) -> LogicalPoint {
    let line = &layout.lines[line];
    LogicalPoint {
        x: match placement.horizontal_align {
            HorizontalAlign::Left => 0.0,
            HorizontalAlign::Center => (placement.width - line.bounds.width) / 2.0,
            HorizontalAlign::Right => placement.width - line.bounds.width,
        },
        y: match placement.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Center => (placement.height - layout.size.height) / 2.0,
            VerticalAlign::Bottom => placement.height - layout.size.height,
        },
    }
}

fn hash_style(style: blit_text::TextStyle, state: &mut impl Hasher) {
    style.font.hash(state);
    (if style.size == 0.0 { 0 } else { style.size.to_bits() }).hash(state);
    style.weight.hash(state);
    style.stretch.hash(state);
    style.style.hash(state);
    style.settings.features.hash(state);
    for (tag, value) in style.settings.variations {
        tag.hash(state);
        (if *value == 0.0 { 0 } else { value.to_bits() }).hash(state);
    }
    style.settings.language.hash(state);
    match style.settings.optical_size {
        OpticalSize::Auto => 0u8.hash(state),
        OpticalSize::FontDefault => 1u8.hash(state),
        OpticalSize::Fixed(size) => {
            2u8.hash(state);
            (if size == 0.0 { 0 } else { size.to_bits() }).hash(state);
        }
    }
}
