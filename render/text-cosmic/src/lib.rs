use std::{iter, sync::Arc};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use cosmic_text::{
    Align, Attrs, Buffer, Cursor, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, LineIter,
    Metrics, Shaping, Wrap,
    fontdb::{self, Query, Source},
};
use unicode_segmentation::UnicodeSegmentation;

use blit_text::{
    Caret, FontCandidate, FontData, FontError, FontFace, FontFaceId, FontSelectionId, FontStyle,
    Glyph, HorizontalAlign, LayoutLine, LayoutRequest, LayoutRun, SystemFontRequest, TextLayout,
    TextOverflow, TextStyle, TextWrap, VerticalAlign,
};

pub struct Backend {
    fonts: FontSystem,
    faces: Vec<CosmicFace>,
    aliases: Vec<CosmicAlias>,
    selections: Vec<Box<str>>,
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

    fn face(&mut self, cosmic: fontdb::ID) -> Option<FontFaceId> {
        if let Some(index) = self.faces.iter().position(|face| face.cosmic == cosmic) {
            return Some(FontFaceId(u64::try_from(index + 1).ok()?));
        }
        if let Some(alias) = self.aliases.iter().find(|alias| alias.cosmic == cosmic) {
            return Some(alias.face);
        }
        let data = self
            .fonts
            .db()
            .with_face_data(cosmic, |data, face_index| FontFace {
                data: FontData::Shared(Arc::from(data)),
                face_index,
            })?;
        let id = FontFaceId(u64::try_from(self.faces.len() + 1).ok()?);
        self.faces.push(CosmicFace { cosmic, data });
        Some(id)
    }
}

impl Default for Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl blit_text::TextLayoutEngine for Backend {
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
        self.face(cosmic).ok_or(FontError::InvalidData)
    }

    fn register_font(&mut self, data: FontData, face_index: u32) -> Result<FontFaceId, FontError> {
        let ids = self
            .fonts
            .db_mut()
            .load_font_source(Source::Binary(Arc::new(data.clone())));
        let cosmic = ids
            .into_iter()
            .find(|id| {
                self.fonts
                    .db()
                    .face(*id)
                    .is_some_and(|face| face.index == face_index)
            })
            .ok_or(FontError::InvalidData)?;
        let id =
            FontFaceId(u64::try_from(self.faces.len() + 1).map_err(|_| FontError::Unsupported)?);
        self.faces.push(CosmicFace {
            cosmic,
            data: FontFace { data, face_index },
        });
        Ok(id)
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

    fn layout(&mut self, text: &str, style: TextStyle, request: LayoutRequest) -> TextLayout {
        let selection_index = style.font.0.checked_sub(1).expect("invalid font selection") as usize;
        let family = self
            .selections
            .get(selection_index)
            .expect("expired font selection");
        let line_height = style.size * 1.2;
        let height = match (request.max_height, request.max_lines) {
            (Some(height), Some(lines)) => Some(height.min(line_height * f32::from(lines))),
            (Some(height), None) => Some(height),
            (None, Some(lines)) => Some(line_height * f32::from(lines)),
            (None, None) => None,
        };
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(style.size, line_height));
        buffer.set_size(request.max_width, height);
        buffer.set_wrap(match request.wrap {
            TextWrap::None => Wrap::None,
            TextWrap::Word => Wrap::Word,
            TextWrap::Character => Wrap::Glyph,
        });
        buffer.set_ellipsize(match request.overflow {
            TextOverflow::Clip => Ellipsize::None,
            TextOverflow::Ellipsis => Ellipsize::End(match request.max_lines {
                Some(lines) => EllipsizeHeightLimit::Lines(usize::from(lines)),
                None => EllipsizeHeightLimit::Height(request.max_height.unwrap_or(f32::MAX)),
            }),
        });
        let attrs = Attrs::new()
            .family(Family::Name(family))
            .stretch(cosmic_stretch(style.stretch))
            .style(cosmic_style(style.style))
            .weight(fontdb::Weight(style.weight));
        buffer.set_text(
            text,
            &attrs,
            Shaping::Advanced,
            Some(match request.horizontal_align {
                HorizontalAlign::Left => Align::Left,
                HorizontalAlign::Center => Align::Center,
                HorizontalAlign::Right => Align::Right,
            }),
        );
        buffer.shape_until_scroll(&mut self.fonts, false);

        let mut width = 0.0f32;
        let mut content_height = if text.is_empty() { line_height } else { 0.0 };
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            content_height = content_height.max(run.line_top + run.line_height);
        }
        let offset_y = request
            .max_height
            .map_or(0.0, |height| match request.vertical_align {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Center => ((height - content_height) / 2.0).floor(),
                VerticalAlign::Bottom => (height - content_height).floor(),
            });
        let mut line_starts: Vec<_> = LineIter::new(text).map(|(range, _)| range.start).collect();
        if line_starts.is_empty() {
            line_starts.push(0);
        } else if matches!(text.as_bytes().last(), Some(b'\r' | b'\n')) {
            line_starts.push(text.len());
        }

        let mut glyphs = Vec::new();
        let mut runs = Vec::new();
        let mut lines = Vec::new();
        let mut carets = Vec::new();
        let mut line_carets = Vec::new();
        for line in buffer.layout_runs() {
            let bounds = LogicalRect {
                x: 0.0,
                y: line.line_top + offset_y,
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
                {
                    end += 1;
                }
                let face = self
                    .face(source.font_id)
                    .expect("cosmic-text returned invalid font");
                let glyph_start = u32::try_from(glyphs.len()).expect("too many glyphs");
                glyphs.extend(line.glyphs[start..end].iter().map(|glyph| Glyph {
                    id: glyph.glyph_id,
                    position: LogicalPoint {
                        x: glyph.x + glyph.font_size * glyph.x_offset,
                        y: line.line_y + glyph.y - glyph.font_size * glyph.y_offset + offset_y,
                    },
                }));
                runs.push(LayoutRun {
                    face,
                    size: source.font_size,
                    glyphs: glyph_start..u32::try_from(glyphs.len()).expect("too many glyphs"),
                });
                start = end;
            }

            line_carets.clear();
            if line.glyphs.is_empty() {
                line_carets.push(Caret {
                    byte_offset: u32::try_from(line_starts[line.line_i]).expect("text is too long"),
                    position: LogicalPoint {
                        x: 0.0,
                        y: line.line_top + offset_y,
                    },
                    height: line.line_height,
                });
            } else {
                for glyph in line.glyphs {
                    let cluster = &line.text[glyph.start..glyph.end];
                    for index in cluster
                        .grapheme_indices(true)
                        .map(|(index, _)| glyph.start + index)
                        .chain(iter::once(glyph.end))
                    {
                        let Some(x) = line.cursor_position(&Cursor::new(line.line_i, index)) else {
                            continue;
                        };
                        line_carets.push(Caret {
                            byte_offset: u32::try_from(
                                line_starts[line.line_i].saturating_add(index),
                            )
                            .expect("text is too long"),
                            position: LogicalPoint {
                                x,
                                y: line.line_top + offset_y,
                            },
                            height: line.line_height,
                        });
                    }
                }
                line_carets.sort_by(|left, right| {
                    left.byte_offset
                        .cmp(&right.byte_offset)
                        .then_with(|| left.position.x.total_cmp(&right.position.x))
                });
                line_carets.dedup_by(|left, right| {
                    left.byte_offset == right.byte_offset
                        && left.position.x.to_bits() == right.position.x.to_bits()
                });
            }
            let caret_start = u32::try_from(carets.len()).expect("too many carets");
            carets.extend_from_slice(&line_carets);
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
    fn selection_resolves_to_matching_face() {
        let mut backend = Backend::without_system_fonts();
        let data = FontData::Static(include_bytes!(env!("BLIT_TEST_FONT")));
        let regular = backend.register_font(data.clone(), 0).unwrap();
        let bold = backend.register_font(data, 0).unwrap();
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
        let layout = backend.layout(
            "selection",
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

        assert!(layout.runs.iter().all(|run| run.face == bold));
    }
}
