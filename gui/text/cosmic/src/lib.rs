use std::{iter, mem::size_of, sync::Arc};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use cosmic_text::{
    Align, Attrs, Buffer, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, LineIter, Metrics,
    Shaping, Wrap,
    fontdb::{self, Query, Source},
};
use unicode_segmentation::UnicodeSegmentation;

use blit_text::{
    Caret, FontCandidate, FontData, FontError, FontFace, FontFaceId, FontSelectionId, FontStyle,
    Glyph, LayoutLine, LayoutRequest, LayoutRun, SystemFontRequest, TextLayout, TextOverflow,
    TextStyle, TextWrap,
};

pub struct Backend {
    fonts: FontSystem,
    faces: Vec<CosmicFace>,
    aliases: Vec<CosmicAlias>,
    selections: Vec<Box<str>>,
}

pub struct Shape {
    buffer: Buffer,
    line_starts: Box<[u32]>,
    empty: bool,
    empty_height: f32,
}

struct CosmicFace {
    cosmic: fontdb::ID,
    data: FontFace,
}

struct CosmicAlias {
    cosmic: fontdb::ID,
    face: FontFaceId,
}

impl Backend {
    pub fn new() -> Self {
        Self::with_font_system(FontSystem::new())
    }

    pub fn without_system_fonts() -> Self {
        Self::with_font_system(FontSystem::new_with_locale_and_db(
            "en-US".into(),
            fontdb::Database::new(),
        ))
    }

    fn with_font_system(fonts: FontSystem) -> Self {
        Self {
            fonts,
            faces: Vec::new(),
            aliases: Vec::new(),
            selections: Vec::new(),
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

    fn system_font(&mut self, request: SystemFontRequest<'_>) -> Result<FontFaceId, FontError> {
        let family = [Family::Name(request.family)];
        let cosmic = self
            .fonts
            .db()
            .query(&Query {
                families: &family,
                weight: fontdb::Weight(request.weight),
                stretch: cosmic_stretch(request.stretch),
                style: cosmic_style(request.style),
            })
            .ok_or(FontError::NotFound)?;
        face(self.fonts.db(), &mut self.faces, &self.aliases, cosmic).ok_or(FontError::InvalidData)
    }

    fn register_font(&mut self, data: FontData) -> Result<Vec<FontFaceId>, FontError> {
        let faces = self
            .fonts
            .db_mut()
            .load_font_source(Source::Binary(Arc::new(data.clone())));
        if faces.is_empty() {
            return Err(FontError::InvalidData);
        }
        let mut registered = Vec::new();
        for cosmic in faces {
            let face_index = self
                .fonts
                .db()
                .face(cosmic)
                .ok_or(FontError::InvalidData)?
                .index;
            let id = FontFaceId(
                u64::try_from(self.faces.len() + 1).map_err(|_| FontError::Unsupported)?,
            );
            self.faces.push(CosmicFace {
                cosmic,
                data: FontFace {
                    data: data.clone(),
                    face_index,
                },
            });
            registered.push(id);
        }
        Ok(registered)
    }

    fn register_font_selection(
        &mut self,
        candidates: &[FontCandidate],
    ) -> Result<FontSelectionId, FontError> {
        if candidates.is_empty() {
            return Err(FontError::NotFound);
        }
        for candidate in candidates {
            let index = usize::try_from(candidate.face.0)
                .ok()
                .and_then(|index| index.checked_sub(1))
                .ok_or(FontError::NotFound)?;
            let face = self.faces.get(index).ok_or(FontError::NotFound)?;
            if self
                .fonts
                .db()
                .face(face.cosmic)
                .is_none_or(|face| face.families.is_empty())
            {
                return Err(FontError::InvalidData);
            }
        }

        let id = FontSelectionId(
            u64::try_from(self.selections.len() + 1).map_err(|_| FontError::Unsupported)?,
        );
        // expose configured candidates to cosmic as one private family
        let family = format!("\0blit-{}", id.0).into_boxed_str();
        for candidate in candidates {
            let index = usize::try_from(candidate.face.0).unwrap() - 1;
            let mut info = self
                .fonts
                .db()
                .face(self.faces[index].cosmic)
                .unwrap()
                .clone();
            info.id = fontdb::ID::dummy();
            info.families[0].0 = family.to_string();
            info.families.truncate(1);
            info.weight = fontdb::Weight(candidate.weight);
            info.stretch = cosmic_stretch(candidate.stretch);
            info.style = cosmic_style(candidate.style);
            let cosmic = self.fonts.db_mut().push_face_info(info);
            self.aliases.push(CosmicAlias {
                cosmic,
                face: candidate.face,
            });
        }
        self.selections.push(family);
        Ok(id)
    }

    fn font_face(&self, face: FontFaceId) -> Option<&FontFace> {
        let index = usize::try_from(face.0).ok()?.checked_sub(1)?;
        self.faces.get(index).map(|face| &face.data)
    }

    fn shape(&mut self, text: blit_text::Text<'_>) -> (Shape, usize) {
        let attrs = |style: TextStyle| {
            let index = style.font.0.checked_sub(1).expect("invalid font selection") as usize;
            Attrs::new()
                .family(Family::Name(&self.selections[index]))
                .metrics(Metrics::relative(style.size, 1.2))
                .stretch(cosmic_stretch(style.stretch))
                .style(cosmic_style(style.style))
                .weight(fontdb::Weight(style.weight))
        };
        let default_style = text.spans.first().expect("text requires a span").style;
        let default_attrs = attrs(default_style);
        let mut buffer = Buffer::new_empty(Metrics::relative(default_style.size, 1.2));
        buffer.set_metrics(Metrics::relative(default_style.size, 1.2));
        buffer.set_rich_text(
            text.segments()
                .map(|(index, range, style)| (&text.text[range], attrs(style).metadata(index))),
            &default_attrs,
            Shaping::Advanced,
            Some(Align::Left),
        );
        buffer.shape_until_scroll(&mut self.fonts, false);
        let mut line_starts = LineIter::new(text.text)
            .map(|(range, _)| u32::try_from(range.start).expect("text is too long"))
            .collect::<Vec<_>>();
        if line_starts.is_empty() {
            line_starts.push(0);
        }
        let weight = line_starts.len() * size_of::<u32>()
            + text.text.len()
                * (2 + size_of::<cosmic_text::ShapeGlyph>()
                    + size_of::<cosmic_text::LayoutGlyph>());
        (
            Shape {
                buffer,
                line_starts: line_starts.into_boxed_slice(),
                empty: text.text.is_empty(),
                empty_height: default_style.size * 1.2,
            },
            weight,
        )
    }

    fn layout(&mut self, shape: &mut Shape, _text: &str, request: LayoutRequest) -> TextLayout {
        prepare(shape, &mut self.fonts, request);
        let max_lines = request.max_lines.map_or(usize::MAX, usize::from);

        let mut width = 0.0f32;
        let mut content_height = if shape.empty { shape.empty_height } else { 0.0 };
        for run in shape.buffer.layout_runs().take(max_lines) {
            width = width.max(run.line_w);
            content_height = content_height.max(run.line_top + run.line_height);
        }

        let mut glyphs = Vec::new();
        let mut runs = Vec::new();
        let mut lines = Vec::new();
        for line in shape.buffer.layout_runs().take(max_lines) {
            let line_start = usize::try_from(
                *shape
                    .line_starts
                    .get(line.line_i)
                    .expect("cosmic-text returned invalid line"),
            )
            .unwrap();
            let line_index = u32::try_from(lines.len()).expect("too many lines");
            let bounds = LogicalRect {
                x: 0.0,
                y: line.line_top,
                width: line.line_w,
                height: line.line_height,
            };
            let mut start = 0;
            while start < line.glyphs.len() {
                let source = &line.glyphs[start];
                let mut end = start + 1;
                while end < line.glyphs.len()
                    && line.glyphs[end].font_id == source.font_id
                    && line.glyphs[end].font_size.to_bits() == source.font_size.to_bits()
                    && line.glyphs[end].metadata == source.metadata
                {
                    end += 1;
                }
                let face = face(
                    self.fonts.db(),
                    &mut self.faces,
                    &self.aliases,
                    source.font_id,
                )
                .expect("cosmic-text returned invalid font");
                let glyph_start = u32::try_from(glyphs.len()).expect("too many glyphs");
                glyphs.extend(line.glyphs[start..end].iter().map(|glyph| Glyph {
                    id: glyph.glyph_id,
                    position: LogicalPoint {
                        x: glyph.x + glyph.font_size * glyph.x_offset,
                        y: line.line_y + glyph.y - glyph.font_size * glyph.y_offset,
                    },
                }));
                runs.push(LayoutRun {
                    face,
                    size: source.font_size,
                    glyphs: glyph_start..u32::try_from(glyphs.len()).expect("too many glyphs"),
                    span: source.metadata,
                    line: line_index,
                });
                start = end;
            }
            let text_start = line
                .glyphs
                .iter()
                .map(|glyph| glyph.start)
                .min()
                .unwrap_or(0);
            let text_end = line.glyphs.iter().map(|glyph| glyph.end).max().unwrap_or(0);
            lines.push(LayoutLine {
                bounds,
                text: u32::try_from(line_start + text_start).expect("text is too long")
                    ..u32::try_from(line_start + text_end).expect("text is too long"),
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
        _text: &str,
        request: LayoutRequest,
        line_index: usize,
    ) -> Box<[Caret]> {
        prepare(shape, &mut self.fonts, request);
        let max_lines = request.max_lines.map_or(usize::MAX, usize::from);
        let Some(line) = shape.buffer.layout_runs().take(max_lines).nth(line_index) else {
            return Box::new([]);
        };
        let line_start = usize::try_from(
            *shape
                .line_starts
                .get(line.line_i)
                .expect("cosmic-text returned invalid line"),
        )
        .unwrap();
        if line.glyphs.is_empty() {
            return Box::new([Caret {
                byte_offset: u32::try_from(line_start).expect("text is too long"),
                position: LogicalPoint {
                    x: 0.0,
                    y: line.line_top,
                },
                height: line.line_height,
            }]);
        }

        let mut carets = Vec::new();
        for glyph in line.glyphs {
            let cluster = &line.text[glyph.start..glyph.end];
            let count = cluster.graphemes(true).count();
            for (position, index) in cluster
                .grapheme_indices(true)
                .map(|(index, _)| glyph.start + index)
                .chain(iter::once(glyph.end))
                .enumerate()
            {
                let end = index == glyph.end;
                let offset = if end {
                    glyph.w
                } else {
                    glyph.w * (position as f32) / (count as f32)
                };
                carets.push((
                    end,
                    Caret {
                        byte_offset: u32::try_from(line_start.saturating_add(index))
                            .expect("text is too long"),
                        position: LogicalPoint {
                            x: if glyph.level.is_rtl() {
                                glyph.x + glyph.w - offset
                            } else {
                                glyph.x + offset
                            },
                            y: line.line_top,
                        },
                        height: line.line_height,
                    },
                ));
            }
        }
        carets.sort_by_key(|(end, caret)| (caret.byte_offset, *end));
        carets.dedup_by_key(|(_, caret)| caret.byte_offset);
        carets
            .into_iter()
            .map(|(_, caret)| caret)
            .collect::<Vec<_>>()
            .into_boxed_slice()
    }
}

fn prepare(shape: &mut Shape, fonts: &mut FontSystem, request: LayoutRequest) {
    shape.buffer.set_size(request.max_width, request.max_height);
    shape.buffer.set_wrap(match request.wrap {
        TextWrap::None => Wrap::None,
        TextWrap::Word => Wrap::Word,
        TextWrap::Character => Wrap::Glyph,
    });
    shape.buffer.set_ellipsize(match request.overflow {
        TextOverflow::Clip => Ellipsize::None,
        TextOverflow::Ellipsis => Ellipsize::End(match request.max_lines {
            Some(lines) => EllipsizeHeightLimit::Lines(usize::from(lines)),
            None => EllipsizeHeightLimit::Height(request.max_height.unwrap_or(f32::MAX)),
        }),
    });
    shape.buffer.shape_until_scroll(fonts, false);
}

fn face(
    db: &fontdb::Database,
    faces: &mut Vec<CosmicFace>,
    aliases: &[CosmicAlias],
    cosmic: fontdb::ID,
) -> Option<FontFaceId> {
    if let Some(index) = faces.iter().position(|face| face.cosmic == cosmic) {
        return Some(FontFaceId(u64::try_from(index + 1).ok()?));
    }
    if let Some(alias) = aliases.iter().find(|alias| alias.cosmic == cosmic) {
        return Some(alias.face);
    }
    let data = db.with_face_data(cosmic, |data, face_index| FontFace {
        data: FontData::Shared(Arc::from(data)),
        face_index,
    })?;
    let id = FontFaceId(u64::try_from(faces.len() + 1).ok()?);
    faces.push(CosmicFace { cosmic, data });
    Some(id)
}

fn cosmic_stretch(stretch: u16) -> fontdb::Stretch {
    match stretch {
        0..=56 => fontdb::Stretch::UltraCondensed,
        57..=68 => fontdb::Stretch::ExtraCondensed,
        69..=81 => fontdb::Stretch::Condensed,
        82..=93 => fontdb::Stretch::SemiCondensed,
        94..=106 => fontdb::Stretch::Normal,
        107..=118 => fontdb::Stretch::SemiExpanded,
        119..=137 => fontdb::Stretch::Expanded,
        138..=175 => fontdb::Stretch::ExtraExpanded,
        _ => fontdb::Stretch::UltraExpanded,
    }
}

fn cosmic_style(style: FontStyle) -> fontdb::Style {
    match style {
        FontStyle::Normal => fontdb::Style::Normal,
        FontStyle::Italic => fontdb::Style::Italic,
        FontStyle::Oblique => fontdb::Style::Oblique,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blit_text::TextLayoutEngine as _;

    #[test]
    fn selection_and_size_resolve_per_span() {
        let mut backend = Backend::without_system_fonts();
        let data = FontData::Static(include_bytes!(env!("BLIT_TEST_FONT")));
        let regular = backend.register_font(data.clone()).unwrap()[0];
        let bold = backend.register_font(data).unwrap()[0];
        let font = backend
            .register_font_selection(&[
                FontCandidate {
                    face: regular,
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
        let text = "regular bold";
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
                    range: 0..8,
                    style: regular_style,
                },
                blit_text::TextSpan {
                    range: 8..text.len(),
                    style: TextStyle {
                        size: 24.0,
                        weight: 700,
                        ..regular_style
                    },
                },
            ],
        });
        let layout = backend.layout(
            &mut shape,
            text,
            LayoutRequest {
                max_width: None,
                max_height: None,
                max_lines: None,
                wrap: TextWrap::None,
                overflow: TextOverflow::Clip,
            },
        );

        assert!(
            layout
                .runs
                .iter()
                .any(|run| run.span == 0 && run.face == regular)
        );
        assert!(
            layout
                .runs
                .iter()
                .any(|run| run.span == 1 && run.face == bold && run.size == 24.0)
        );
        assert!(layout.lines[0].bounds.height > 16.0 * 1.2);
    }
}
