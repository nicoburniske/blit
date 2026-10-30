use std::{iter, mem::size_of, sync::Arc};

use blit::{LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint};
use blit_text::{
    Caret, Family, FontError, FontStackId, FontStyle, Glyph, GlyphImage, GlyphMetrics, GlyphPixels, GlyphRequest,
    LayoutLine, LayoutRequest, LayoutRun, PreparedGlyph, TextLayout, TextOverflow, TextStyle, TextWrap,
};
use cosmic_text::{
    Align, Attrs, BidiParagraphs, Buffer, CacheKey, CacheKeyFlags, Ellipsize, EllipsizeHeightLimit, FontFeatures,
    FontSystem, Metrics, Shaping, SwashCache, SwashContent, Wrap,
    fontdb::{self, Query, Source},
    skrifa::{FontRef, MetadataProvider},
};
use unicode_segmentation::UnicodeSegmentation;

/// system and embedded fonts with shaping features and color glyphs
/// variations and per span language and optical sizing are not exposed by cosmic text
pub struct Backend {
    fonts: FontSystem,
    stacks: Vec<Vec<String>>,
    swash: SwashCache,
}

pub struct Shape {
    buffer: Buffer,
    line_starts: Box<[u32]>,
    empty: bool,
    empty_height: f32,
    fonts: Vec<Instance>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Instance {
    face: fontdb::ID,
    size: u32,
    weight: fontdb::Weight,
    flags: CacheKeyFlags,
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
            stacks: Vec::new(),
            swash: SwashCache::new(),
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
    type GlyphKey = CacheKey;

    fn register_stack(&mut self, families: &[Family]) -> Result<FontStackId, FontError> {
        let mut names = Vec::new();
        for family in families {
            match family {
                Family::Embedded(data) => {
                    let name = format!("\0blit-{}-{}", self.stacks.len(), names.len());
                    let mut count = 0;
                    for data in data {
                        let ids = self
                            .fonts
                            .db_mut()
                            .load_font_source(Source::Binary(Arc::new(data.clone())));
                        if ids.is_empty() {
                            return Err(FontError::InvalidData);
                        }
                        for id in ids {
                            let mut info = self.fonts.db().face(id).ok_or(FontError::InvalidData)?.clone();
                            info.id = fontdb::ID::dummy();
                            info.families = vec![(name.clone(), fontdb::Language::English_UnitedStates)];
                            self.fonts.db_mut().push_face_info(info);
                            count += 1;
                        }
                    }
                    if count == 0 {
                        return Err(FontError::NotFound);
                    }
                    names.push(name);
                }
                Family::Named(name) => names.push(name.clone()),
            }
        }
        if names.is_empty() {
            return Err(FontError::NotFound);
        }
        self.stacks.push(names);
        Ok(FontStackId(self.stacks.len() as u64))
    }

    fn prepare_glyph(&mut self, shape: &Shape, request: GlyphRequest) -> PreparedGlyph<Self::GlyphKey> {
        let font = shape.fonts[request.font];
        let (cache, x, y) = CacheKey::new(
            font.face,
            request.glyph,
            f32::from_bits(font.size) * request.scale,
            (request.origin.x, request.origin.y),
            font.weight,
            font.flags,
        );
        PreparedGlyph {
            key: cache,
            origin: PhysicalPoint::new(x, y),
        }
    }

    fn rasterize_glyph(&mut self, key: &Self::GlyphKey) -> Result<GlyphImage, FontError> {
        let Some(mut image) = self.swash.get_image_uncached(&mut self.fonts, *key) else {
            return Ok(GlyphImage::default());
        };
        let metrics = GlyphMetrics {
            width: image.placement.width.try_into().map_err(|_| FontError::Unsupported)?,
            height: image.placement.height.try_into().map_err(|_| FontError::Unsupported)?,
            left: image.placement.left.try_into().map_err(|_| FontError::Unsupported)?,
            top: (-image.placement.top).try_into().map_err(|_| FontError::Unsupported)?,
        };
        let pixels = match image.content {
            SwashContent::Mask => GlyphPixels::Alpha8(image.data.into_boxed_slice()),
            SwashContent::Color => {
                if matches!(image.source, swash::scale::Source::ColorBitmap(_)) {
                    for pixel in image.data.as_chunks_mut::<4>().0 {
                        for channel in 0..3 {
                            pixel[channel] = (u16::from(pixel[channel]) * u16::from(pixel[3]) / 255) as u8;
                        }
                    }
                }
                GlyphPixels::Rgba8(image.data.into_boxed_slice())
            }
            SwashContent::SubpixelMask => return Err(FontError::Unsupported),
        };
        Ok(GlyphImage { metrics, pixels })
    }

    fn shape(&mut self, text: blit_text::Text<'_>) -> (Shape, usize) {
        assert!(text.text.len() <= u32::MAX as usize, "text is too long");
        let stacks = &self.stacks;
        let fonts = &mut self.fonts;
        let mut attrs = |style: TextStyle<'_>| {
            let names = &stacks[style.font.0 as usize - 1];
            let candidates = names
                .iter()
                .filter_map(|name| {
                    let id = fonts.db().query(&Query {
                        families: &[fontdb::Family::Name(name)],
                        weight: fontdb::Weight(style.weight),
                        stretch: cosmic_stretch(style.stretch),
                        style: cosmic_style(style.style),
                    })?;
                    let index = fonts.db().face(id)?.index;
                    Some((name.as_str(), fonts.get_font(id, fontdb::Weight(style.weight))?, index))
                })
                .collect::<Vec<_>>();
            let mut features = FontFeatures::new();
            for (tag, value) in style.settings.features {
                features.set(cosmic_text::FeatureTag::new(tag), *value);
            }
            let attrs = Attrs::new()
                .family(fontdb::Family::Name(names.first().expect("empty font stack")))
                .metrics(Metrics::relative(style.size, 1.2))
                .stretch(cosmic_stretch(style.stretch))
                .style(cosmic_style(style.style))
                .weight(fontdb::Weight(style.weight))
                .font_features(features);
            (attrs, candidates)
        };
        let default = text.spans.first().expect("text requires a span").style;
        let default_attrs = attrs(default).0;
        let mut buffer = Buffer::new_empty(Metrics::relative(default.size, 1.2));
        buffer.set_rich_text(
            text.segments().flat_map(|(index, range, style)| {
                let (attrs, candidates) = attrs(style);
                let mut clusters = text.text[range.clone()].grapheme_indices(true).peekable();
                // family of the peeked cluster that ended the previous run
                let mut pending = None;
                iter::from_fn(move || {
                    let select = |cluster: &str| {
                        let mut fallback = None;
                        for (name, font, index) in &candidates {
                            let Ok(font) = FontRef::from_index(font.data(), *index) else {
                                continue;
                            };
                            let charmap = font.charmap();
                            let mut variants = true;
                            let mut previous = '\0';
                            if !cluster.chars().all(|ch| {
                                if matches!(ch, '\u{fe00}'..='\u{fe0f}' | '\u{e0100}'..='\u{e01ef}') {
                                    variants &= charmap.map_variant(previous, ch).is_some();
                                    return true;
                                }
                                previous = ch;
                                ch.is_control()
                                    || matches!(ch, '\u{200c}'..='\u{200d}' | '\u{e0000}'..='\u{e0fff}')
                                    || charmap.map(ch).is_some_and(|glyph| glyph.to_u32() != 0)
                            }) {
                                continue;
                            }
                            if variants {
                                return Some(*name);
                            }
                            fallback.get_or_insert(*name);
                        }
                        fallback
                    };
                    let (start, cluster) = clusters.next()?;
                    let family = pending.take().unwrap_or_else(|| select(cluster));
                    let mut end = start + cluster.len();
                    while let Some(&(offset, cluster)) = clusters.peek() {
                        let next = select(cluster);
                        if next != family {
                            pending = Some(next);
                            break;
                        }
                        end = offset + cluster.len();
                        clusters.next();
                    }
                    let mut attrs = attrs.clone().metadata(index);
                    if let Some(family) = family {
                        attrs = attrs.family(fontdb::Family::Name(family));
                    }
                    Some((&text.text[range.start + start..range.start + end], attrs))
                })
            }),
            &default_attrs,
            Shaping::Advanced,
            Some(Align::Left),
        );
        buffer.shape_until_scroll(&mut self.fonts, false);
        let line_starts = BidiParagraphs::new(text.text)
            .map(|line| (line.as_ptr() as usize - text.text.as_ptr() as usize) as u32)
            .chain(text.text.is_empty().then_some(0))
            .collect::<Box<[_]>>();
        let weight = line_starts.len() * size_of::<u32>()
            + buffer.lines.len() * size_of::<cosmic_text::BufferLine>()
            + text.text.len()
                * (2 + size_of::<cosmic_text::ShapeGlyph>()
                    + size_of::<cosmic_text::LayoutGlyph>()
                    + size_of::<Instance>());
        (
            Shape {
                buffer,
                line_starts,
                empty: text.text.is_empty(),
                empty_height: default.size * 1.2,
                fonts: Vec::new(),
            },
            weight,
        )
    }

    fn layout(&mut self, shape: &mut Shape, _text: &str, request: LayoutRequest) -> TextLayout {
        prepare(shape, &mut self.fonts, request);
        let max_lines = request.max_lines.map_or(usize::MAX, usize::from);
        let mut width = 0.0f32;
        let mut content_height = if shape.empty { shape.empty_height } else { 0.0 };
        let mut glyphs = Vec::new();
        let mut runs = Vec::new();
        let mut lines = Vec::new();
        for line in shape.buffer.layout_runs().take(max_lines) {
            width = width.max(line.line_w);
            content_height = content_height.max(line.line_top + line.line_height);
            let line_start = *shape.line_starts.get(line.line_i).expect("invalid line") as usize;
            let line_index = lines.len() as u32;
            let mut start = 0;
            while start < line.glyphs.len() {
                let source = &line.glyphs[start];
                let mut end = start + 1;
                while end < line.glyphs.len()
                    && line.glyphs[end].font_id == source.font_id
                    && line.glyphs[end].font_size.to_bits() == source.font_size.to_bits()
                    && line.glyphs[end].font_weight == source.font_weight
                    && line.glyphs[end].cache_key_flags == source.cache_key_flags
                    && line.glyphs[end].metadata == source.metadata
                {
                    end += 1;
                }
                let instance = Instance {
                    face: source.font_id,
                    size: source.font_size.to_bits(),
                    weight: source.font_weight,
                    flags: source.cache_key_flags,
                };
                let font = shape
                    .fonts
                    .iter()
                    .position(|font| *font == instance)
                    .unwrap_or_else(|| {
                        shape.fonts.push(instance);
                        shape.fonts.len() - 1
                    });
                let glyph_start = glyphs.len() as u32;
                glyphs.extend(line.glyphs[start..end].iter().map(|glyph| Glyph {
                    id: glyph.glyph_id,
                    position: LogicalPoint::new(
                        glyph.x + glyph.font_size * glyph.x_offset,
                        line.line_y + glyph.y - glyph.font_size * glyph.y_offset,
                    ),
                }));
                runs.push(LayoutRun {
                    font,
                    glyphs: glyph_start..glyphs.len() as u32,
                    span: source.metadata,
                    line: line_index,
                });
                start = end;
            }
            let text_start = line.glyphs.iter().map(|glyph| glyph.start).min().unwrap_or(0);
            let text_end = line.glyphs.iter().map(|glyph| glyph.end).max().unwrap_or(0);
            lines.push(LayoutLine {
                bounds: LogicalRect::new(0.0, line.line_top, line.line_w, line.line_height),
                text: (line_start + text_start) as u32..(line_start + text_end) as u32,
            });
        }
        TextLayout {
            size: LogicalSize::new(width, content_height),
            glyphs: glyphs.into_boxed_slice(),
            runs: runs.into_boxed_slice(),
            lines: lines.into_boxed_slice(),
        }
    }

    fn carets(&mut self, shape: &mut Shape, _text: &str, request: LayoutRequest, line_index: usize) -> Box<[Caret]> {
        prepare(shape, &mut self.fonts, request);
        let max_lines = request.max_lines.map_or(usize::MAX, usize::from);
        let Some(line) = shape.buffer.layout_runs().take(max_lines).nth(line_index) else {
            return Box::new([]);
        };
        let line_start = *shape
            .line_starts
            .get(line.line_i)
            .expect("cosmic-text returned invalid line");
        if line.glyphs.is_empty() {
            return Box::new([Caret {
                byte_offset: line_start,
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
                        byte_offset: (line_start as usize + index) as u32,
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
        carets.into_iter().map(|(_, caret)| caret).collect()
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
    use blit_text::{FontData, TextLayoutEngine as _};

    use super::*;

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
            settings: blit_text::FontSettings::default(),
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

    const FONT: &[u8] = include_bytes!(env!("BLIT_TEST_FONT"));

    #[test]
    fn empty_and_paragraph_text_keep_valid_line_offsets() {
        let (mut backend, style, request) = setup();
        for (text, starts) in [
            ("", &[0][..]),
            ("a\nb", &[0, 2][..]),
            ("a\r\nb", &[0, 3][..]),
            ("a\u{2029}b", &[0, 4][..]),
        ] {
            let (mut shape, _) = backend.shape(blit_text::Text {
                text,
                spans: &[blit_text::TextSpan {
                    range: 0..text.len(),
                    style,
                }],
            });
            let layout = backend.layout(&mut shape, text, request);
            assert_eq!(layout.lines.len(), starts.len());
            for (index, &start) in starts.iter().enumerate() {
                assert_eq!(layout.lines[index].text.start, start);
                let carets = backend.carets(&mut shape, text, request, index);
                assert_eq!(carets[0].byte_offset, start);
            }
        }
    }

    #[test]
    fn selection_and_size_resolve_per_span() {
        let (mut backend, style, request) = setup();
        let bold = backend
            .register_stack(&[Family::embedded([FontData::Static(FONT)])])
            .unwrap();
        let text = "regular bold";
        let (mut shape, _) = backend.shape(blit_text::Text {
            text,
            spans: &[
                blit_text::TextSpan { range: 0..8, style },
                blit_text::TextSpan {
                    range: 8..text.len(),
                    style: TextStyle {
                        font: bold,
                        size: 24.0,
                        weight: 700,
                        ..style
                    },
                },
            ],
        });
        let layout = backend.layout(&mut shape, text, request);
        let regular_index = layout.runs.iter().find(|run| run.span == 0).unwrap().font;
        let bold_index = layout.runs.iter().find(|run| run.span == 1).unwrap().font;
        let regular = shape.fonts[regular_index];
        let bold = shape.fonts[bold_index];
        assert_ne!(regular.face, bold.face);
        assert_eq!(f32::from_bits(regular.size), 16.0);
        assert_eq!(f32::from_bits(bold.size), 24.0);
        assert!(layout.lines[0].bounds.height > 16.0 * 1.2);
        let count = shape.fonts.len();
        backend.layout(
            &mut shape,
            text,
            LayoutRequest {
                max_width: Some(40.0),
                wrap: TextWrap::Word,
                ..request
            },
        );
        assert!(shape.fonts[regular_index] == regular && shape.fonts[bold_index] == bold);
        assert_eq!(shape.fonts.len(), count);
    }
}
