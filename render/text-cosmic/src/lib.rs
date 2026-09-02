use std::{iter, sync::Arc};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use cosmic_text::{
    Align, Attrs, Buffer, Cursor, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, LineIter,
    Metrics, Shaping, Wrap,
    fontdb::{self, Query, Source},
};
use unicode_segmentation::UnicodeSegmentation;

use blit_text::{
    Caret, FontData, FontError, FontFace, FontId, FontStyle, Glyph, HorizontalAlign, LayoutLine,
    LayoutRequest, LayoutRun, SystemFontRequest, TextLayout, TextOverflow, TextStyle, TextWrap,
    VerticalAlign,
};

pub struct Backend {
    fonts: FontSystem,
    faces: Vec<CosmicFace>,
}

struct CosmicFace {
    cosmic: fontdb::ID,
    data: FontFace,
    family: Box<str>,
    weight: fontdb::Weight,
    stretch: fontdb::Stretch,
    style: fontdb::Style,
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
        }
    }

    fn face(&mut self, cosmic: fontdb::ID) -> Option<FontId> {
        if let Some(index) = self.faces.iter().position(|face| face.cosmic == cosmic) {
            return Some(FontId(u64::try_from(index + 1).ok()?));
        }
        let info = self.fonts.db().face(cosmic)?;
        let family = info.families.first()?.0.clone().into_boxed_str();
        let weight = info.weight;
        let stretch = info.stretch;
        let style = info.style;
        let data = self
            .fonts
            .db()
            .with_face_data(cosmic, |data, face_index| FontFace {
                data: FontData::Shared(Arc::from(data)),
                face_index,
            })?;
        let id = FontId(u64::try_from(self.faces.len() + 1).ok()?);
        self.faces.push(CosmicFace {
            cosmic,
            data,
            family,
            weight,
            stretch,
            style,
        });
        Some(id)
    }
}

impl Default for Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl blit_text::TextLayoutEngine for Backend {
    fn system_font(&mut self, request: SystemFontRequest<'_>) -> Result<FontId, FontError> {
        let family = [Family::Name(request.family)];
        let cosmic = self
            .fonts
            .db()
            .query(&Query {
                families: &family,
                weight: fontdb::Weight(request.weight),
                stretch: match request.stretch {
                    0..=56 => fontdb::Stretch::UltraCondensed,
                    57..=68 => fontdb::Stretch::ExtraCondensed,
                    69..=81 => fontdb::Stretch::Condensed,
                    82..=93 => fontdb::Stretch::SemiCondensed,
                    94..=106 => fontdb::Stretch::Normal,
                    107..=118 => fontdb::Stretch::SemiExpanded,
                    119..=137 => fontdb::Stretch::Expanded,
                    138..=175 => fontdb::Stretch::ExtraExpanded,
                    _ => fontdb::Stretch::UltraExpanded,
                },
                style: match request.style {
                    FontStyle::Normal => fontdb::Style::Normal,
                    FontStyle::Italic => fontdb::Style::Italic,
                    FontStyle::Oblique => fontdb::Style::Oblique,
                },
            })
            .ok_or(FontError::NotFound)?;
        self.face(cosmic).ok_or(FontError::InvalidData)
    }

    fn register_font(&mut self, data: FontData, face_index: u32) -> Result<FontId, FontError> {
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
        let info = self.fonts.db().face(cosmic).ok_or(FontError::InvalidData)?;
        let id = FontId(u64::try_from(self.faces.len() + 1).map_err(|_| FontError::Unsupported)?);
        self.faces.push(CosmicFace {
            cosmic,
            data: FontFace { data, face_index },
            family: info
                .families
                .first()
                .ok_or(FontError::InvalidData)?
                .0
                .clone()
                .into_boxed_str(),
            weight: info.weight,
            stretch: info.stretch,
            style: info.style,
        });
        Ok(id)
    }

    fn font(&self, font: FontId) -> Option<&FontFace> {
        let index = usize::try_from(font.0).ok()?.checked_sub(1)?;
        self.faces.get(index).map(|face| &face.data)
    }

    fn layout(&mut self, text: &str, style: TextStyle, request: LayoutRequest) -> TextLayout {
        let face_index = style.font.0.checked_sub(1).expect("invalid font") as usize;
        let face = self.faces.get(face_index).expect("expired font");
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
            TextWrap::Glyph => Wrap::Glyph,
        });
        buffer.set_ellipsize(match request.overflow {
            TextOverflow::Clip => Ellipsize::None,
            TextOverflow::Ellipsis => Ellipsize::End(match request.max_lines {
                Some(lines) => EllipsizeHeightLimit::Lines(usize::from(lines)),
                None => EllipsizeHeightLimit::Height(request.max_height.unwrap_or(f32::MAX)),
            }),
        });
        let attrs = Attrs::new()
            .family(Family::Name(&face.family))
            .stretch(face.stretch)
            .style(face.style)
            .weight(face.weight);
        buffer.set_text(
            text,
            &attrs,
            Shaping::Advanced,
            Some(match request.horizontal_align {
                HorizontalAlign::Start => Align::Left,
                HorizontalAlign::Center => Align::Center,
                HorizontalAlign::End => Align::Right,
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
                VerticalAlign::Start => 0.0,
                VerticalAlign::Center => ((height - content_height) / 2.0).floor(),
                VerticalAlign::End => (height - content_height).floor(),
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
                let font = self
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
                    font,
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
