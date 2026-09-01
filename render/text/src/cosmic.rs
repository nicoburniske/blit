use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::Arc,
};

use blit::{LogicalPoint, LogicalRect, LogicalSize};
use cosmic_text::{
    Align, Attrs, Buffer, Cursor, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, LineIter,
    Metrics, Shaping, Wrap,
    fontdb::{self, Query, Source},
};

use crate::{
    Backend, FontData, FontError, FontFace, FontId, FontStyle, Glyph, GlyphRun, GlyphRunVisitor,
    HorizontalAlign, SystemFontRequest, TextId, TextLayoutId, TextLayoutRequest, TextOverflow,
    TextStyle, TextWrap, VerticalAlign,
};

pub struct CosmicBackend {
    fonts: FontSystem,
    faces: Vec<CosmicFace>,
    face_ids: HashMap<fontdb::ID, FontId>,
    texts: Vec<CosmicText>,
    text_ids: HashMap<u64, Vec<usize>>,
    layouts: Vec<CosmicLayout>,
}

struct CosmicFace {
    data: FontFace,
    family: Box<str>,
    stretch: fontdb::Stretch,
    style: fontdb::Style,
}

struct CosmicText {
    text: Box<str>,
    line_starts: Box<[usize]>,
    style: TextStyle,
}

struct CosmicLayout {
    text: usize,
    buffer: Buffer,
    runs: Box<[OwnedRun]>,
    size: LogicalSize,
    offset_x: f32,
    offset_y: f32,
}

struct OwnedRun {
    font: FontId,
    size: f32,
    bounds: LogicalRect,
    glyphs: Box<[Glyph]>,
}

impl CosmicBackend {
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
            face_ids: HashMap::new(),
            texts: Vec::new(),
            text_ids: HashMap::new(),
            layouts: Vec::new(),
        }
    }

    fn face(&mut self, cosmic: fontdb::ID) -> Option<FontId> {
        if let Some(id) = self.face_ids.get(&cosmic) {
            return Some(*id);
        }
        let info = self.fonts.db().face(cosmic)?;
        let family = info.families.first()?.0.clone().into_boxed_str();
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
            data,
            family,
            stretch,
            style,
        });
        self.face_ids.insert(cosmic, id);
        Some(id)
    }

    fn layout(&self, layout: TextLayoutId) -> &CosmicLayout {
        let index = layout.0.checked_sub(1).expect("invalid text layout") as usize;
        self.layouts.get(index).expect("expired text layout")
    }
}

impl Default for CosmicBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for CosmicBackend {
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
            data: FontFace { data, face_index },
            family: info
                .families
                .first()
                .ok_or(FontError::InvalidData)?
                .0
                .clone()
                .into_boxed_str(),
            stretch: info.stretch,
            style: info.style,
        });
        self.face_ids.insert(cosmic, id);
        Ok(id)
    }

    fn font(&self, font: FontId) -> Option<FontFace> {
        let index = usize::try_from(font.0).ok()?.checked_sub(1)?;
        self.faces.get(index).map(|face| face.data.clone())
    }

    fn text(&mut self, text: &str, style: TextStyle) -> TextId {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        style.font.hash(&mut hasher);
        style.size.to_bits().hash(&mut hasher);
        style.weight.hash(&mut hasher);
        let digest = hasher.finish();
        if let Some(indices) = self.text_ids.get(&digest)
            && let Some(index) = indices.iter().copied().find(|index| {
                let cached = &self.texts[*index];
                cached.text.as_ref() == text && cached.style == style
            })
        {
            return TextId(u64::try_from(index + 1).expect("too many texts"));
        }

        let mut line_starts: Vec<_> = LineIter::new(text).map(|(range, _)| range.start).collect();
        if line_starts.is_empty() {
            line_starts.push(0);
        } else if matches!(text.as_bytes().last(), Some(b'\r' | b'\n')) {
            line_starts.push(text.len());
        }
        let index = self.texts.len();
        self.texts.push(CosmicText {
            text: text.into(),
            line_starts: line_starts.into_boxed_slice(),
            style,
        });
        self.text_ids.entry(digest).or_default().push(index);
        TextId(u64::try_from(index + 1).expect("too many texts"))
    }

    fn layout(&mut self, request: TextLayoutRequest) -> TextLayoutId {
        let text_index = request.text.0.checked_sub(1).expect("invalid text") as usize;
        let line_height;
        let mut buffer;
        {
            let text = self.texts.get(text_index).expect("expired text");
            let face_index = text.style.font.0.checked_sub(1).expect("invalid font") as usize;
            let face = self.faces.get(face_index).expect("expired font");
            line_height = text.style.size * 1.2;
            let height = match (request.max_height, request.max_lines) {
                (Some(height), Some(lines)) => Some(height.min(line_height * f32::from(lines))),
                (Some(height), None) => Some(height),
                (None, Some(lines)) => Some(line_height * f32::from(lines)),
                (None, None) => None,
            };
            buffer = Buffer::new(&mut self.fonts, Metrics::new(text.style.size, line_height));
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
                .weight(cosmic_text::Weight(text.style.weight));
            buffer.set_text(
                &text.text,
                &attrs,
                Shaping::Advanced,
                Some(match request.horizontal_align {
                    HorizontalAlign::Start => Align::Left,
                    HorizontalAlign::Center => Align::Center,
                    HorizontalAlign::End => Align::Right,
                    HorizontalAlign::Justify => Align::Justified,
                }),
            );
            buffer.shape_until_scroll(&mut self.fonts, false);
        }

        let mut width = 0.0f32;
        let mut content_height = if self.texts[text_index].text.is_empty() {
            line_height
        } else {
            0.0
        };
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

        let mut runs = Vec::<OwnedRun>::new();
        for run in buffer.layout_runs() {
            let bounds = LogicalRect {
                x: -request.offset_x,
                y: run.line_top + offset_y,
                width: run.line_w,
                height: run.line_height,
            };
            let mut start = 0;
            while start < run.glyphs.len() {
                let source = &run.glyphs[start];
                let mut end = start + 1;
                while end < run.glyphs.len()
                    && run.glyphs[end].font_id == source.font_id
                    && run.glyphs[end].font_size.to_bits() == source.font_size.to_bits()
                {
                    end += 1;
                }
                let font = self
                    .face(source.font_id)
                    .expect("cosmic-text returned invalid font");
                let glyphs = run.glyphs[start..end]
                    .iter()
                    .map(|glyph| Glyph {
                        id: u32::from(glyph.glyph_id),
                        position: LogicalPoint {
                            x: glyph.x + glyph.font_size * glyph.x_offset - request.offset_x,
                            y: run.line_y + glyph.y - glyph.font_size * glyph.y_offset + offset_y,
                        },
                        advance: glyph.w,
                        cluster: u32::try_from(
                            self.texts[text_index].line_starts[run.line_i]
                                .saturating_add(glyph.start),
                        )
                        .expect("text is too long"),
                    })
                    .collect();
                runs.push(OwnedRun {
                    font,
                    size: source.font_size,
                    bounds,
                    glyphs,
                });
                start = end;
            }
        }

        self.layouts.push(CosmicLayout {
            text: text_index,
            buffer,
            runs: runs.into_boxed_slice(),
            size: LogicalSize {
                width,
                height: content_height,
            },
            offset_x: request.offset_x,
            offset_y,
        });
        TextLayoutId(u64::try_from(self.layouts.len()).expect("too many text layouts"))
    }

    fn size(&self, layout: TextLayoutId) -> LogicalSize {
        self.layout(layout).size
    }

    fn hit_test(&self, layout: TextLayoutId, position: LogicalPoint) -> usize {
        let layout = self.layout(layout);
        let Some(cursor) = layout
            .buffer
            .hit(position.x + layout.offset_x, position.y - layout.offset_y)
        else {
            return 0;
        };
        self.texts[layout.text].line_starts[cursor.line].saturating_add(cursor.index)
    }

    fn cursor_rect(&self, layout: TextLayoutId, byte_offset: usize) -> LogicalRect {
        let layout = self.layout(layout);
        let text = &self.texts[layout.text];
        let byte_offset = byte_offset.min(text.text.len());
        let line = text
            .line_starts
            .partition_point(|start| *start <= byte_offset)
            .saturating_sub(1);
        let index = byte_offset
            .saturating_sub(text.line_starts[line])
            .min(layout.buffer.lines[line].text().len());
        let cursor = Cursor::new(line, index);
        let (x, y) = layout.buffer.cursor_position(&cursor).unwrap_or((0.0, 0.0));
        let height = layout
            .buffer
            .layout_runs()
            .find(|run| run.line_i == line && y == run.line_top)
            .map_or(text.style.size * 1.2, |run| run.line_height);
        LogicalRect {
            x: x - layout.offset_x,
            y: y + layout.offset_y,
            width: 0.0,
            height,
        }
    }

    fn visit_runs(&self, layout: TextLayoutId, visitor: &mut GlyphRunVisitor<'_>) {
        for run in &self.layout(layout).runs {
            visitor.push(GlyphRun {
                font: run.font,
                size: run.size,
                bounds: run.bounds,
                glyphs: &run.glyphs,
            });
        }
    }

    fn finish_frame(&mut self) {
        self.layouts.clear();
    }
}
