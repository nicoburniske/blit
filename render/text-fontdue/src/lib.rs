use std::{borrow::Borrow, cmp::Reverse};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use blit_text::{
    Caret, FontCandidate, FontData, FontError, FontFace, FontFaceId, FontSelectionId, Glyph,
    HorizontalAlign, LayoutLine, LayoutRequest, LayoutRun, TextLayout, TextLayoutEngine,
    TextOverflow, TextStyle, TextWrap, VerticalAlign,
};
use fontdue::{
    Font, FontSettings,
    layout::{
        CoordinateSystem, HorizontalAlign as FontdueHorizontalAlign, Layout,
        LayoutSettings as FontdueLayoutSettings, TextStyle as FontdueTextStyle,
        VerticalAlign as FontdueVerticalAlign, WrapStyle,
    },
};

pub struct Backend {
    faces: Vec<Face>,
    selections: Vec<Box<[FontCandidate]>>,
    layout: Layout,
}

struct Face {
    data: FontFace,
    font: Font,
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
    fn register_font(&mut self, data: FontData, face_index: u32) -> Result<FontFaceId, FontError> {
        let font = Font::from_bytes(
            data.as_ref(),
            FontSettings {
                collection_index: face_index,
                load_substitutions: false,
                ..FontSettings::default()
            },
        )
        .map_err(|_| FontError::InvalidData)?;
        let id =
            FontFaceId(u64::try_from(self.faces.len() + 1).map_err(|_| FontError::Unsupported)?);
        self.faces.push(Face {
            data: FontFace { data, face_index },
            font,
        });
        Ok(id)
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

    fn layout(&mut self, text: &str, style: TextStyle, request: LayoutRequest) -> TextLayout {
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
                    candidate.style != style.style,
                    candidate.stretch.abs_diff(style.stretch),
                    candidate.weight.abs_diff(style.weight),
                    Reverse(candidate.weight),
                )
            })
            .expect("empty font selection");
        let face_index = usize::try_from(candidate.face.0)
            .expect("invalid font")
            .checked_sub(1)
            .expect("invalid font");
        let face = self.faces.get(face_index).expect("expired font");
        let wrap = request.wrap != TextWrap::None;
        self.layout.reset(&FontdueLayoutSettings {
            max_width: wrap.then(|| request.max_width.unwrap_or(f32::MAX).max(0.0)),
            max_height: None,
            horizontal_align: FontdueHorizontalAlign::Left,
            vertical_align: FontdueVerticalAlign::Top,
            line_height: 1.0,
            wrap_style: match request.wrap {
                TextWrap::None | TextWrap::Word => WrapStyle::Word,
                TextWrap::Character => WrapStyle::Letter,
            },
            ..FontdueLayoutSettings::default()
        });
        self.layout.append(
            &self.faces,
            &FontdueTextStyle::new(text, style.size, face_index),
        );

        let empty_height = face
            .font
            .horizontal_line_metrics(style.size)
            .map_or(style.size.max(0.0), |metrics| {
                metrics.new_line_size.ceil().max(0.0)
            });
        let Some(source_lines) = self.layout.lines() else {
            if request.max_lines == Some(0)
                || request
                    .max_height
                    .is_some_and(|height| height < empty_height)
            {
                return TextLayout {
                    size: LogicalSize::default(),
                    glyphs: Box::new([]),
                    runs: Box::new([]),
                    lines: Box::new([]),
                    carets: Box::new([]),
                };
            }
            let offset_y = request
                .max_height
                .map_or(0.0, |height| match request.vertical_align {
                    VerticalAlign::Top => 0.0,
                    VerticalAlign::Center => ((height - empty_height) / 2.0).floor(),
                    VerticalAlign::Bottom => (height - empty_height).floor(),
                });
            return TextLayout {
                size: LogicalSize {
                    width: 0.0,
                    height: empty_height,
                },
                glyphs: Box::new([]),
                runs: Box::new([]),
                lines: Box::new([LayoutLine {
                    bounds: LogicalRect {
                        x: 0.0,
                        y: offset_y,
                        width: 0.0,
                        height: empty_height,
                    },
                    carets: 0..1,
                }]),
                carets: Box::new([Caret {
                    byte_offset: 0,
                    position: LogicalPoint {
                        x: 0.0,
                        y: offset_y,
                    },
                    height: empty_height,
                }]),
            };
        };

        let mut visible_lines = 0usize;
        let mut content_height = 0.0f32;
        let max_lines = request.max_lines.map_or(usize::MAX, usize::from);
        for line in source_lines {
            if visible_lines == max_lines {
                break;
            }
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
        let truncated = visible_lines < source_lines.len();
        let offset_y = request
            .max_height
            .map_or(0.0, |height| match request.vertical_align {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Center => ((height - content_height) / 2.0).floor(),
                VerticalAlign::Bottom => (height - content_height).floor(),
            });

        let source_glyphs = self.layout.glyphs();
        let mut glyphs = Vec::new();
        let mut runs = Vec::new();
        let mut lines = Vec::with_capacity(visible_lines);
        let mut carets = Vec::new();
        let mut width = 0.0f32;
        for (line_index, line) in source_lines[..visible_lines].iter().enumerate() {
            let source_end = line
                .glyph_end
                .checked_add(1)
                .unwrap_or(source_glyphs.len())
                .min(source_glyphs.len());
            let source = source_glyphs
                .get(line.glyph_start..source_end)
                .unwrap_or(&[]);
            let mut source_width = 0.0f32;
            for glyph in source {
                let font = &self.faces[glyph.font_index].font;
                let metrics = font.metrics_indexed(glyph.key.glyph_index, glyph.key.px);
                let pen = glyph.x - metrics.bounds.xmin.floor();
                source_width = source_width.max(pen + metrics.advance_width.ceil());
            }

            let ellipsize = request.overflow == TextOverflow::Ellipsis
                && line_index + 1 == visible_lines
                && (truncated
                    || request
                        .max_width
                        .is_some_and(|max_width| source_width > max_width.max(0.0)));
            let ellipsis = face.font.lookup_glyph_index('…');
            let ellipsis_metrics = face.font.metrics_indexed(ellipsis, style.size);
            let ellipsis_advance = ellipsis_metrics.advance_width.ceil();
            let mut source_len = source.len();
            let mut displayed_width = source_width;
            if ellipsize {
                let available =
                    request.max_width.unwrap_or(source_width).max(0.0) - ellipsis_advance;
                source_len = 0;
                for glyph in source {
                    let font = &self.faces[glyph.font_index].font;
                    let metrics = font.metrics_indexed(glyph.key.glyph_index, glyph.key.px);
                    let pen = glyph.x - metrics.bounds.xmin.floor();
                    let end = pen + metrics.advance_width.ceil();
                    if end > available {
                        break;
                    }
                    source_len += 1;
                }
                while source_len != 0 && source[source_len - 1].parent.is_whitespace() {
                    source_len -= 1;
                }
                displayed_width = source[..source_len].last().map_or(0.0, |glyph| {
                    let font = &self.faces[glyph.font_index].font;
                    let metrics = font.metrics_indexed(glyph.key.glyph_index, glyph.key.px);
                    glyph.x - metrics.bounds.xmin.floor() + metrics.advance_width.ceil()
                }) + ellipsis_advance;
            }
            let align_x =
                request
                    .max_width
                    .map_or(0.0, |max_width| match request.horizontal_align {
                        HorizontalAlign::Left => 0.0,
                        HorizontalAlign::Center => ((max_width - displayed_width) / 2.0).floor(),
                        HorizontalAlign::Right => (max_width - displayed_width).floor(),
                    });
            width = width.max(displayed_width);
            let bounds = LogicalRect {
                x: align_x,
                y: line.baseline_y - line.max_ascent + offset_y,
                width: displayed_width,
                height: line.max_new_line_size,
            };
            let glyph_start = u32::try_from(glyphs.len()).expect("too many glyphs");
            let caret_start = u32::try_from(carets.len()).expect("too many carets");
            let mut final_offset = source.first().map_or(text.len(), |glyph| glyph.byte_offset);
            let mut final_x = 0.0;
            for source in &source[..source_len] {
                let font = &self.faces[source.font_index].font;
                let metrics = font.metrics_indexed(source.key.glyph_index, source.key.px);
                let pen = source.x - metrics.bounds.xmin.floor();
                let advance = metrics.advance_width.ceil();
                let byte_offset = u32::try_from(source.byte_offset).expect("text is too long");
                let caret = Caret {
                    byte_offset,
                    position: LogicalPoint {
                        x: pen + align_x,
                        y: bounds.y,
                    },
                    height: bounds.height,
                };
                if carets.last().is_none_or(|previous: &Caret| {
                    previous.byte_offset != caret.byte_offset
                        || previous.position.x.to_bits() != caret.position.x.to_bits()
                }) {
                    carets.push(caret);
                }
                glyphs.push(Glyph {
                    id: source.key.glyph_index,
                    position: LogicalPoint {
                        x: pen + align_x,
                        y: line.baseline_y + offset_y,
                    },
                });
                final_offset = source.byte_offset + source.parent.len_utf8();
                final_x = pen + advance;
            }
            if ellipsize {
                glyphs.push(Glyph {
                    id: ellipsis,
                    position: LogicalPoint {
                        x: displayed_width - ellipsis_advance + align_x,
                        y: line.baseline_y + offset_y,
                    },
                });
                final_x = displayed_width;
            }
            let final_caret = Caret {
                byte_offset: u32::try_from(final_offset).expect("text is too long"),
                position: LogicalPoint {
                    x: final_x + align_x,
                    y: bounds.y,
                },
                height: bounds.height,
            };
            if carets.last().is_none_or(|previous| {
                previous.byte_offset != final_caret.byte_offset
                    || previous.position.x.to_bits() != final_caret.position.x.to_bits()
            }) {
                carets.push(final_caret);
            }
            let glyph_end = u32::try_from(glyphs.len()).expect("too many glyphs");
            if glyph_start != glyph_end {
                runs.push(LayoutRun {
                    face: candidate.face,
                    size: style.size,
                    glyphs: glyph_start..glyph_end,
                });
            }
            lines.push(LayoutLine {
                bounds,
                carets: caret_start..u32::try_from(carets.len()).expect("too many carets"),
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
            carets: carets.into_boxed_slice(),
        }
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
            .register_font(FontData::Static(include_bytes!(env!("BLIT_TEST_FONT"))), 0)
            .unwrap();
        let bold = backend
            .register_font(FontData::Static(include_bytes!(env!("BLIT_TEST_FONT"))), 0)
            .unwrap();
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
        let layout = backend.layout(
            "secure approval",
            TextStyle {
                font,
                size: 16.0,
                weight: 700,
                stretch: 100,
                style: FontStyle::Normal,
            },
            LayoutRequest {
                max_width: None,
                max_height: None,
                max_lines: None,
                wrap: TextWrap::None,
                overflow: TextOverflow::Clip,
                horizontal_align: HorizontalAlign::Left,
                vertical_align: VerticalAlign::Top,
            },
        );

        assert!(!layout.glyphs.is_empty());
        assert_eq!(layout.lines.len(), 1);
        assert!(layout.runs.iter().all(|run| run.face == bold));
        assert_eq!(backend.font_face(bold).unwrap().face_index, 0);
        assert_eq!(
            layout
                .carets
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
            .register_font(FontData::Static(include_bytes!(env!("BLIT_TEST_FONT"))), 0)
            .unwrap();
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
        let layout = backend.layout(
            "one two three",
            style,
            LayoutRequest {
                max_width: Some(40.0),
                max_height: None,
                max_lines: None,
                wrap: TextWrap::Word,
                overflow: TextOverflow::Clip,
                horizontal_align: HorizontalAlign::Left,
                vertical_align: VerticalAlign::Top,
            },
        );

        assert!(layout.lines.len() > 1);
        for line in &layout.lines {
            assert!(line.carets.end as usize <= layout.carets.len());
        }
        for run in &layout.runs {
            assert!(run.glyphs.end as usize <= layout.glyphs.len());
        }

        let layout = backend.layout(
            "one two three",
            style,
            LayoutRequest {
                max_width: Some(40.0),
                max_height: None,
                max_lines: Some(1),
                wrap: TextWrap::Word,
                overflow: TextOverflow::Ellipsis,
                horizontal_align: HorizontalAlign::Left,
                vertical_align: VerticalAlign::Top,
            },
        );
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(
            layout.glyphs.last().unwrap().id,
            backend.faces[0].font.lookup_glyph_index('…')
        );
    }
}
