use std::{
    hash::{Hash, Hasher},
    mem::size_of,
};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use blit_cache::{DeferredCache, Equivalent, Scale};
use blit_text::{
    Caret, FontCandidate, FontData, FontError, FontFace, FontFaceId, FontSelectionId, FontStyle,
    LayoutRequest, TextLayout, TextLayoutEngine,
};

use crate::text::{
    FontId, HorizontalAlign, Span, TextLayoutRequest, TextOptions, TextOverflow, TextRequest,
    TextRunId, TextStyle, TextWrap, VerticalAlign,
};

pub struct TextConfig {
    pub fonts: Vec<FontFamily>,
    pub text_cache_capacity: usize,
    pub layout_cache_capacity: usize,
}

pub struct FontFamily {
    pub id: FontId,
    pub fonts: Vec<FontData>,
}

pub struct TextSystem {
    engine: Box<dyn ErasedTextEngine>,
    fonts: Box<[ConfiguredFont]>,
    texts: DeferredCache<TextKey, CachedText, TextScale>,
    layouts: DeferredCache<LayoutKey, CachedLayout, LayoutScale>,
    next_text: u32,
    next_layout: u64,
}

pub struct ResolvedTextLayout<'a> {
    pub id: TextLayoutId,
    pub layout: &'a TextLayout,
    engine: &'a dyn ErasedTextEngine,
    placement: Placement,
}

impl ResolvedTextLayout<'_> {
    pub fn font_face(&self, face: FontFaceId) -> Option<&FontFace> {
        self.engine.font_face(face)
    }

    pub fn line_offset(&self, line: usize) -> LogicalPoint {
        placement_offset(self.placement, self.layout, line)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextLayoutId(u64);

struct ConfiguredFont {
    id: FontId,
    font: FontSelectionId,
}

struct TextKey {
    text: Box<str>,
    spans: CachedSpans,
}

struct CachedText {
    id: TextRunId,
    shape: Option<usize>,
}

enum CachedSpans {
    One(blit_text::TextStyle),
    Many(Box<[blit_text::TextSpan]>),
}

struct TextQuery<'a> {
    text: &'a str,
    spans: &'a [Span],
    style: blit_text::TextStyle,
    fonts: &'a [ConfiguredFont],
}

impl TextQuery<'_> {
    fn resolve(&self, span: &Span) -> blit_text::TextStyle {
        blit_text::TextStyle {
            font: span.style.font.map_or(self.style.font, |id| {
                self.fonts.iter().find(|font| font.id == id).unwrap().font
            }),
            size: span.style.size.unwrap_or(self.style.size),
            weight: span.style.weight.unwrap_or(self.style.weight),
            stretch: span.style.stretch.unwrap_or(self.style.stretch),
            style: span.style.style.unwrap_or(self.style.style),
        }
    }
}

impl Hash for TextQuery<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for span in self.spans {
            hash_span(&self.text[span.range.clone()], self.resolve(span), state);
        }
    }
}

impl Equivalent<TextKey> for TextQuery<'_> {
    fn equivalent(&self, key: &TextKey) -> bool {
        if key.text.as_ref() != self.text {
            return false;
        }
        match &key.spans {
            CachedSpans::One(style) => {
                self.spans.len() == 1 && *style == self.resolve(&self.spans[0])
            }
            CachedSpans::Many(spans) => {
                spans.len() == self.spans.len()
                    && spans.iter().zip(self.spans).all(|(cached_span, span)| {
                        cached_span.range == span.range && cached_span.style == self.resolve(span)
                    })
            }
        }
    }
}

impl Hash for TextKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.spans {
            CachedSpans::One(style) => hash_span(&self.text, *style, state),
            CachedSpans::Many(spans) => {
                for span in spans {
                    hash_span(&self.text[span.range.clone()], span.style, state);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct LayoutKey {
    text: TextRunId,
    max_width: Option<u32>,
    max_height: Option<u32>,
    max_lines: Option<u16>,
    wrap: blit_text::TextWrap,
    overflow: blit_text::TextOverflow,
}

struct CachedLayout {
    id: TextLayoutId,
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
    fn font_face(&self, face: FontFaceId) -> Option<&FontFace>;

    fn shape(&mut self, index: usize, text: blit_text::Text<'_>) -> usize;

    fn layout(&mut self, text: ShapedText<'_>, request: LayoutRequest) -> TextLayout;

    fn carets(&mut self, text: ShapedText<'_>, request: LayoutRequest, line: usize)
    -> Box<[Caret]>;

    fn remove_shape(&mut self, index: usize);
}

struct ShapedText<'a> {
    index: usize,
    text: &'a str,
}

struct ErasedEngine<E: TextLayoutEngine> {
    engine: E,
    shapes: Vec<Option<E::Shape>>,
}

impl<E: TextLayoutEngine> ErasedTextEngine for ErasedEngine<E> {
    fn font_face(&self, face: FontFaceId) -> Option<&FontFace> {
        self.engine.font_face(face)
    }

    fn shape(&mut self, index: usize, text: blit_text::Text<'_>) -> usize {
        if self.shapes.len() <= index {
            self.shapes.resize_with(index + 1, || None);
        }
        let (shape, heap_weight) = self.engine.shape(text);
        assert!(self.shapes[index].replace(shape).is_none());
        size_of::<E::Shape>() + heap_weight
    }

    fn layout(
        &mut self,
        ShapedText { index, text }: ShapedText<'_>,
        request: LayoutRequest,
    ) -> TextLayout {
        self.engine
            .layout(self.shapes[index].as_mut().unwrap(), text, request)
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

struct TextScale;

impl Scale<TextKey, CachedText> for TextScale {
    fn weight(&self, key: &TextKey, text: &CachedText) -> usize {
        let spans = match &key.spans {
            CachedSpans::One(_) => 0,
            CachedSpans::Many(spans) => spans.len() * size_of::<blit_text::TextSpan>(),
        };
        size_of::<TextKey>()
            + size_of::<CachedText>()
            + key.text.len()
            + spans
            + text.shape.unwrap_or(0)
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

impl TextSystem {
    pub fn new<E: TextLayoutEngine>(config: TextConfig, mut engine: E) -> Result<Self, FontError> {
        let mut fonts = Vec::new();
        let mut candidates = Vec::new();
        for configured in config.fonts {
            if fonts
                .iter()
                .any(|font: &ConfiguredFont| font.id == configured.id)
            {
                return Err(FontError::InvalidData);
            }
            candidates.clear();
            for data in configured.fonts {
                for backend_face in engine.register_font(data)? {
                    let registered = engine
                        .font_face(backend_face)
                        .ok_or(FontError::InvalidData)?;
                    let face =
                        ttf_parser::Face::parse(registered.data.as_ref(), registered.face_index)
                            .map_err(|_| FontError::InvalidData)?;
                    let stretch = match face.width() {
                        ttf_parser::Width::UltraCondensed => 50,
                        ttf_parser::Width::ExtraCondensed => 63,
                        ttf_parser::Width::Condensed => 75,
                        ttf_parser::Width::SemiCondensed => 88,
                        ttf_parser::Width::Normal => 100,
                        ttf_parser::Width::SemiExpanded => 113,
                        ttf_parser::Width::Expanded => 125,
                        ttf_parser::Width::ExtraExpanded => 150,
                        ttf_parser::Width::UltraExpanded => 200,
                    };
                    let style = match face.style() {
                        ttf_parser::Style::Normal => FontStyle::Normal,
                        ttf_parser::Style::Italic => FontStyle::Italic,
                        ttf_parser::Style::Oblique => FontStyle::Oblique,
                    };
                    candidates.push(FontCandidate {
                        face: backend_face,
                        weight: face.weight().to_number(),
                        stretch,
                        style,
                    });
                }
            }
            let font = engine.register_font_selection(&candidates)?;
            fonts.push(ConfiguredFont {
                id: configured.id,
                font,
            });
        }
        Ok(Self {
            engine: Box::new(ErasedEngine {
                engine,
                shapes: Vec::new(),
            }),
            fonts: fonts.into_boxed_slice(),
            texts: DeferredCache::new(TextScale, config.text_cache_capacity),
            layouts: DeferredCache::new(LayoutScale, config.layout_cache_capacity),
            next_text: 1,
            next_layout: 1,
        })
    }

    pub fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId {
        self.rich_text(text, &[Span::new(0..text.len())], style)
    }

    pub fn rich_text(&mut self, text: &str, spans: &[Span], style: TextStyle) -> TextRunId {
        let Some(font) = self.fonts.iter().find(|font| font.id == style.font) else {
            return TextRunId::default();
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
                return TextRunId::default();
            }
        }
        assert_eq!(end, text.len(), "rich text spans must cover the text");
        let style = blit_text::TextStyle {
            font: font.font,
            size: style.size,
            weight: style.weight,
            stretch: style.stretch,
            style: style.style,
        };
        let query = TextQuery {
            text,
            spans,
            style,
            fonts: &self.fonts,
        };
        let next_text = self.next_text;
        let mut inserted = false;
        let (_, index) = self.texts.get_or_insert(query, |query| {
            inserted = true;
            let spans = if query.spans.len() == 1 {
                CachedSpans::One(query.resolve(&query.spans[0]))
            } else {
                let mut resolved = Vec::with_capacity(query.spans.len());
                for span in query.spans {
                    resolved.push(blit_text::TextSpan {
                        range: span.range.clone(),
                        style: query.resolve(span),
                    });
                }
                CachedSpans::Many(resolved.into_boxed_slice())
            };
            (
                TextKey {
                    text: query.text.into(),
                    spans,
                },
                CachedText {
                    id: TextRunId(u64::from(next_text) << 32),
                    shape: None,
                },
            )
        });
        if inserted {
            let slot = u32::try_from(index + 1).expect("too many cached texts");
            self.texts
                .update_index(index, |text| text.id.0 |= u64::from(slot));
            self.next_text = self.next_text.checked_add(1).expect("too many texts");
        }
        self.texts.get_index(index).id
    }

    pub fn paint_layout(&mut self, request: &TextRequest) -> ResolvedTextLayout<'_> {
        let (layout_request, placement) = paint_request(request);
        let (index, _) = self.layout(request.text, layout_request);
        let cached = self.layouts.get_index(index);
        ResolvedTextLayout {
            id: cached.id,
            layout: &cached.layout,
            engine: self.engine.as_ref(),
            placement,
        }
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

    pub fn offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        let (layout_request, placement) = paint_request(request);
        let (layout_index, text_index) = self.layout(request.text, layout_request);
        let position = LogicalPoint {
            x: position.x - request.area.x + request.offset_x,
            y: position.y - request.area.y,
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
        self.ensure_carets(text_index, layout_index, layout_request, line);
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
        request: &TextRequest,
        byte_offset: usize,
        width: f32,
    ) -> LogicalRect {
        let (layout_request, placement) = paint_request(request);
        let (layout_index, text_index) = self.layout(request.text, layout_request);
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
                x: request.area.x - request.offset_x,
                y: request.area.y,
                width,
                height: 0.0,
            };
        };
        self.ensure_carets(text_index, layout_index, layout_request, line);
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
            x: request.area.x + caret.map_or(0.0, |caret| caret.position.x) + offset.x
                - request.offset_x,
            y: request.area.y + caret.map_or(0.0, |caret| caret.position.y) + offset.y,
            width,
            height: caret.map_or(0.0, |caret| caret.height),
        }
    }

    pub fn finish_frame(&mut self) {
        self.layouts.trim_to_weight();
        let engine = &mut self.engine;
        self.texts.trim_to_weight_if(|_, text| {
            if text.shape.is_some() {
                let index = (text.id.0 as u32).checked_sub(1).unwrap() as usize;
                engine.remove_shape(index);
            }
            true
        });
    }
}

impl TextSystem {
    fn layout(&mut self, id: TextRunId, request: LayoutRequest) -> (usize, usize) {
        let index = (id.0 as u32).checked_sub(1).expect("invalid text") as usize;
        let unshaped = self.texts.update_index(index, |cached| {
            assert_eq!(cached.id, id, "expired text");
            cached.shape.is_none()
        });
        if unshaped {
            let cached = self.texts.get_key_index(index);
            let one;
            let spans = match &cached.spans {
                CachedSpans::One(style) => {
                    one = blit_text::TextSpan {
                        range: 0..cached.text.len(),
                        style: *style,
                    };
                    std::slice::from_ref(&one)
                }
                CachedSpans::Many(spans) => spans,
            };
            let shape = self.engine.shape(
                index,
                blit_text::Text {
                    text: &cached.text,
                    spans,
                },
            );
            self.texts
                .update_index(index, |cached| cached.shape = Some(shape));
        }
        let key = LayoutKey {
            text: id,
            max_width: request.max_width.map(f32::to_bits),
            max_height: request.max_height.map(f32::to_bits),
            max_lines: request.max_lines,
            wrap: request.wrap,
            overflow: request.overflow,
        };
        let next_layout = self.next_layout;
        let engine = &mut self.engine;
        let text = &self.texts.get_key_index(index).text;
        let (_, layout) = self.layouts.get_or_insert(key, |_| {
            let layout = engine.layout(ShapedText { index, text }, request);
            (
                key,
                CachedLayout {
                    id: TextLayoutId(next_layout),
                    layout,
                    carets: Vec::new(),
                },
            )
        });
        if self.layouts.get_index(layout).id.0 == next_layout {
            self.next_layout = self
                .next_layout
                .checked_add(1)
                .expect("too many text layouts");
        }
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
        let text = &self.texts.get_key_index(index).text;
        let carets = self
            .engine
            .carets(ShapedText { index, text }, request, line);
        self.layouts.update_index(layout, |cached| {
            cached.carets.push(LineCarets {
                line: u32::try_from(line).expect("too many text lines"),
                carets,
            });
        });
    }
}

fn paint_request(request: &TextRequest) -> (LayoutRequest, Placement) {
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
    let width = request.area.width.max(0.0);
    let height = request.area.height.max(0.0);
    (
        LayoutRequest {
            max_width: (wrap != blit_text::TextWrap::None
                || overflow == blit_text::TextOverflow::Ellipsis)
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
            HorizontalAlign::Center => ((placement.width - line.bounds.width) / 2.0).floor(),
            HorizontalAlign::Right => (placement.width - line.bounds.width).floor(),
        },
        y: match placement.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Center => ((placement.height - layout.size.height) / 2.0).floor(),
            VerticalAlign::Bottom => (placement.height - layout.size.height).floor(),
        },
    }
}

fn hash_span(text: &str, style: blit_text::TextStyle, state: &mut impl Hasher) {
    text.hash(state);
    style.font.hash(state);
    style.size.to_bits().hash(state);
    style.weight.hash(state);
    style.stretch.hash(state);
    style.style.hash(state);
}
