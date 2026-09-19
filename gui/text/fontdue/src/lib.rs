use std::{borrow::Borrow, cmp::Reverse, mem::size_of, ops::Range};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use blit_text::{
    Caret, FontCandidate, FontData, FontError, FontFace, FontFaceId, FontSelectionId, FontStyle,
    Glyph, LayoutLine, LayoutRequest, LayoutRun, TextLayout, TextLayoutEngine, TextOverflow,
    TextStyle, TextWrap,
};
use fontdue::{
    Font, FontSettings,
    layout::{
        CoordinateSystem, GlyphPosition, HorizontalAlign as FontdueHorizontalAlign, Layout,
        LayoutSettings as FontdueLayoutSettings, LinePosition, TextStyle as FontdueTextStyle,
        WrapStyle,
    },
};

pub struct Backend {
    faces: Vec<Face>,
    selections: Vec<Box<[FontCandidate]>>,
    layout: Layout<(usize, usize)>,
}

struct Face {
    data: FontFace,
    font: Font,
}

pub struct Shape {
    spans: Box<[ShapeSpan]>,
}

struct ShapeSpan {
    range: Range<usize>,
    face: usize,
    size: f32,
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
            faces: Vec::new(),
            selections: Vec::new(),
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

    fn register_font(&mut self, data: FontData) -> Result<Vec<FontFaceId>, FontError> {
        let face_count = ttf_parser::fonts_in_collection(data.as_ref()).unwrap_or(1);
        let mut registered = Vec::new();
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
            let id = FontFaceId(
                u64::try_from(self.faces.len() + 1).map_err(|_| FontError::Unsupported)?,
            );
            self.faces.push(Face {
                data: FontFace {
                    data: data.clone(),
                    face_index,
                },
                font,
            });
            registered.push(id);
        }
        if registered.is_empty() {
            return Err(FontError::InvalidData);
        }
        Ok(registered)
    }

    fn register_font_selection(
        &mut self,
        candidates: &[FontCandidate],
    ) -> Result<FontSelectionId, FontError> {
        if candidates.is_empty()
            || candidates
                .iter()
                .any(|candidate| self.font_face(candidate.face).is_none())
        {
            return Err(FontError::NotFound);
        }
        let id = FontSelectionId(
            u64::try_from(self.selections.len() + 1).map_err(|_| FontError::Unsupported)?,
        );
        self.selections.push(candidates.into());
        Ok(id)
    }

    fn font_face(&self, face: FontFaceId) -> Option<&FontFace> {
        let index = usize::try_from(face.0).ok()?.checked_sub(1)?;
        self.faces.get(index).map(|face| &face.data)
    }

    fn shape(&mut self, text: blit_text::Text<'_>) -> (Shape, usize) {
        let resolve = |style: TextStyle| {
            let selection_index = usize::try_from(style.font.0)
                .expect("invalid font selection")
                .checked_sub(1)
                .expect("invalid font selection");
            let candidate = *self
                .selections
                .get(selection_index)
                .expect("expired font selection")
                .iter()
                .min_by_key(|candidate| {
                    (
                        match (style.style, candidate.style) {
                            (requested, candidate) if requested == candidate => 0,
                            (FontStyle::Italic, FontStyle::Oblique)
                            | (FontStyle::Oblique, FontStyle::Italic) => 1,
                            _ => 2,
                        },
                        candidate.stretch.abs_diff(style.stretch),
                        candidate.weight.abs_diff(style.weight),
                        Reverse(candidate.weight),
                    )
                })
                .expect("empty font selection");
            usize::try_from(candidate.face.0)
                .expect("invalid font")
                .checked_sub(1)
                .expect("invalid font")
        };
        let spans = text
            .segments()
            .map(|(_, range, style)| ShapeSpan {
                range,
                face: resolve(style),
                size: style.size,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let weight = spans.len() * size_of::<ShapeSpan>();
        (Shape { spans }, weight)
    }

    fn layout(&mut self, shape: &mut Shape, text: &str, request: LayoutRequest) -> TextLayout {
        let (visible_lines, content_height, empty_height) =
            prepare(&mut self.layout, &self.faces, shape, text, request);
        let default = shape.spans.first().expect("text requires a span");
        let face_index = default.face;
        let Some(source_lines) = self.layout.lines() else {
            if request.max_lines == Some(0)
                || request
                    .max_height
                    .is_some_and(|height| height < empty_height)
            {
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
                face_index,
                default.size,
            );
            width = width.max(prepared.width);
            let bounds = LogicalRect {
                x: 0.0,
                y: line.baseline_y - line.max_ascent,
                width: prepared.width,
                height: line.max_new_line_size,
            };
            let line_start = u32::try_from(glyphs.len()).expect("too many glyphs");
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
                let index = u32::try_from(glyphs.len()).expect("too many glyphs");
                let face = FontFaceId(source.font_index as u64 + 1);
                let span = source.user_data.1;
                if let Some(run) = runs.last_mut().filter(|run: &&mut LayoutRun| {
                    run.face == face
                        && run.size.to_bits() == source.key.px.to_bits()
                        && run.span == span
                        && run.glyphs.start >= line_start
                }) {
                    run.glyphs.end = index + 1;
                } else {
                    runs.push(LayoutRun {
                        face,
                        size: source.key.px,
                        span,
                        glyphs: index..index + 1,
                        line: u32::try_from(line_index).expect("too many lines"),
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
                let index = u32::try_from(glyphs.len()).expect("too many glyphs");
                runs.push(LayoutRun {
                    face: FontFaceId(ellipsis.face as u64 + 1),
                    size: ellipsis.size,
                    span: ellipsis.span,
                    glyphs: index..index + 1,
                    line: u32::try_from(line_index).expect("too many lines"),
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
                text: u32::try_from(
                    prepared
                        .glyphs
                        .first()
                        .map_or(final_offset, |glyph| glyph.user_data.0 + glyph.byte_offset),
                )
                .expect("text is too long")
                    ..u32::try_from(final_offset).expect("text is too long"),
            });
        }

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

    fn carets(
        &mut self,
        shape: &mut Shape,
        text: &str,
        request: LayoutRequest,
        line_index: usize,
    ) -> Box<[Caret]> {
        let (visible_lines, _, empty_height) =
            prepare(&mut self.layout, &self.faces, shape, text, request);
        let default = shape.spans.first().expect("text requires a span");
        let Some(source_lines) = self.layout.lines() else {
            if line_index != 0
                || request.max_lines == Some(0)
                || request
                    .max_height
                    .is_some_and(|height| height < empty_height)
            {
                return Box::new([]);
            }
            return Box::new([Caret {
                byte_offset: 0,
                position: LogicalPoint { x: 0.0, y: 0.0 },
                height: empty_height,
            }]);
        };

        let Some(line) = source_lines
            .get(line_index)
            .filter(|_| line_index < visible_lines)
        else {
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
                byte_offset: u32::try_from(source.user_data.0 + source.byte_offset)
                    .expect("text is too long"),
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
            byte_offset: u32::try_from(final_offset).expect("text is too long"),
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
        .map_or(default.size.max(0.0), |metrics| {
            metrics.new_line_size.ceil().max(0.0)
        });
    let mut visible_lines = 0;
    let mut content_height = 0.0f32;
    let max_lines = request.max_lines.map_or(usize::MAX, usize::from);
    for line in layout.lines().into_iter().flatten().take(max_lines) {
        let bottom = line.baseline_y - line.max_ascent + line.max_new_line_size;
        if request
            .max_height
            .is_some_and(|height| bottom > height.max(0.0))
        {
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
        previous.byte_offset != caret.byte_offset
            || previous.position.x.to_bits() != caret.position.x.to_bits()
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
        || !(truncated
            || request
                .max_width
                .is_some_and(|max_width| width > max_width.max(0.0)))
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
        ellipsis.advance = font
            .metrics_indexed(ellipsis.glyph, ellipsis.size)
            .advance_width
            .ceil();
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
    use super::*;
    use blit_text::FontStyle;

    #[test]
    fn registered_font_produces_common_layout() {
        let mut backend = Backend::new();
        let face = backend
            .register_font(FontData::Static(include_bytes!(env!("BLIT_TEST_FONT"))))
            .unwrap()[0];
        let bold = backend
            .register_font(FontData::Static(include_bytes!(env!("BLIT_TEST_FONT"))))
            .unwrap()[0];
        let font = backend
            .register_font_selection(&[
                FontCandidate {
                    face,
                    weight: 400,
                    stretch: 100,
                    style: FontStyle::Normal,
                },
                FontCandidate {
                    face: bold,
                    weight: 700,
                    stretch: 100,
                    style: FontStyle::Normal,
                },
            ])
            .unwrap();
        let text = "secure approval";
        let regular_style = TextStyle {
            font,
            size: 16.0,
            weight: 400,
            stretch: 100,
            style: FontStyle::Normal,
        };
        let (mut shape, _) = backend.shape(blit_text::Text {
            text,
            spans: &[
                blit_text::TextSpan {
                    range: 0..7,
                    style: regular_style,
                },
                blit_text::TextSpan {
                    range: 7..text.len(),
                    style: TextStyle {
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
        assert!(
            layout
                .runs
                .iter()
                .any(|run| run.span == 0 && run.face == face)
        );
        assert!(
            layout
                .runs
                .iter()
                .any(|run| run.span == 1 && run.face == bold && run.size == 24.0)
        );
        assert_eq!(backend.font_face(bold).unwrap().face_index, 0);
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
        let face = backend
            .register_font(FontData::Static(include_bytes!(env!("BLIT_TEST_FONT"))))
            .unwrap()[0];
        let font = backend
            .register_font_selection(&[FontCandidate {
                face,
                weight: 400,
                stretch: 100,
                style: FontStyle::Normal,
            }])
            .unwrap();
        let style = TextStyle {
            font,
            size: 16.0,
            weight: 400,
            stretch: 100,
            style: FontStyle::Normal,
        };
        let (mut shape, _) = backend.shape(blit_text::Text {
            text: "one two three",
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
