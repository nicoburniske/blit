use std::{
    hash::{Hash, Hasher},
    mem::size_of,
};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use blit_cache::{DeferredCache, Equivalent, Scale};
use blit_text::{
    FontCandidate, FontData, FontError, FontSelectionId, FontStyle, LayoutRequest, TextLayout,
    TextLayoutEngine,
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
    engine: Box<dyn TextLayoutEngine>,
    fonts: Box<[ConfiguredFont]>,
    texts: DeferredCache<TextKey, TextRunId, TextScale>,
    layouts: DeferredCache<LayoutKey, CachedLayout, LayoutScale>,
    next_text: u32,
    next_layout: u64,
}

pub struct ResolvedTextLayout<'a> {
    pub id: TextLayoutId,
    pub layout: &'a TextLayout,
    pub engine: &'a dyn TextLayoutEngine,
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

enum CachedSpans {
    One(blit_text::TextStyle),
    Many(Box<[blit_text::TextSpan]>),
}

struct TextQuery<'a> {
    spans: &'a [Span<'a>],
    style: blit_text::TextStyle,
    fonts: &'a [ConfiguredFont],
}

impl TextQuery<'_> {
    fn resolve(&self, span: &Span<'_>) -> blit_text::TextStyle {
        blit_text::TextStyle {
            font: span.font.map_or(self.style.font, |id| {
                self.fonts.iter().find(|font| font.id == id).unwrap().font
            }),
            size: span.size.unwrap_or(self.style.size),
            weight: span.weight.unwrap_or(self.style.weight),
            stretch: span.stretch.unwrap_or(self.style.stretch),
            style: span.style.unwrap_or(self.style.style),
        }
    }
}

impl Hash for TextQuery<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for span in self.spans {
            hash_span(span.text, self.resolve(span), state);
        }
    }
}

impl Equivalent<TextKey> for TextQuery<'_> {
    fn equivalent(&self, key: &TextKey) -> bool {
        match &key.spans {
            CachedSpans::One(style) => {
                self.spans.len() == 1
                    && key.text.as_ref() == self.spans[0].text
                    && *style == self.resolve(&self.spans[0])
            }
            CachedSpans::Many(spans) => {
                spans.len() == self.spans.len()
                    && spans.iter().zip(self.spans).all(|(cached_span, span)| {
                        &key.text[cached_span.range.clone()] == span.text
                            && cached_span.style == self.resolve(span)
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
    horizontal_align: blit_text::HorizontalAlign,
    vertical_align: blit_text::VerticalAlign,
}

struct CachedLayout {
    id: TextLayoutId,
    layout: TextLayout,
}

struct TextScale;

impl Scale<TextKey, TextRunId> for TextScale {
    fn weight(&self, key: &TextKey, _text: &TextRunId) -> usize {
        let spans = match &key.spans {
            CachedSpans::One(_) => 0,
            CachedSpans::Many(spans) => spans.len() * size_of::<blit_text::TextSpan>(),
        };
        size_of::<TextKey>() + size_of::<TextRunId>() + key.text.len() + spans
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
            + cached.layout.carets.len() * size_of::<blit_text::Caret>()
    }
}

impl TextSystem {
    pub fn new(
        config: TextConfig,
        mut engine: Box<dyn TextLayoutEngine>,
    ) -> Result<Self, FontError> {
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
            engine,
            fonts: fonts.into_boxed_slice(),
            texts: DeferredCache::new(TextScale, config.text_cache_capacity),
            layouts: DeferredCache::new(LayoutScale, config.layout_cache_capacity),
            next_text: 1,
            next_layout: 1,
        })
    }

    pub fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId {
        self.rich_text(&[Span::new(text)], style)
    }

    pub fn rich_text(&mut self, spans: &[Span<'_>], style: TextStyle) -> TextRunId {
        let empty = [Span::new("")];
        let spans = if spans.is_empty() { &empty } else { spans };
        let Some(font) = self.fonts.iter().find(|font| font.id == style.font) else {
            return TextRunId::default();
        };
        if spans.iter().any(|span| {
            span.font
                .is_some_and(|id| !self.fonts.iter().any(|font| font.id == id))
        }) {
            return TextRunId::default();
        }
        let style = blit_text::TextStyle {
            font: font.font,
            size: style.size,
            weight: style.weight,
            stretch: style.stretch,
            style: style.style,
        };
        let query = TextQuery {
            spans,
            style,
            fonts: &self.fonts,
        };
        let len = spans.iter().map(|span| span.text.len()).sum();
        let next_text = self.next_text;
        let (_, index) = self.texts.get_or_insert(query, |query| {
            let (text, spans) = if query.spans.len() == 1 {
                (
                    query.spans[0].text.into(),
                    CachedSpans::One(query.resolve(&query.spans[0])),
                )
            } else {
                let mut text = String::with_capacity(len);
                let mut resolved = Vec::with_capacity(query.spans.len());
                for span in query.spans {
                    let start = text.len();
                    text.push_str(span.text);
                    resolved.push(blit_text::TextSpan {
                        range: start..text.len(),
                        style: query.resolve(span),
                    });
                }
                (text.into(), CachedSpans::Many(resolved.into_boxed_slice()))
            };
            (
                TextKey { text, spans },
                TextRunId(u64::from(next_text) << 32),
            )
        });
        if self.texts.get_index(index).0 as u32 == 0 {
            let slot = u32::try_from(index + 1).expect("too many cached texts");
            self.texts
                .update_index(index, |text| text.0 |= u64::from(slot));
            self.next_text = self.next_text.checked_add(1).expect("too many texts");
        }
        *self.texts.get_index(index)
    }

    pub fn paint_layout(&mut self, request: &TextRequest) -> ResolvedTextLayout<'_> {
        self.layout(request.text, paint_request(request))
    }

    pub fn measure(&mut self, request: &TextLayoutRequest) -> LogicalSize {
        self.layout(
            request.text,
            LayoutRequest {
                max_width: request.max_width,
                max_height: None,
                max_lines: request.max_lines,
                wrap: match request.wrap {
                    TextWrap::None => blit_text::TextWrap::None,
                    TextWrap::Word => blit_text::TextWrap::Word,
                    TextWrap::Character => blit_text::TextWrap::Character,
                },
                overflow: blit_text::TextOverflow::Clip,
                horizontal_align: blit_text::HorizontalAlign::Left,
                vertical_align: blit_text::VerticalAlign::Top,
            },
        )
        .layout
        .size
    }

    pub fn offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        let layout = self.layout(request.text, paint_request(request)).layout;
        let position = LogicalPoint {
            x: position.x - request.area.x + request.offset_x,
            y: position.y - request.area.y,
        };
        let Some(line) = layout.lines.iter().min_by(|left, right| {
            let left_distance = if position.y < left.bounds.y {
                left.bounds.y - position.y
            } else if position.y > left.bounds.y + left.bounds.height {
                position.y - left.bounds.y - left.bounds.height
            } else {
                0.0
            };
            let right_distance = if position.y < right.bounds.y {
                right.bounds.y - position.y
            } else if position.y > right.bounds.y + right.bounds.height {
                position.y - right.bounds.y - right.bounds.height
            } else {
                0.0
            };
            left_distance.total_cmp(&right_distance)
        }) else {
            return 0;
        };
        layout.carets[line.carets.start as usize..line.carets.end as usize]
            .iter()
            .min_by(|left, right| {
                (left.position.x - position.x)
                    .abs()
                    .total_cmp(&(right.position.x - position.x).abs())
            })
            .map_or(0, |caret| caret.byte_offset as usize)
    }

    pub fn cursor_rect(
        &mut self,
        request: &TextRequest,
        byte_offset: usize,
        width: f32,
    ) -> LogicalRect {
        let layout = self.layout(request.text, paint_request(request)).layout;
        let caret = layout
            .carets
            .iter()
            .min_by_key(|caret| (caret.byte_offset as usize).abs_diff(byte_offset));
        LogicalRect {
            x: request.area.x + caret.map_or(0.0, |caret| caret.position.x) - request.offset_x,
            y: request.area.y + caret.map_or(0.0, |caret| caret.position.y),
            width,
            height: caret.map_or(0.0, |caret| caret.height),
        }
    }

    pub fn finish_frame(&mut self) {
        self.layouts.trim_to_weight();
        self.texts.trim_to_weight();
    }
}

impl TextSystem {
    fn layout(&mut self, text: TextRunId, request: LayoutRequest) -> ResolvedTextLayout<'_> {
        let text_index = (text.0 as u32).checked_sub(1).expect("invalid text") as usize;
        self.texts.update_index(text_index, |cached| {
            assert_eq!(*cached, text, "expired text")
        });
        let cached = self.texts.get_key_index(text_index);
        let key = LayoutKey {
            text,
            max_width: request.max_width.map(f32::to_bits),
            max_height: request.max_height.map(f32::to_bits),
            max_lines: request.max_lines,
            wrap: request.wrap,
            overflow: request.overflow,
            horizontal_align: request.horizontal_align,
            vertical_align: request.vertical_align,
        };
        let next_layout = self.next_layout;
        let engine = &mut self.engine;
        let (_, index) = self.layouts.get_or_insert(key, |_| {
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
            (
                key,
                CachedLayout {
                    id: TextLayoutId(next_layout),
                    layout: engine.layout(
                        blit_text::Text {
                            text: &cached.text,
                            spans,
                        },
                        request,
                    ),
                },
            )
        });
        if self.layouts.get_index(index).id.0 == next_layout {
            self.next_layout = self
                .next_layout
                .checked_add(1)
                .expect("too many text layouts");
        }
        let cached = self.layouts.get_index(index);
        ResolvedTextLayout {
            id: cached.id,
            layout: &cached.layout,
            engine: self.engine.as_ref(),
        }
    }
}

fn paint_request(request: &TextRequest) -> LayoutRequest {
    let TextOptions {
        wrap,
        overflow,
        horizontal_align,
        vertical_align,
        max_lines,
    } = request.options;
    LayoutRequest {
        max_width: Some(request.area.width.max(0.0)),
        max_height: Some(request.area.height.max(0.0)),
        max_lines,
        wrap: match wrap {
            TextWrap::None => blit_text::TextWrap::None,
            TextWrap::Word => blit_text::TextWrap::Word,
            TextWrap::Character => blit_text::TextWrap::Character,
        },
        overflow: match overflow {
            TextOverflow::Clip => blit_text::TextOverflow::Clip,
            TextOverflow::Ellipsis => blit_text::TextOverflow::Ellipsis,
        },
        horizontal_align: match horizontal_align {
            HorizontalAlign::Left => blit_text::HorizontalAlign::Left,
            HorizontalAlign::Center => blit_text::HorizontalAlign::Center,
            HorizontalAlign::Right => blit_text::HorizontalAlign::Right,
        },
        vertical_align: match vertical_align {
            VerticalAlign::Top => blit_text::VerticalAlign::Top,
            VerticalAlign::Center => blit_text::VerticalAlign::Center,
            VerticalAlign::Bottom => blit_text::VerticalAlign::Bottom,
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
