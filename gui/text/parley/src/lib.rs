use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
    mem::size_of,
    ops::Range,
    rc::Rc,
    sync::Arc,
};

use blit::{LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint};
use blit_text::{
    Caret, Family, FontError, FontStackId, FontStyle, Glyph, GlyphImage, GlyphMetrics, GlyphPixels, GlyphRequest,
    LayoutLine, LayoutRequest, LayoutRun, OpticalSize, PreparedGlyph, TextLayout, TextOverflow, TextWrap,
};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontFamilyName, FontFeature, FontFeatures, FontVariation,
    FontVariations, FontWeight, FontWidth, Language, Layout, LayoutContext, LineHeight, PositionedLayoutItem,
    StyleProperty, WordBreak,
    fontique::{Blob, Collection, CollectionOptions, SourceCache},
    setting::Tag,
};
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, StrikeWith, image::Content},
    zeno::{Angle, Format, Transform, Vector},
};

/// system and embedded fonts with css style font stacks and color glyphs
/// parley has no ellipsis so trailing glyphs are trimmed and a single ellipsis glyph from the last font is appended
/// this keeps byte offsets of the source text unlike reshaping a truncated string
pub struct Backend {
    fonts: FontContext,
    layouts: LayoutContext<usize>,
    scale: ScaleContext,
    stacks: Vec<Box<[FontFamilyName<'static>]>>,
}

pub struct Shape {
    spans: Box<[Span]>,
    word: Layout<usize>,
    /// built on first character wrapped layout since break opportunities are fixed at build
    character: Option<Layout<usize>>,
    fonts: Vec<Font>,
}

/// glyph pixels for a font instance at a physical size and horizontal phase
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    instance: Rc<Instance>,
    glyph: u16,
    size: u32,
    phase: u8,
}

struct Span {
    range: Range<usize>,
    stack: usize,
    size: f32,
    weight: u16,
    stretch: u16,
    style: FontStyle,
    features: Box<[FontFeature]>,
    variations: Box<[FontVariation]>,
    language: Option<Language>,
}

struct Font {
    instance: Rc<Instance>,
    size: f32,
}

/// a font face with variation coordinates and synthesis
struct Instance {
    font: parley::FontData,
    coords: Box<[i16]>,
    embolden: bool,
    skew: i8,
}

impl PartialEq for Instance {
    fn eq(&self, other: &Self) -> bool {
        self.font.data.id() == other.font.data.id()
            && self.font.index == other.font.index
            && self.coords == other.coords
            && self.embolden == other.embolden
            && self.skew == other.skew
    }
}

impl Eq for Instance {}

impl Hash for Instance {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font.data.id().hash(state);
        self.font.index.hash(state);
        self.coords.hash(state);
        self.embolden.hash(state);
        self.skew.hash(state);
    }
}

impl Backend {
    pub fn new() -> Self {
        Self::with_fonts(FontContext::new())
    }

    pub fn without_system_fonts() -> Self {
        Self::with_fonts(FontContext {
            collection: Collection::new(CollectionOptions {
                shared: false,
                system_fonts: false,
            }),
            source_cache: SourceCache::default(),
        })
    }

    fn with_fonts(fonts: FontContext) -> Self {
        Self {
            fonts,
            layouts: LayoutContext::new(),
            scale: ScaleContext::new(),
            stacks: Vec::new(),
        }
    }
}

impl Default for Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl blit_text::TextLayoutEngine for Backend {
    type Shape = Shape;
    type GlyphKey = GlyphKey;

    fn register_stack(&mut self, families: &[Family]) -> Result<FontStackId, FontError> {
        let mut names = Vec::new();
        for family in families {
            match family {
                Family::Embedded(data) => {
                    let start = names.len();
                    for data in data {
                        let registered = self
                            .fonts
                            .collection
                            .register_fonts(Blob::new(Arc::new(data.clone())), None);
                        if registered.is_empty() {
                            return Err(FontError::InvalidData);
                        }
                        for (id, _) in registered {
                            let name = self.fonts.collection.family_name(id).ok_or(FontError::InvalidData)?;
                            if !names[start..].iter().any(|family| match family {
                                FontFamilyName::Named(family) => family == name,
                                FontFamilyName::Generic(_) => false,
                            }) {
                                names.push(FontFamilyName::Named(Cow::Owned(name.to_owned())));
                            }
                        }
                    }
                }
                Family::Named(name) => names.push(FontFamilyName::Named(Cow::Owned(name.clone()))),
            }
        }
        if names.is_empty() {
            return Err(FontError::NotFound);
        }
        self.stacks.push(names.into_boxed_slice());
        Ok(FontStackId(self.stacks.len() as u64))
    }

    fn prepare_glyph(&mut self, shape: &Shape, request: GlyphRequest) -> PreparedGlyph<GlyphKey> {
        let font = &shape.fonts[request.font];
        let x = (request.origin.x * 4.0).round() as i32;
        PreparedGlyph {
            key: GlyphKey {
                instance: font.instance.clone(),
                glyph: request.glyph,
                size: (font.size * request.scale).to_bits(),
                phase: x.rem_euclid(4) as u8,
            },
            origin: PhysicalPoint::new(x.div_euclid(4), request.origin.y.round() as i32),
        }
    }

    fn rasterize_glyph(&mut self, key: &GlyphKey) -> Result<GlyphImage, FontError> {
        let instance = &key.instance;
        let font = FontRef::from_index(instance.font.data.as_ref(), instance.font.index as usize)
            .ok_or(FontError::InvalidData)?;
        let size = f32::from_bits(key.size);
        let mut scaler = self
            .scale
            .builder(font)
            .size(size)
            .normalized_coords(instance.coords.iter().copied())
            .build();
        let Some(mut image) = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ])
        .format(Format::Alpha)
        .offset(Vector::new(f32::from(key.phase) / 4.0, 0.0))
        .embolden(if instance.embolden { 1.0 } else { 0.0 })
        .transform(
            (instance.skew != 0)
                .then(|| Transform::skew(Angle::from_degrees(f32::from(instance.skew)), Angle::from_degrees(0.0))),
        )
        .render(&mut scaler, key.glyph) else {
            return Ok(GlyphImage::default());
        };
        let metrics = GlyphMetrics {
            width: image.placement.width.try_into().map_err(|_| FontError::Unsupported)?,
            height: image.placement.height.try_into().map_err(|_| FontError::Unsupported)?,
            left: image.placement.left.try_into().map_err(|_| FontError::Unsupported)?,
            top: (-image.placement.top).try_into().map_err(|_| FontError::Unsupported)?,
        };
        let pixels = match image.content {
            Content::Mask => GlyphPixels::Alpha8(image.data.into_boxed_slice()),
            Content::Color => {
                // color bitmaps are straight alpha while color outlines are already premultiplied
                if matches!(image.source, Source::ColorBitmap(_)) {
                    for pixel in image.data.as_chunks_mut::<4>().0 {
                        for channel in 0..3 {
                            pixel[channel] = (u16::from(pixel[channel]) * u16::from(pixel[3]) / 255) as u8;
                        }
                    }
                }
                GlyphPixels::Rgba8(image.data.into_boxed_slice())
            }
            Content::SubpixelMask => return Err(FontError::Unsupported),
        };
        Ok(GlyphImage { metrics, pixels })
    }

    fn shape(&mut self, text: blit_text::Text<'_>) -> (Shape, usize) {
        let spans = text
            .segments()
            .map(|(_, range, style)| {
                let settings = style.settings;
                // optical size follows css font-optical-sizing unless variations set opsz
                let optical_size = match settings.optical_size {
                    OpticalSize::Auto => Some(style.size),
                    OpticalSize::FontDefault => None,
                    OpticalSize::Fixed(size) => Some(size),
                }
                .filter(|_| !settings.variations.iter().any(|(tag, _)| tag == b"opsz"));
                Span {
                    range,
                    stack: style.font.0 as usize - 1,
                    size: style.size,
                    weight: style.weight,
                    stretch: style.stretch,
                    style: style.style,
                    features: settings
                        .features
                        .iter()
                        .map(|(tag, value)| FontFeature {
                            tag: Tag::new(tag),
                            value: (*value).try_into().unwrap_or(u16::MAX),
                        })
                        .collect(),
                    variations: settings
                        .variations
                        .iter()
                        .map(|(tag, value)| (tag, *value))
                        .chain(optical_size.map(|size| (b"opsz", size)))
                        .map(|(tag, value)| FontVariation {
                            tag: Tag::new(tag),
                            value,
                        })
                        .collect(),
                    language: settings.language.and_then(|language| Language::parse(language).ok()),
                }
            })
            .collect::<Box<[_]>>();
        let word = build(self, text.text, &spans, false);
        let weight = spans.len() * size_of::<Span>()
            + spans
                .iter()
                .map(|span| {
                    span.features.len() * size_of::<FontFeature>()
                        + span.variations.len() * size_of::<FontVariation>()
                })
                .sum::<usize>()
            // rough estimate of shaped clusters and glyphs
            + text.text.len() * 64;
        (
            Shape {
                spans,
                word,
                character: None,
                fonts: Vec::new(),
            },
            weight,
        )
    }

    fn layout(&mut self, shape: &mut Shape, text: &str, request: LayoutRequest) -> TextLayout {
        let (layout, fonts) = prepare(self, shape, text, request);
        let visible = visible_lines(layout, request);
        let truncated = visible < layout.len();
        let mut glyphs = Vec::new();
        let mut runs = Vec::new();
        let mut lines = Vec::with_capacity(visible);
        let mut size = LogicalSize::new(0.0f32, 0.0f32);
        for line in layout.lines().take(visible) {
            let line_index = lines.len() as u32;
            let line_start = glyphs.len();
            let metrics = line.metrics();
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let font = font_index(fonts, run);
                let start = glyphs.len() as u32;
                glyphs.extend(glyph_run.positioned_glyphs().map(|glyph| Glyph {
                    id: glyph.id as u16,
                    position: LogicalPoint::new(glyph.x, glyph.y),
                }));
                runs.push(LayoutRun {
                    font,
                    glyphs: start..glyphs.len() as u32,
                    span: glyph_run.style().brush,
                    line: line_index,
                });
            }
            let mut width = metrics.advance - metrics.hanging_advance;
            let last = lines.len() + 1 == visible;
            if request.overflow == TextOverflow::Ellipsis
                && last
                && (truncated || request.max_width.is_some_and(|max_width| width > max_width))
            {
                width = ellipsize(&mut glyphs, &mut runs, fonts, line, line_start, request);
            }
            let bounds = LogicalRect::new(
                0.0,
                metrics.block_min_coord,
                width,
                metrics.block_max_coord - metrics.block_min_coord,
            );
            size.width = size.width.max(width);
            size.height = size.height.max(bounds.y + bounds.height);
            let text_range = line.text_range();
            lines.push(LayoutLine {
                bounds,
                text: text_range.start as u32..text_range.end as u32,
            });
        }
        TextLayout {
            size,
            glyphs: glyphs.into_boxed_slice(),
            runs: runs.into_boxed_slice(),
            lines: lines.into_boxed_slice(),
        }
    }

    fn carets(&mut self, shape: &mut Shape, text: &str, request: LayoutRequest, line_index: usize) -> Box<[Caret]> {
        let (layout, _) = prepare(self, shape, text, request);
        if line_index >= visible_lines(layout, request) {
            return Box::new([]);
        }
        let Some(line) = layout.get(line_index) else {
            return Box::new([]);
        };
        let metrics = line.metrics();
        let y = metrics.block_min_coord;
        let height = metrics.block_max_coord - metrics.block_min_coord;
        let mut carets = Vec::new();
        let mut x = metrics.offset;
        for run in line.runs() {
            for cluster in run.visual_clusters() {
                let range = cluster.text_range();
                let advance = cluster.advance();
                let (start, end) = if cluster.is_rtl() {
                    (x + advance, x)
                } else {
                    (x, x + advance)
                };
                for (byte, x, end) in [(range.start, start, false), (range.end, end, true)] {
                    carets.push((
                        end,
                        Caret {
                            byte_offset: byte as u32,
                            position: LogicalPoint { x, y },
                            height,
                        },
                    ));
                }
                x += advance;
            }
        }
        if carets.is_empty() {
            return Box::new([Caret {
                byte_offset: line.text_range().start as u32,
                position: LogicalPoint { x: 0.0, y },
                height,
            }]);
        }
        carets.sort_by_key(|(end, caret)| (caret.byte_offset, *end));
        carets.dedup_by_key(|(_, caret)| caret.byte_offset);
        carets.into_iter().map(|(_, caret)| caret).collect()
    }
}

fn build(backend: &mut Backend, text: &str, spans: &[Span], break_all: bool) -> Layout<usize> {
    let mut builder = backend.layouts.ranged_builder(&mut backend.fonts, text, 1.0, false);
    if break_all {
        builder.push_default(StyleProperty::WordBreak(WordBreak::BreakAll));
    }
    for (index, span) in spans.iter().enumerate() {
        let range = span.range.clone();
        builder.push(
            StyleProperty::FontFamily(FontFamily::List(Cow::Borrowed(&backend.stacks[span.stack]))),
            range.clone(),
        );
        builder.push(StyleProperty::FontSize(span.size), range.clone());
        builder.push(
            StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.2)),
            range.clone(),
        );
        builder.push(
            StyleProperty::FontWeight(FontWeight::new(f32::from(span.weight))),
            range.clone(),
        );
        builder.push(
            StyleProperty::FontWidth(FontWidth::from_percentage(f32::from(span.stretch))),
            range.clone(),
        );
        builder.push(
            StyleProperty::FontStyle(match span.style {
                FontStyle::Normal => parley::FontStyle::Normal,
                FontStyle::Italic => parley::FontStyle::Italic,
                FontStyle::Oblique => parley::FontStyle::Oblique(None),
            }),
            range.clone(),
        );
        if !span.features.is_empty() {
            builder.push(
                StyleProperty::FontFeatures(FontFeatures::List(Cow::Borrowed(&span.features))),
                range.clone(),
            );
        }
        if !span.variations.is_empty() {
            builder.push(
                StyleProperty::FontVariations(FontVariations::List(Cow::Borrowed(&span.variations))),
                range.clone(),
            );
        }
        if span.language.is_some() {
            builder.push(StyleProperty::Locale(span.language), range.clone());
        }
        builder.push(StyleProperty::Brush(index), range);
    }
    builder.build(text)
}

/// breaks lines of the layout matching the requested wrap
fn prepare<'a>(
    backend: &mut Backend,
    shape: &'a mut Shape,
    text: &str,
    request: LayoutRequest,
) -> (&'a Layout<usize>, &'a mut Vec<Font>) {
    let Shape {
        spans,
        word,
        character,
        fonts,
    } = shape;
    let layout = match request.wrap {
        TextWrap::Character => character.get_or_insert_with(|| build(backend, text, spans, true)),
        TextWrap::None | TextWrap::Word => word,
    };
    layout.break_all_lines(match request.wrap {
        TextWrap::None => None,
        TextWrap::Word | TextWrap::Character => request.max_width,
    });
    layout.align(Alignment::Start, AlignmentOptions::default());
    (layout, fonts)
}

fn visible_lines(layout: &Layout<usize>, request: LayoutRequest) -> usize {
    layout
        .lines()
        .take(request.max_lines.map_or(usize::MAX, usize::from))
        .take_while(|line| {
            request
                .max_height
                .is_none_or(|height| line.metrics().block_max_coord <= height.max(0.0))
        })
        .count()
}

fn font_index(fonts: &mut Vec<Font>, run: &parley::Run<'_, usize>) -> usize {
    let synthesis = run.synthesis();
    let skew = synthesis.skew().unwrap_or(0.0) as i8;
    let coords = run.normalized_coords();
    fonts
        .iter()
        .position(|font| {
            font.size.to_bits() == run.font_size().to_bits()
                && font.instance.font.data.id() == run.font().data.id()
                && font.instance.font.index == run.font().index
                && font.instance.embolden == synthesis.embolden()
                && font.instance.skew == skew
                && font
                    .instance
                    .coords
                    .iter()
                    .copied()
                    .eq(coords.iter().map(|coord| coord.to_bits()))
        })
        .unwrap_or_else(|| {
            fonts.push(Font {
                instance: Rc::new(Instance {
                    font: run.font().clone(),
                    coords: coords.iter().map(|coord| coord.to_bits()).collect(),
                    embolden: synthesis.embolden(),
                    skew,
                }),
                size: run.font_size(),
            });
            fonts.len() - 1
        })
}

/// drops trailing glyphs of the line until an ellipsis fits and returns the line width
fn ellipsize(
    glyphs: &mut Vec<Glyph>,
    runs: &mut Vec<LayoutRun>,
    fonts: &mut Vec<Font>,
    line: parley::Line<'_, usize>,
    line_start: usize,
    request: LayoutRequest,
) -> f32 {
    let Some(PositionedLayoutItem::GlyphRun(source)) = line
        .items()
        .filter(|item| matches!(item, PositionedLayoutItem::GlyphRun(_)))
        .last()
    else {
        return line.metrics().advance;
    };
    let run = source.run();
    let font = font_index(fonts, run);
    let instance = &fonts[font].instance;
    let Some(face) = FontRef::from_index(instance.font.data.as_ref(), instance.font.index as usize) else {
        return line.metrics().advance;
    };
    let glyph = face.charmap().map('…');
    let advance = face
        .glyph_metrics(&instance.coords)
        .scale(run.font_size())
        .advance_width(glyph);
    let available = request.max_width.unwrap_or(f32::MAX).max(0.0) - advance;
    let mut end = line.metrics().offset;
    let mut kept = 0;
    for item in line.items() {
        let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
            continue;
        };
        for glyph in glyph_run.positioned_glyphs() {
            if glyph.x + glyph.advance > available {
                break;
            }
            end = glyph.x + glyph.advance;
            kept += 1;
        }
    }
    let kept = (line_start + kept) as u32;
    glyphs.truncate(kept as usize);
    runs.retain_mut(|run| {
        run.glyphs.end = run.glyphs.end.min(kept);
        run.glyphs.start < run.glyphs.end
    });
    let line_index = runs.last().map_or(0, |run| run.line);
    glyphs.push(Glyph {
        id: glyph,
        position: LogicalPoint::new(end, source.baseline()),
    });
    runs.push(LayoutRun {
        font,
        glyphs: kept..kept + 1,
        span: source.style().brush,
        line: line_index,
    });
    end + advance
}

#[cfg(test)]
mod tests {
    use blit_text::{FontData, FontSettings, Text, TextLayoutEngine as _, TextSpan, TextStyle};

    use super::*;

    const FONT: &[u8] = include_bytes!(env!("BLIT_TEST_FONT"));

    fn setup() -> (Backend, TextStyle<'static>, LayoutRequest) {
        let mut backend = Backend::without_system_fonts();
        let font = backend
            .register_stack(&[Family::embedded([FontData::Static(FONT)])])
            .unwrap();
        let style = TextStyle {
            font,
            size: 16.0,
            weight: 400,
            stretch: 100,
            style: FontStyle::Normal,
            settings: FontSettings::new(),
        };
        let request = LayoutRequest {
            max_width: None,
            max_height: None,
            max_lines: None,
            wrap: TextWrap::None,
            overflow: TextOverflow::Clip,
        };
        (backend, style, request)
    }

    #[test]
    fn empty_and_paragraph_text_keep_valid_line_offsets() {
        let (mut backend, style, request) = setup();
        for (text, starts) in [
            ("", &[0][..]),
            ("a\nb", &[0, 2][..]),
            ("a\r\nb", &[0, 3][..]),
            ("a\u{2029}b", &[0, 4][..]),
        ] {
            let (mut shape, _) = backend.shape(Text {
                text,
                spans: &[TextSpan {
                    range: 0..text.len(),
                    style,
                }],
            });
            let layout = backend.layout(&mut shape, text, request);
            assert_eq!(layout.lines.len(), starts.len(), "{text:?}");
            for (index, &start) in starts.iter().enumerate() {
                assert_eq!(layout.lines[index].text.start, start, "{text:?}");
                let carets = backend.carets(&mut shape, text, request, index);
                assert_eq!(carets[0].byte_offset, start, "{text:?}");
            }
        }
    }

    #[test]
    fn spans_wrap_and_ellipsis_resolve() {
        let (mut backend, style, request) = setup();
        let text = "regular bold";
        let (mut shape, _) = backend.shape(Text {
            text,
            spans: &[
                TextSpan { range: 0..8, style },
                TextSpan {
                    range: 8..text.len(),
                    style: TextStyle {
                        size: 24.0,
                        weight: 700,
                        ..style
                    },
                },
            ],
        });
        let layout = backend.layout(&mut shape, text, request);
        let regular = layout.runs.iter().find(|run| run.span == 0).unwrap().font;
        let bold = layout.runs.iter().find(|run| run.span == 1).unwrap().font;
        assert_eq!(shape.fonts[regular].size, 16.0);
        assert_eq!(shape.fonts[bold].size, 24.0);
        assert_eq!(layout.lines.len(), 1);

        let wrapped = LayoutRequest {
            max_width: Some(40.0),
            ..request
        };
        for wrap in [TextWrap::Word, TextWrap::Character] {
            let layout = backend.layout(&mut shape, text, LayoutRequest { wrap, ..wrapped });
            assert!(layout.lines.len() > 1, "{wrap:?}");
        }
        assert!(shape.character.is_some());

        let layout = backend.layout(
            &mut shape,
            text,
            LayoutRequest {
                overflow: TextOverflow::Ellipsis,
                ..wrapped
            },
        );
        let ellipsis = FontRef::from_index(FONT, 0).unwrap().charmap().map('…');
        assert_eq!(layout.glyphs.last().unwrap().id, ellipsis);
        assert!(layout.size.width <= 40.0);
    }
}
