use std::{borrow::Borrow, cmp::Reverse, mem::size_of, ops::Range};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use blit_raster::{FontFace, Rasterizer};
use blit_text::{
    Caret, Family, FontError, FontStackId, FontStyle, Glyph, GlyphImage, GlyphRequest, LayoutLine, LayoutRequest,
    LayoutRun, PreparedGlyph, TextLayout, TextLayoutEngine, TextOverflow, TextWrap,
};
use fontdue::{
    Font, FontSettings,
    layout::{
        CoordinateSystem, GlyphPosition, HorizontalAlign as FontdueHorizontalAlign, Layout,
        LayoutSettings as FontdueLayoutSettings, LinePosition, TextStyle as FontdueTextStyle, WrapStyle,
    },
};
use unicode_segmentation::UnicodeSegmentation;

/// embedded fonts with basic layout and alpha rasterization
/// font features are ignored by this backend
/// each change of fallback face appends a fontdue run which repositions all prior glyphs
/// so long text alternating between faces is quadratic and better suited to parley
pub struct Backend {
    rasterizer: Rasterizer,
    faces: Vec<Face>,
    stacks: Vec<Vec<Box<[FontCandidate]>>>,
    layout: Layout<(usize, usize)>,
}

struct Face {
    data: FontFace,
    font: Font,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    face: usize,
    glyph: u16,
    size: u32,
    phase: u8,
}

pub struct Shape {
    spans: Box<[ShapeSpan]>,
}

struct ShapeSpan {
    range: Range<usize>,
    face: usize,
    size: f32,
    span: usize,
}

#[derive(Clone, Copy)]
struct FontCandidate {
    face: usize,
    weight: u16,
    stretch: u16,
    style: FontStyle,
}

struct Line<'a> {
    glyphs: &'a [GlyphPosition<(usize, usize)>],
    len: usize,
    width: f32,
    ellipsis: Option<Ellipsis>,
}

#[derive(Clone, Copy)]
struct Ellipsis {
    glyph: u16,
    face: usize,
    span: usize,
    size: f32,
    advance: f32,
}

impl Borrow<Font> for Face {
    fn borrow(&self) -> &Font {
        &self.font
    }
}

impl Backend {
    pub fn new() -> Self {
        Self {
            rasterizer: Rasterizer::default(),
            faces: Vec::new(),
            stacks: Vec::new(),
            layout: Layout::new(CoordinateSystem::PositiveYDown),
        }
    }
}

impl Default for Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLayoutEngine for Backend {
    type Shape = Shape;

    type GlyphKey = GlyphKey;

    fn register_stack(&mut self, families: &[Family]) -> Result<FontStackId, FontError> {
        let mut stack = Vec::new();
        for family in families {
            let Family::Embedded(fonts) = family else {
                return Err(FontError::Unsupported);
            };
            let mut candidates = Vec::new();
            for data in fonts {
                let face_count = ttf_parser::fonts_in_collection(data.as_ref()).unwrap_or(1);
                for face_index in 0..face_count {
                    let font = Font::from_bytes(
                        data.as_ref(),
                        FontSettings {
                            collection_index: face_index,
                            load_substitutions: false,
                            ..FontSettings::default()
                        },
                    )
                    .map_err(|_| FontError::InvalidData)?;
                    let face =
                        ttf_parser::Face::parse(data.as_ref(), face_index).map_err(|_| FontError::InvalidData)?;
                    candidates.push(FontCandidate {
                        face: self.faces.len(),
                        weight: face.weight().to_number(),
                        stretch: match face.width() {
                            ttf_parser::Width::UltraCondensed => 50,
                            ttf_parser::Width::ExtraCondensed => 63,
                            ttf_parser::Width::Condensed => 75,
                            ttf_parser::Width::SemiCondensed => 88,
                            ttf_parser::Width::Normal => 100,
                            ttf_parser::Width::SemiExpanded => 113,
                            ttf_parser::Width::Expanded => 125,
                            ttf_parser::Width::ExtraExpanded => 150,
                            ttf_parser::Width::UltraExpanded => 200,
                        },
                        style: match face.style() {
                            ttf_parser::Style::Normal => FontStyle::Normal,
                            ttf_parser::Style::Italic => FontStyle::Italic,
                            ttf_parser::Style::Oblique => FontStyle::Oblique,
                        },
                    });
                    self.faces.push(Face {
                        data: FontFace::new(data.clone(), face_index).ok_or(FontError::InvalidData)?,
                        font,
                    });
                }
            }
            if candidates.is_empty() {
                return Err(FontError::NotFound);
            }
            stack.push(candidates.into_boxed_slice());
        }
        if stack.is_empty() {
            return Err(FontError::NotFound);
        }
        self.stacks.push(stack);
        Ok(FontStackId(self.stacks.len() as u64))
    }

    fn prepare_glyph(&mut self, shape: &Shape, request: GlyphRequest) -> PreparedGlyph<Self::GlyphKey> {
        let font = &shape.spans[request.font];
        let x = (request.origin.x * 4.0).round() as i32;
        PreparedGlyph {
            key: GlyphKey {
                face: font.face,
                glyph: request.glyph,
                size: (font.size * request.scale).to_bits(),
                phase: x.rem_euclid(4) as u8,
            },
            origin: blit::PhysicalPoint::new(x.div_euclid(4), request.origin.y.round() as i32),
        }
    }

    fn rasterize_glyph(&mut self, key: &Self::GlyphKey) -> Result<GlyphImage, FontError> {
        Ok(self.rasterizer.rasterize(
            &self.faces[key.face].data,
            key.glyph,
            f32::from_bits(key.size),
            key.phase as f32 / 4.0,
        ))
    }

    fn shape(&mut self, text: blit_text::Text<'_>) -> (Shape, usize) {
        let mut faces = Vec::new();
        let mut spans = Vec::<ShapeSpan>::new();
        for (span, range, style) in text.segments() {
            // best face per family in stack order
            faces.clear();
            faces.extend(self.stacks[style.font.0 as usize - 1].iter().filter_map(|family| {
                let candidate = family.iter().min_by_key(|candidate| {
                    (
                        match (style.style, candidate.style) {
                            (requested, candidate) if requested == candidate => 0,
                            (FontStyle::Italic, FontStyle::Oblique) | (FontStyle::Oblique, FontStyle::Italic) => 1,
                            _ => 2,
                        },
                        candidate.stretch.abs_diff(style.stretch),
                        candidate.weight.abs_diff(style.weight),
                        Reverse(candidate.weight),
                    )
                })?;
                Some(candidate.face)
            }));
            let first = *faces.first().expect("empty font stack");
            if range.is_empty() {
                spans.push(ShapeSpan {
                    range,
                    face: first,
                    size: style.size,
                    span,
                });
                continue;
            }
            for (offset, cluster) in text.text[range.clone()].grapheme_indices(true) {
                let face = faces
                    .iter()
                    .copied()
                    .find(|&face| {
                        let font = &self.faces[face].font;
                        cluster.chars().all(|ch| {
                            ch.is_control()
                                || matches!(ch, '\u{200c}'..='\u{200d}' | '\u{fe00}'..='\u{fe0f}' | '\u{e0000}'..='\u{e0fff}')
                                || font.lookup_glyph_index(ch) != 0
                        })
                    })
                    .unwrap_or(first);
                let start = range.start + offset;
                if let Some(last) = spans.last_mut().filter(|last| {
                    last.span == span && last.face == face && last.size.to_bits() == style.size.to_bits()
                }) {
                    last.range.end = start + cluster.len();
                } else {
                    spans.push(ShapeSpan {
                        range: start..start + cluster.len(),
                        face,
                        size: style.size,
                        span,
                    });
                }
            }
        }
        let spans = spans.into_boxed_slice();
        let weight = spans.len() * size_of::<ShapeSpan>();
        (Shape { spans }, weight)
    }

    fn layout(&mut self, shape: &mut Shape, text: &str, request: LayoutRequest) -> TextLayout {
        let (visible_lines, content_height, empty_height) =
            prepare(&mut self.layout, &self.faces, shape, text, request);
        let default = shape.spans.first().expect("text requires a span");
        let Some(source_lines) = self.layout.lines() else {
            if request.max_lines == Some(0) || request.max_height.is_some_and(|height| height < empty_height) {
                return TextLayout::default();
            }
            return TextLayout {
                size: LogicalSize {
                    width: 0.0,
                    height: empty_height,
                },
                lines: Box::new([LayoutLine {
                    bounds: LogicalRect {
                        x: 0.0,
                        y: 0.0,
                        width: 0.0,
                        height: empty_height,
                    },
                    text: 0..0,
                }]),
                ..TextLayout::default()
            };
        };

        let truncated = visible_lines < source_lines.len();

        let source_glyphs = self.layout.glyphs();
        let mut glyphs = Vec::new();
        let mut runs = Vec::new();
        let mut lines = Vec::with_capacity(visible_lines);
        let mut width = 0.0f32;
        for (line_index, line) in source_lines[..visible_lines].iter().enumerate() {
            let source = line_glyphs(source_glyphs, line);
            let prepared = prepare_line(
                &self.faces,
                source,
                request,
                line_index + 1 == visible_lines,
                truncated,
                default.face,
                default.size,
            );
            let line_index = u32::try_from(line_index).expect("too many lines");
            width = width.max(prepared.width);
            let bounds = LogicalRect {
                x: 0.0,
                y: line.baseline_y - line.max_ascent,
                width: prepared.width,
                height: line.max_new_line_size,
            };
            let line_start = glyphs.len() as u32;
            let mut final_offset = prepared
                .glyphs
                .first()
                .map_or(text.len(), |glyph| glyph.user_data.0 + glyph.byte_offset);
            for source in &prepared.glyphs[..prepared.len] {
                final_offset = source.user_data.0 + source.byte_offset + source.parent.len_utf8();
                if source.char_data.is_control() {
                    continue;
                }
                let (_, pen) = glyph_metrics(&self.faces, source);
                let index = glyphs.len() as u32;
                let font = source.user_data.1;
                let span = shape.spans[source.user_data.1].span;
                if let Some(run) = runs.last_mut().filter(|run: &&mut LayoutRun| {
                    run.font == font && run.span == span && run.glyphs.start >= line_start
                }) {
                    run.glyphs.end = index + 1;
                } else {
                    runs.push(LayoutRun {
                        font,
                        span,
                        glyphs: index..index + 1,
                        line: line_index,
                    });
                }
                glyphs.push(Glyph {
                    id: source.key.glyph_index,
                    position: LogicalPoint {
                        x: pen,
                        y: line.baseline_y,
                    },
                });
            }
            if let Some(ellipsis) = prepared.ellipsis {
                let index = glyphs.len() as u32;
                runs.push(LayoutRun {
                    font: ellipsis.span,
                    span: shape.spans[ellipsis.span].span,
                    glyphs: index..index + 1,
                    line: line_index,
                });
                glyphs.push(Glyph {
                    id: ellipsis.glyph,
                    position: LogicalPoint {
                        x: prepared.width - ellipsis.advance,
                        y: line.baseline_y,
                    },
                });
            }
            lines.push(LayoutLine {
                bounds,
                text: prepared
                    .glyphs
                    .first()
                    .map_or(final_offset, |glyph| glyph.user_data.0 + glyph.byte_offset) as u32
                    ..final_offset as u32,
            });
        }

        assert!(glyphs.len() <= u32::MAX as usize, "too many glyphs");
        TextLayout {
            size: LogicalSize {
                width,
                height: content_height,
            },
            glyphs: glyphs.into_boxed_slice(),
            runs: runs.into_boxed_slice(),
            lines: lines.into_boxed_slice(),
        }
    }

    fn carets(&mut self, shape: &mut Shape, text: &str, request: LayoutRequest, line_index: usize) -> Box<[Caret]> {
        let (visible_lines, _, empty_height) = prepare(&mut self.layout, &self.faces, shape, text, request);
        let default = shape.spans.first().expect("text requires a span");
        let Some(source_lines) = self.layout.lines() else {
            if line_index != 0
                || request.max_lines == Some(0)
                || request.max_height.is_some_and(|height| height < empty_height)
            {
                return Box::new([]);
            }
            return Box::new([Caret {
                byte_offset: 0,
                position: LogicalPoint { x: 0.0, y: 0.0 },
                height: empty_height,
            }]);
        };

        let Some(line) = source_lines.get(line_index).filter(|_| line_index < visible_lines) else {
            return Box::new([]);
        };
        let source = line_glyphs(self.layout.glyphs(), line);
        let prepared = prepare_line(
            &self.faces,
            source,
            request,
            line_index + 1 == visible_lines,
            visible_lines < source_lines.len(),
            default.face,
            default.size,
        );
        let y = line.baseline_y - line.max_ascent;
        let mut carets = Vec::new();
        let mut final_offset = prepared
            .glyphs
            .first()
            .map_or(text.len(), |glyph| glyph.user_data.0 + glyph.byte_offset);
        let mut final_x = 0.0;
        for source in &prepared.glyphs[..prepared.len] {
            let (metrics, pen) = glyph_metrics(&self.faces, source);
            let caret = Caret {
                byte_offset: (source.user_data.0 + source.byte_offset) as u32,
                position: LogicalPoint { x: pen, y },
                height: line.max_new_line_size,
            };
            push_caret(&mut carets, caret);
            final_offset = source.user_data.0 + source.byte_offset + source.parent.len_utf8();
            final_x = pen + metrics.advance_width.ceil();
        }
        if prepared.ellipsis.is_some() {
            final_x = prepared.width;
        }
        let final_caret = Caret {
            byte_offset: final_offset as u32,
            position: LogicalPoint { x: final_x, y },
            height: line.max_new_line_size,
        };
        push_caret(&mut carets, final_caret);
        carets.into_boxed_slice()
    }
}

fn prepare(
    layout: &mut Layout<(usize, usize)>,
    faces: &[Face],
    shape: &Shape,
    text: &str,
    request: LayoutRequest,
) -> (usize, f32, f32) {
    assert!(text.len() <= u32::MAX as usize, "text is too long");
    let wrap = request.wrap != TextWrap::None;
    layout.reset(&FontdueLayoutSettings {
        max_width: wrap.then(|| request.max_width.unwrap_or(f32::MAX).max(0.0)),
        max_height: None,
        horizontal_align: FontdueHorizontalAlign::Left,
        line_height: 1.0,
        wrap_style: match request.wrap {
            TextWrap::None | TextWrap::Word => WrapStyle::Word,
            TextWrap::Character => WrapStyle::Letter,
        },
        ..FontdueLayoutSettings::default()
    });
    for (index, span) in shape.spans.iter().enumerate() {
        layout.append(
            faces,
            &FontdueTextStyle::with_user_data(
                &text[span.range.clone()],
                span.size,
                span.face,
                (span.range.start, index),
            ),
        );
    }
    let default = shape.spans.first().expect("text requires a span");
    let empty_height = faces[default.face]
        .font
        .horizontal_line_metrics(default.size)
        .map_or(default.size.max(0.0), |metrics| metrics.new_line_size.ceil().max(0.0));
    let mut visible_lines = 0;
    let mut content_height = 0.0f32;
    let max_lines = request.max_lines.map_or(usize::MAX, usize::from);
    for line in layout.lines().into_iter().flatten().take(max_lines) {
        let bottom = line.baseline_y - line.max_ascent + line.max_new_line_size;
        if request.max_height.is_some_and(|height| bottom > height.max(0.0)) {
            break;
        }
        visible_lines += 1;
        content_height = content_height.max(bottom);
    }
    (visible_lines, content_height, empty_height)
}

fn line_glyphs<'a>(
    glyphs: &'a [GlyphPosition<(usize, usize)>],
    line: &LinePosition,
) -> &'a [GlyphPosition<(usize, usize)>] {
    let end = line.glyph_end.saturating_add(1).min(glyphs.len());
    glyphs.get(line.glyph_start..end).unwrap_or(&[])
}

fn glyph_metrics(faces: &[Face], glyph: &GlyphPosition<(usize, usize)>) -> (fontdue::Metrics, f32) {
    let metrics = if glyph.char_data.is_control() {
        fontdue::Metrics::default()
    } else {
        faces[glyph.font_index]
            .font
            .metrics_indexed(glyph.key.glyph_index, glyph.key.px)
    };
    let pen = glyph.x - metrics.bounds.xmin.floor();
    (metrics, pen)
}

fn push_caret(carets: &mut Vec<Caret>, caret: Caret) {
    if carets.last().is_none_or(|previous| {
        previous.byte_offset != caret.byte_offset || previous.position.x.to_bits() != caret.position.x.to_bits()
    }) {
        carets.push(caret);
    }
}

fn prepare_line<'a>(
    faces: &[Face],
    glyphs: &'a [GlyphPosition<(usize, usize)>],
    request: LayoutRequest,
    last: bool,
    truncated: bool,
    default_face: usize,
    default_size: f32,
) -> Line<'a> {
    let mut width = 0.0f32;
    for glyph in glyphs {
        if glyph.char_data.is_control() {
            continue;
        }
        let (metrics, pen) = glyph_metrics(faces, glyph);
        width = width.max(pen + metrics.advance_width.ceil());
    }
    if request.overflow != TextOverflow::Ellipsis
        || !last
        || !(truncated || request.max_width.is_some_and(|max_width| width > max_width.max(0.0)))
    {
        return Line {
            glyphs,
            len: glyphs.len(),
            width,
            ellipsis: None,
        };
    }

    let available = request.max_width.unwrap_or(width).max(0.0);
    let mut len = glyphs.len();
    let mut ellipsis = Ellipsis {
        glyph: 0,
        face: default_face,
        span: 0,
        size: default_size,
        advance: 0.0,
    };
    loop {
        while len != 0 && glyphs[len - 1].parent.is_whitespace() {
            len -= 1;
        }
        let last = glyphs[..len].last();
        if let Some(glyph) = last.or_else(|| glyphs.first()) {
            ellipsis.face = glyph.font_index;
            ellipsis.span = glyph.user_data.1;
            ellipsis.size = glyph.key.px;
        }
        let font = &faces[ellipsis.face].font;
        ellipsis.glyph = font.lookup_glyph_index('…');
        ellipsis.advance = font.metrics_indexed(ellipsis.glyph, ellipsis.size).advance_width.ceil();
        width = last.map_or(0.0, |glyph| {
            let (metrics, pen) = glyph_metrics(faces, glyph);
            pen + metrics.advance_width.ceil()
        }) + ellipsis.advance;
        if len == 0 || width <= available {
            break;
        }
        len -= 1;
    }
    Line {
        glyphs,
        len,
        width,
        ellipsis: Some(ellipsis),
    }
}

#[cfg(test)]
mod tests {
    use blit_text::{FontData, FontStyle, TextStyle};

    use super::*;

    #[test]
    fn registered_font_produces_common_layout() {
        let mut backend = Backend::new();
        let font = backend
            .register_stack(&[Family::embedded([FontData::Static(include_bytes!(env!(
                "BLIT_TEST_FONT"
            )))])])
            .unwrap();
        let bold = backend
            .register_stack(&[Family::embedded([FontData::Static(include_bytes!(env!(
                "BLIT_TEST_FONT"
            )))])])
            .unwrap();
        let text = "secure approval";
        let regular_style = TextStyle {
            font,
            size: 16.0,
            weight: 400,
            stretch: 100,
            style: FontStyle::Normal,
            settings: blit_text::FontSettings::default(),
        };
        let (mut shape, _) = backend.shape(blit_text::Text {
            text,
            style: regular_style,
            spans: &[
                blit_text::TextSpan {
                    range: 0..7,
                    style: regular_style,
                },
                blit_text::TextSpan {
                    range: 7..text.len(),
                    style: TextStyle {
                        font: bold,
                        size: 24.0,
                        weight: 700,
                        ..regular_style
                    },
                },
            ],
        });
        let request = LayoutRequest {
            max_width: None,
            max_height: None,
            max_lines: None,
            wrap: TextWrap::None,
            overflow: TextOverflow::Clip,
        };
        let layout = backend.layout(&mut shape, text, request);

        assert!(!layout.glyphs.is_empty());
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(shape.spans[0].face, 0);
        assert_eq!(shape.spans[1].face, 1);
        assert!(layout.runs.iter().any(|run| run.span == 0 && run.font == 0));
        assert!(
            layout
                .runs
                .iter()
                .any(|run| run.span == 1 && run.font == 1 && shape.spans[run.font].size == 24.0)
        );
        let carets = backend.carets(&mut shape, text, request, 0);
        assert_eq!(
            carets
                .iter()
                .find(|caret| caret.byte_offset as usize == "secure approval".len())
                .unwrap()
                .height,
            layout.size.height
        );
    }

    #[test]
    fn layout_wraps_and_exposes_valid_ranges() {
        let mut backend = Backend::new();
        let font = backend
            .register_stack(&[Family::embedded([FontData::Static(include_bytes!(env!(
                "BLIT_TEST_FONT"
            )))])])
            .unwrap();
        let style = TextStyle {
            font,
            size: 16.0,
            weight: 400,
            stretch: 100,
            style: FontStyle::Normal,
            settings: blit_text::FontSettings::default(),
        };
        let (mut shape, _) = backend.shape(blit_text::Text {
            text: "one two three",
            style,
            spans: &[blit_text::TextSpan {
                range: 0.."one two three".len(),
                style,
            }],
        });
        let layout = backend.layout(
            &mut shape,
            "one two three",
            LayoutRequest {
                max_width: Some(40.0),
                max_height: None,
                max_lines: None,
                wrap: TextWrap::Word,
                overflow: TextOverflow::Clip,
            },
        );

        assert!(layout.lines.len() > 1);
        for line in &layout.lines {
            assert!(line.text.end as usize <= "one two three".len());
        }
        for run in &layout.runs {
            assert!(run.glyphs.end as usize <= layout.glyphs.len());
        }

        let layout = backend.layout(
            &mut shape,
            "one two three",
            LayoutRequest {
                max_width: Some(40.0),
                max_height: None,
                max_lines: Some(1),
                wrap: TextWrap::Word,
                overflow: TextOverflow::Ellipsis,
            },
        );
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(
            layout.glyphs.last().unwrap().id,
            backend.faces[0].font.lookup_glyph_index('…')
        );
    }
}
