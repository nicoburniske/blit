use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
    ops::Range,
};

use blit::{LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint};
use blit_gui::text::{
    FontId, FontSettings, FontStyle, GlyphFormat, GlyphImage, GlyphKey, GlyphMetrics, HorizontalAlign, OpticalSize,
    PositionedGlyph, Span, TextOptions, TextOverflow, TextRequest, TextRun, TextStyle, TextSystem, TextWrap,
    VerticalAlign,
};
use blit_util::cache::{DeferredCache, Equivalent, Scale};
pub use parley;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontFamilyName, FontFeature, FontFeatures, FontVariation,
    FontVariations, FontWeight, FontWidth, GenericFamily, Language, Layout, LayoutContext, LineHeight,
    PositionedLayoutItem, StyleProperty, WordBreak, setting::Tag,
};
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, StrikeWith, image::Content},
    zeno::{Angle, Format, Transform, Vector},
};

const DEFAULT_CACHE_CAPACITY: usize = 1024;

/// parley and swash text system
/// parley has no ellipsis so trailing glyphs are trimmed and a single ellipsis glyph from the last font is appended
/// this keeps byte offsets of the source text unlike reshaping a truncated string
pub struct Backend {
    pub fonts: FontContext,
    layouts: LayoutContext<usize>,
    scale: ScaleContext,
    stacks: Vec<(FontId, Box<[FontFamilyName<'static>]>)>,
    sources: DeferredCache<SourceKey, CachedSource, UnitWeight>,
    font_faces: Vec<FontFace>,
    next_source: u32,
    subpixel_phases: u8,
}

impl Backend {
    pub fn new(fonts: FontContext) -> Self {
        Self {
            fonts,
            layouts: LayoutContext::new(),
            scale: ScaleContext::new(),
            stacks: vec![(FontId::default(), Box::new([FontFamilyName::Generic(GenericFamily::SansSerif)]))],
            sources: DeferredCache::new(UnitWeight, DEFAULT_CACHE_CAPACITY),
            font_faces: Vec::new(),
            next_source: 1,
            subpixel_phases: 4,
        }
    }

    pub fn font(
        mut self,
        id: FontId,
        families: impl IntoIterator<Item = impl Into<FontFamilyName<'static>>>,
    ) -> Self {
        let mut names = Vec::new();
        for family in families {
            let family = family.into();
            if !names.contains(&family) {
                names.push(family);
            }
        }
        let names = names.into_boxed_slice();
        if let Some((_, existing)) = self.stacks.iter_mut().find(|(stack_id, _)| *stack_id == id) {
            *existing = names;
        } else {
            self.stacks.push((id, names));
        }
        self
    }

    /// maximum number of retained text sources across frames
    pub fn cache_capacity(mut self, entries: usize) -> Self {
        self.sources = DeferredCache::new(UnitWeight, entries);
        self
    }

    /// horizontal subpixel phases per pixel
    pub fn subpixel_phases(mut self, phases: u8) -> Self {
        self.subpixel_phases = phases.max(1);
        self
    }
}

impl Default for Backend {
    fn default() -> Self {
        Self::new(FontContext::new())
    }
}

impl TextSystem for Backend {
    fn rich_text(&mut self, text: &str, spans: &[Span<'_>], style: TextStyle) -> TextRun {
        if !style.size.is_finite() || style.size <= 0.0 {
            return TextRun::default();
        }
        let Some(base_stack) = self.stacks.iter().position(|(id, _)| *id == style.font) else {
            return TextRun::default();
        };
        let default = [Span::new(0..text.len())];
        let mut spans = if spans.is_empty() { &default } else { spans };
        let mut end = 0;
        for span in spans {
            if span.range.start != end
                || span.range.end < end
                || !text.is_char_boundary(span.range.end)
                || span.style.size.is_some_and(|size| !size.is_finite() || size <= 0.0)
                || span
                    .style
                    .font
                    .is_some_and(|id| !self.stacks.iter().any(|(stack_id, _)| *stack_id == id))
            {
                spans = &default;
                break;
            }
            end = span.range.end;
        }
        if end != text.len() {
            spans = &default;
        }
        let query = SourceQuery {
            text,
            spans,
            style: ResolvedStyle {
                stack: base_stack,
                size: style.size,
                weight: style.weight,
                stretch: style.stretch,
                style: style.style,
                settings: style.settings,
            },
            stacks: &self.stacks,
        };
        let mut inserted = false;
        let (_, source_index) = self.sources.get_or_insert(query, |query| {
            inserted = true;
            let spans = if query.spans.len() == 1 && query.resolve(&query.spans[0]) == query.style {
                Box::default()
            } else {
                query
                    .spans
                    .iter()
                    .map(|span| CachedSpan {
                        range: span.range.clone(),
                        style: CachedStyle::new(query.resolve(span)),
                    })
                    .collect()
            };
            (
                SourceKey {
                    text: query.text.into(),
                    style: CachedStyle::new(query.style),
                    spans,
                },
                CachedSource {
                    id: u64::from(self.next_source) << 32,
                    word: None,
                    character: None,
                },
            )
        });
        if inserted {
            let slot = u32::try_from(source_index + 1).expect("too many cached text sources");
            self.sources.get_mut_index(source_index).id |= u64::from(slot);
            self.next_source = self.next_source.checked_add(1).expect("too many text sources");
        }
        TextRun(self.sources.get_index(source_index).id)
    }

    fn measure(&mut self, text: TextRun, max_width: Option<f32>, options: TextOptions) -> LogicalSize {
        let Some(layout) = self.prepare_layout(text, options.wrap, max_width) else {
            return LogicalSize::default();
        };
        let mut size = LogicalSize::new(0.0f32, 0.0f32);
        for line in layout.lines().take(options.max_lines.map_or(usize::MAX, usize::from)) {
            let metrics = line.metrics();
            size.width = size.width.max(metrics.advance - metrics.hanging_advance);
            size.height = size.height.max(metrics.block_max_coord);
        }
        size
    }

    fn offset_at_position(&mut self, area: LogicalRect, request: &TextRequest, position: LogicalPoint) -> usize {
        let width = area.width.max(0.0);
        let height = area.height.max(0.0);
        let max_width = (request.options.wrap != TextWrap::None).then_some(width);
        let Some(layout) = self.prepare_layout(request.text, request.options.wrap, max_width) else {
            return 0;
        };
        let (visible, total_height) = visible_layout(layout, request.options, height);
        let align_y = align_offset_y(request.options.vertical_align, height, total_height);
        let target = LogicalPoint::new(
            position.x - area.x + request.offset.x,
            position.y - area.y + request.offset.y - align_y,
        );
        let Some(line) = layout.lines().take(visible).min_by(|left, right| {
            let distance = |line: &parley::Line<'_, usize>| {
                let metrics = line.metrics();
                if target.y < metrics.block_min_coord {
                    metrics.block_min_coord - target.y
                } else if target.y > metrics.block_max_coord {
                    target.y - metrics.block_max_coord
                } else {
                    0.0
                }
            };
            distance(left).total_cmp(&distance(right))
        }) else {
            return 0;
        };
        let metrics = line.metrics();
        let line_width = metrics.advance - metrics.hanging_advance;
        let target_x = target.x - align_offset_x(request.options.horizontal_align, width, line_width);
        let mut best: Option<(f32, usize)> = None;
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
                for (byte, caret_x) in [(range.start, start), (range.end, end)] {
                    let distance = (caret_x - target_x).abs();
                    if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                        best = Some((distance, byte));
                    }
                }
                x += advance;
            }
        }
        best.map_or(line.text_range().start, |(_, byte)| byte)
    }

    fn cursor_rect(
        &mut self,
        area: LogicalRect,
        request: &TextRequest,
        byte_offset: usize,
        width: f32,
    ) -> LogicalRect {
        let area_width = area.width.max(0.0);
        let area_height = area.height.max(0.0);
        let max_width = (request.options.wrap != TextWrap::None).then_some(area_width);
        let Some(layout) = self.prepare_layout(request.text, request.options.wrap, max_width) else {
            return LogicalRect::new(area.x - request.offset.x, area.y - request.offset.y, width, 0.0);
        };
        let (visible, total_height) = visible_layout(layout, request.options, area_height);
        let align_y = align_offset_y(request.options.vertical_align, area_height, total_height);
        let Some(line) = layout.lines().take(visible).min_by_key(|line| {
            let range = line.text_range();
            if byte_offset < range.start {
                range.start - byte_offset
            } else {
                byte_offset.saturating_sub(range.end)
            }
        }) else {
            return LogicalRect::new(area.x - request.offset.x, area.y - request.offset.y, width, 0.0);
        };
        let metrics = line.metrics();
        let line_width = metrics.advance - metrics.hanging_advance;
        let align_x = align_offset_x(request.options.horizontal_align, area_width, line_width);
        let mut best: Option<(usize, bool, f32)> = None;
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
                for (byte, caret_x, is_end) in [(range.start, start, false), (range.end, end, true)] {
                    let key = (byte.abs_diff(byte_offset), is_end);
                    if best.is_none_or(|(best_diff, best_end, _)| key < (best_diff, best_end)) {
                        best = Some((key.0, key.1, caret_x));
                    }
                }
                x += advance;
            }
        }
        LogicalRect::new(
            area.x + best.map_or(0.0, |(_, _, caret_x)| caret_x) + align_x - request.offset.x,
            area.y + metrics.block_min_coord + align_y - request.offset.y,
            width,
            metrics.block_max_coord - metrics.block_min_coord,
        )
    }

    fn resolve(&mut self, area: LogicalRect, request: &TextRequest, scale: f32, glyphs: &mut Vec<PositionedGlyph>) {
        let width = area.width.max(0.0);
        let height = area.height.max(0.0);
        let options = request.options;
        let max_width = (options.wrap != TextWrap::None || options.overflow == TextOverflow::Ellipsis).then_some(width);
        let Some(source_index) = self.prepare_source(request.text, options.wrap, max_width) else {
            return;
        };
        let cache_key = ResolvedKey {
            max_width: max_width.map(f32::to_bits),
            max_height: (options.overflow == TextOverflow::Ellipsis).then(|| height.to_bits()),
            max_lines: options.max_lines,
            overflow: options.overflow,
        };
        let break_all = options.wrap == TextWrap::Character;
        let source = self.sources.get_mut_index(source_index);
        let shaped = if break_all {
            source.character.as_mut().unwrap()
        } else {
            source.word.as_mut().unwrap()
        };
        if shaped.resolved.as_ref().is_none_or(|cached| cached.key != cache_key) {
            let layout = &shaped.layout;
            let (visible, total_height) = visible_layout(layout, options, height);
            let truncated = visible < layout.len();
            let font_faces = &mut self.font_faces;
            let mut cached_glyphs = Vec::new();
            let mut runs = Vec::new();
            let mut line_widths = Vec::with_capacity(visible);
            for (line_index, line) in layout.lines().take(visible).enumerate() {
                let line_start = cached_glyphs.len();
                let metrics = line.metrics();
                let mut last_baseline = 0.0;
                for item in line.items() {
                    let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                        continue;
                    };
                    let run = glyph_run.run();
                    let synthesis = run.synthesis();
                    let blob_id = run.font().data.id();
                    let face_index = run.font().index;
                    let embolden = synthesis.embolden();
                    let skew = synthesis.skew().unwrap_or(0.0) as i8;
                    let coords = run.normalized_coords();
                    let font_id = if let Some(index) = font_faces.iter().position(|face| {
                        face.blob_id == blob_id
                            && face.face_index == face_index
                            && face.embolden == embolden
                            && face.skew == skew
                            && face.coords.len() == coords.len()
                            && face
                                .coords
                                .iter()
                                .zip(coords)
                                .all(|(&cached, coord)| cached == coord.to_bits())
                    }) {
                        index as u64
                    } else {
                        let index = font_faces.len() as u64;
                        font_faces.push(FontFace {
                            blob_id,
                            face_index,
                            embolden,
                            skew,
                            coords: coords.iter().map(|coord| coord.to_bits()).collect(),
                            font: run.font().clone(),
                        });
                        index
                    };
                    let start = cached_glyphs.len() as u32;
                    cached_glyphs.extend(glyph_run.positioned_glyphs().map(|glyph| Glyph {
                        id: glyph.id as u16,
                        x: glyph.x,
                        y: glyph.y,
                    }));
                    runs.push(CachedRun {
                        font_id,
                        font_size: run.font_size(),
                        glyphs: start..cached_glyphs.len() as u32,
                        span: glyph_run.style().brush as u32,
                        line: line_index as u32,
                    });
                    last_baseline = glyph_run.baseline();
                }
                let mut line_width = metrics.advance - metrics.hanging_advance;
                if options.overflow == TextOverflow::Ellipsis
                    && line_index + 1 == visible
                    && (truncated || line_width > width)
                    && let Some(last_run) = runs.last().filter(|run| run.line as usize == line_index)
                {
                    let (font_id, font_size, span) = (last_run.font_id, last_run.font_size, last_run.span);
                    let face = &font_faces[font_id as usize];
                    if let Some(swash_face) = FontRef::from_index(face.font.data.as_ref(), face.font.index as usize) {
                        let ellipsis = swash_face.charmap().map('…');
                        let advance = swash_face
                            .glyph_metrics(&face.coords)
                            .scale(font_size)
                            .advance_width(ellipsis);
                        let available = width - advance;
                        let mut end = metrics.offset;
                        let mut kept = 0usize;
                        'scan: for item in line.items() {
                            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                                continue;
                            };
                            for glyph in glyph_run.positioned_glyphs() {
                                if glyph.x + glyph.advance > available {
                                    break 'scan;
                                }
                                end = glyph.x + glyph.advance;
                                kept += 1;
                            }
                        }
                        let kept = (line_start + kept) as u32;
                        cached_glyphs.truncate(kept as usize);
                        runs.retain_mut(|run| {
                            run.glyphs.end = run.glyphs.end.min(kept);
                            run.glyphs.start < run.glyphs.end
                        });
                        cached_glyphs.push(Glyph {
                            id: ellipsis,
                            x: end,
                            y: last_baseline,
                        });
                        runs.push(CachedRun {
                            font_id,
                            font_size,
                            glyphs: kept..kept + 1,
                            span,
                            line: line_index as u32,
                        });
                        line_width = end + advance;
                    }
                }
                line_widths.push(line_width);
            }
            shaped.resolved = Some(ResolvedLayout {
                key: cache_key,
                total_height,
                line_widths: line_widths.into_boxed_slice(),
                runs: runs.into_boxed_slice(),
                glyphs: cached_glyphs.into_boxed_slice(),
            });
        }
        let resolved = shaped.resolved.as_ref().unwrap();
        glyphs.reserve(resolved.glyphs.len());
        let align_y = align_offset_y(options.vertical_align, height, resolved.total_height);
        let origin_y = area.y - request.offset.y + align_y;
        let phases = self.subpixel_phases;
        for run in &resolved.runs {
            let line_width = resolved.line_widths.get(run.line as usize).copied().unwrap_or(0.0);
            let origin_x = area.x - request.offset.x + align_offset_x(options.horizontal_align, width, line_width);
            let size = (run.font_size * scale).to_bits();
            let span = run.span;
            let run_glyphs = &resolved.glyphs[run.glyphs.start as usize..run.glyphs.end as usize];
            let glyph = |item: &Glyph, px: i32, phase: u8| PositionedGlyph {
                key: GlyphKey {
                    font: run.font_id,
                    size,
                    glyph: item.id,
                    phase,
                },
                origin: PhysicalPoint::new(px, ((origin_y + item.y) * scale).round() as i32),
                span,
            };
            match phases {
                1 => glyphs.extend(
                    run_glyphs
                        .iter()
                        .map(|item| glyph(item, ((origin_x + item.x) * scale).round() as i32, 0)),
                ),
                4 => {
                    let scale4 = scale * 4.0;
                    glyphs.extend(run_glyphs.iter().map(|item| {
                        let quantized = ((origin_x + item.x) * scale4).round() as i32;
                        glyph(item, quantized.div_euclid(4), (quantized.rem_euclid(4) as u8) * 64)
                    }));
                }
                _ => {
                    let scale_phases = scale * f32::from(phases);
                    let divisor = i32::from(phases);
                    glyphs.extend(run_glyphs.iter().map(|item| {
                        let quantized = ((origin_x + item.x) * scale_phases).round() as i32;
                        let step = quantized.rem_euclid(divisor) as u16;
                        glyph(
                            item,
                            quantized.div_euclid(divisor),
                            (step * 256 / u16::from(phases)) as u8,
                        )
                    }));
                }
            }
        }
    }

    fn rasterize(&mut self, key: GlyphKey) -> GlyphImage {
        (|| {
            let face = self.font_faces.get(key.font as usize)?;
            let font = FontRef::from_index(face.font.data.as_ref(), face.font.index as usize)?;
            let mut scaler = self
                .scale
                .builder(font)
                .size(f32::from_bits(key.size))
                .normalized_coords(face.coords.iter().copied())
                .build();
            let mut image = Render::new(&[
                Source::ColorOutline(0),
                Source::ColorBitmap(StrikeWith::BestFit),
                Source::Outline,
            ])
            .format(Format::Alpha)
            .offset(Vector::new(f32::from(key.phase) / 256.0, 0.0))
            .embolden(if face.embolden { 1.0 } else { 0.0 })
            .transform(
                (face.skew != 0)
                    .then(|| Transform::skew(Angle::from_degrees(f32::from(face.skew)), Angle::from_degrees(0.0))),
            )
            .render(&mut scaler, key.glyph)?;
            let metrics = GlyphMetrics {
                width: image.placement.width.try_into().ok()?,
                height: image.placement.height.try_into().ok()?,
                left: image.placement.left.try_into().ok()?,
                top: image.placement.top.checked_neg()?.try_into().ok()?,
            };
            let format = match image.content {
                Content::Mask => GlyphFormat::Alpha8,
                Content::Color => {
                    // color bitmaps are straight alpha while color outlines are already premultiplied
                    if matches!(image.source, Source::ColorBitmap(_)) {
                        for pixel in image.data.as_chunks_mut::<4>().0 {
                            for channel in 0..3 {
                                pixel[channel] = (u16::from(pixel[channel]) * u16::from(pixel[3]) / 255) as u8;
                            }
                        }
                    }
                    GlyphFormat::Rgba8
                }
                Content::SubpixelMask => return None,
            };
            (usize::from(metrics.width) * usize::from(metrics.height) * format.bytes_per_pixel() == image.data.len())
                .then_some(GlyphImage {
                    metrics,
                    format,
                    pixels: Cow::Owned(image.data),
                })
        })()
        .unwrap_or_default()
    }

    fn finish_frame(&mut self) {
        self.sources.trim_to_weight();
    }
}

impl Backend {
    fn prepare_layout(&mut self, text: TextRun, wrap: TextWrap, max_width: Option<f32>) -> Option<&Layout<usize>> {
        let source_index = self.prepare_source(text, wrap, max_width)?;
        Some(self.sources.get_index(source_index).layout(wrap))
    }

    fn prepare_source(&mut self, text: TextRun, wrap: TextWrap, max_width: Option<f32>) -> Option<usize> {
        if text.0 == 0 {
            return None;
        }
        let source_index = (text.0 as u32).checked_sub(1).expect("invalid text") as usize;
        assert!(
            self.sources
                .try_get_index(source_index)
                .is_some_and(|source| source.id == text.0),
            "expired text source"
        );
        self.sources.touch_index(source_index);
        let break_all = wrap == TextWrap::Character;
        let missing = {
            let source = self.sources.get_index(source_index);
            if break_all {
                source.character.is_none()
            } else {
                source.word.is_none()
            }
        };
        if missing {
            let source_key = self.sources.get_key_index(source_index);
            let text = source_key.text.as_ref();
            let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, false);
            builder.push_default(StyleProperty::Brush(0));
            if break_all {
                builder.push_default(StyleProperty::WordBreak(WordBreak::BreakAll));
            }
            let base = (0..text.len(), &source_key.style);
            let spans = source_key.spans.iter().map(|span| (span.range.clone(), &span.style));
            for (index, (range, style)) in std::iter::once(base).chain(spans).enumerate() {
                let features = style
                    .features
                    .iter()
                    .map(|(tag, value)| FontFeature {
                        tag: Tag::new(tag),
                        value: (*value).try_into().unwrap_or(u16::MAX),
                    })
                    .collect();
                // optical size follows css optical sizing unless variations set opsz
                let optical_size = match style.optical_size {
                    OpticalSize::Auto => Some(style.size),
                    OpticalSize::FontDefault => None,
                    OpticalSize::Fixed(size) => Some(size),
                }
                .filter(|_| !style.variations.iter().any(|(tag, _)| tag == b"opsz"));
                let variations = style
                    .variations
                    .iter()
                    .map(|(tag, value)| (tag, *value))
                    .chain(optical_size.map(|size| (b"opsz", size)))
                    .map(|(tag, value)| FontVariation {
                        tag: Tag::new(tag),
                        value,
                    })
                    .collect();
                let language = style
                    .language
                    .as_deref()
                    .and_then(|language| Language::parse(language).ok());
                for property in [
                    StyleProperty::FontFamily(FontFamily::List(Cow::Borrowed(&self.stacks[style.stack].1))),
                    StyleProperty::FontSize(style.size),
                    StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.2)),
                    StyleProperty::FontWeight(FontWeight::new(f32::from(style.weight))),
                    StyleProperty::FontWidth(FontWidth::from_percentage(f32::from(style.stretch))),
                    StyleProperty::FontStyle(match style.style {
                        FontStyle::Normal => parley::FontStyle::Normal,
                        FontStyle::Italic => parley::FontStyle::Italic,
                        FontStyle::Oblique => parley::FontStyle::Oblique(None),
                    }),
                    StyleProperty::FontFeatures(FontFeatures::List(Cow::Owned(features))),
                    StyleProperty::FontVariations(FontVariations::List(Cow::Owned(variations))),
                    StyleProperty::Locale(language),
                ] {
                    if index == 0 {
                        builder.push_default(property);
                    } else {
                        builder.push(property, range.clone());
                    }
                }
                if index != 0 {
                    builder.push(StyleProperty::Brush(index - 1), range);
                }
            }
            let built = ShapedLayout {
                layout: builder.build(text),
                broken_width: None,
                resolved: None,
            };
            let source = self.sources.get_mut_index(source_index);
            if break_all {
                source.character = Some(built);
            } else {
                source.word = Some(built);
            }
        }
        let target_width = (wrap != TextWrap::None)
            .then_some(max_width)
            .flatten()
            .filter(|width| width.is_finite())
            .map(|width| width.max(0.0).to_bits());
        let source = self.sources.get_mut_index(source_index);
        let shaped = if break_all {
            source.character.as_mut().unwrap()
        } else {
            source.word.as_mut().unwrap()
        };
        if shaped.broken_width != Some(target_width) {
            shaped.layout.break_all_lines(target_width.map(f32::from_bits));
            shaped.layout.align(Alignment::Start, AlignmentOptions::default());
            shaped.broken_width = Some(target_width);
            shaped.resolved = None;
        }
        Some(source_index)
    }
}

struct FontFace {
    blob_id: u64,
    face_index: u32,
    embolden: bool,
    skew: i8,
    coords: Box<[i16]>,
    font: parley::FontData,
}

struct SourceKey {
    text: Box<str>,
    style: CachedStyle,
    // styled sections of rich text with base style defaults already applied
    // empty means plain text using only the base style
    spans: Box<[CachedSpan]>,
}

struct CachedSource {
    id: u64,
    word: Option<ShapedLayout>,
    /// built on first character wrapped layout since break opportunities are fixed at build
    character: Option<ShapedLayout>,
}

impl CachedSource {
    fn layout(&self, wrap: TextWrap) -> &Layout<usize> {
        match wrap {
            TextWrap::Character => &self.character.as_ref().unwrap().layout,
            TextWrap::None | TextWrap::Word => &self.word.as_ref().unwrap().layout,
        }
    }
}

struct ShapedLayout {
    layout: Layout<usize>,
    broken_width: Option<Option<u32>>,
    resolved: Option<ResolvedLayout>,
}

#[derive(PartialEq, Eq)]
struct ResolvedKey {
    max_width: Option<u32>,
    max_height: Option<u32>,
    max_lines: Option<u16>,
    overflow: TextOverflow,
}

struct ResolvedLayout {
    key: ResolvedKey,
    total_height: f32,
    line_widths: Box<[f32]>,
    runs: Box<[CachedRun]>,
    glyphs: Box<[Glyph]>,
}

struct CachedRun {
    font_id: u64,
    font_size: f32,
    glyphs: Range<u32>,
    span: u32,
    line: u32,
}

#[derive(Clone, Copy)]
struct Glyph {
    id: u16,
    x: f32,
    y: f32,
}

struct CachedSpan {
    range: Range<usize>,
    style: CachedStyle,
}

#[derive(Clone, Copy, PartialEq)]
struct ResolvedStyle<'a> {
    stack: usize,
    size: f32,
    weight: u16,
    stretch: u16,
    style: FontStyle,
    settings: FontSettings<'a>,
}

struct CachedStyle {
    stack: usize,
    size: f32,
    weight: u16,
    stretch: u16,
    style: FontStyle,
    features: Box<[([u8; 4], u32)]>,
    variations: Box<[([u8; 4], f32)]>,
    language: Option<Box<str>>,
    optical_size: OpticalSize,
}

impl CachedStyle {
    fn new(style: ResolvedStyle<'_>) -> Self {
        Self {
            stack: style.stack,
            size: style.size,
            weight: style.weight,
            stretch: style.stretch,
            style: style.style,
            features: style.settings.features.into(),
            variations: style.settings.variations.into(),
            language: style.settings.language.map(Into::into),
            optical_size: style.settings.optical_size,
        }
    }

    fn as_style(&self) -> ResolvedStyle<'_> {
        ResolvedStyle {
            stack: self.stack,
            size: self.size,
            weight: self.weight,
            stretch: self.stretch,
            style: self.style,
            settings: FontSettings {
                features: &self.features,
                variations: &self.variations,
                language: self.language.as_deref(),
                optical_size: self.optical_size,
            },
        }
    }
}

struct SourceQuery<'a> {
    text: &'a str,
    spans: &'a [Span<'a>],
    style: ResolvedStyle<'a>,
    stacks: &'a [(FontId, Box<[FontFamilyName<'static>]>)],
}

impl<'a> SourceQuery<'a> {
    fn resolve(&self, span: &Span<'a>) -> ResolvedStyle<'a> {
        ResolvedStyle {
            stack: span.style.font.map_or(self.style.stack, |id| {
                self.stacks.iter().position(|(stack_id, _)| *stack_id == id).unwrap()
            }),
            size: span.style.size.unwrap_or(self.style.size),
            weight: span.style.weight.unwrap_or(self.style.weight),
            stretch: span.style.stretch.unwrap_or(self.style.stretch),
            style: span.style.style.unwrap_or(self.style.style),
            settings: span.style.settings.unwrap_or(self.style.settings),
        }
    }
}

impl Hash for SourceQuery<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_style(self.style, state);
        for span in self.spans {
            self.text[span.range.clone()].hash(state);
            hash_style(self.resolve(span), state);
        }
    }
}

impl Equivalent<SourceKey> for SourceQuery<'_> {
    fn equivalent(&self, key: &SourceKey) -> bool {
        if key.text.as_ref() != self.text || key.style.as_style() != self.style {
            return false;
        }
        if key.spans.is_empty() {
            self.spans.len() == 1 && self.resolve(&self.spans[0]) == self.style
        } else {
            key.spans.len() == self.spans.len()
                && key.spans.iter().zip(self.spans).all(|(cached_span, span)| {
                    cached_span.range == span.range && cached_span.style.as_style() == self.resolve(span)
                })
        }
    }
}

impl Hash for SourceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_style(self.style.as_style(), state);
        if self.spans.is_empty() {
            self.text.hash(state);
            hash_style(self.style.as_style(), state);
        } else {
            for span in &self.spans {
                self.text[span.range.clone()].hash(state);
                hash_style(span.style.as_style(), state);
            }
        }
    }
}

struct UnitWeight;

impl Scale<SourceKey, CachedSource> for UnitWeight {
    fn weight(&self, _key: &SourceKey, _value: &CachedSource) -> usize {
        1
    }
}

fn visible_layout(layout: &Layout<usize>, options: TextOptions, max_height: f32) -> (usize, f32) {
    let limit = options.max_lines.map_or(usize::MAX, usize::from);
    let clip_height = (options.overflow == TextOverflow::Ellipsis).then_some(max_height);
    let mut visible = 0;
    let mut height = 0.0f32;
    for line in layout.lines().take(limit) {
        let bottom = line.metrics().block_max_coord;
        if clip_height.is_some_and(|max| bottom > max) {
            break;
        }
        visible += 1;
        height = height.max(bottom);
    }
    (visible, height)
}

fn align_offset_x(align: HorizontalAlign, area_width: f32, line_width: f32) -> f32 {
    match align {
        HorizontalAlign::Left => 0.0,
        HorizontalAlign::Center => (area_width - line_width) * 0.5,
        HorizontalAlign::Right => area_width - line_width,
    }
}

fn align_offset_y(align: VerticalAlign, area_height: f32, content_height: f32) -> f32 {
    match align {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Center => (area_height - content_height) * 0.5,
        VerticalAlign::Bottom => area_height - content_height,
    }
}

fn hash_style<H: Hasher>(style: ResolvedStyle<'_>, state: &mut H) {
    let float_bits = |value: f32| if value == 0.0 { 0 } else { value.to_bits() };
    style.stack.hash(state);
    float_bits(style.size).hash(state);
    style.weight.hash(state);
    style.stretch.hash(state);
    style.style.hash(state);
    style.settings.features.hash(state);
    for (tag, value) in style.settings.variations {
        tag.hash(state);
        float_bits(*value).hash(state);
    }
    style.settings.language.hash(state);
    match style.settings.optical_size {
        OpticalSize::Auto => 0u8.hash(state),
        OpticalSize::FontDefault => 1u8.hash(state),
        OpticalSize::Fixed(size) => {
            2u8.hash(state);
            float_bits(size).hash(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blit_gui::text::SpanStyle;
    use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};

    use super::*;

    const FONT: &[u8] = include_bytes!(env!("BLIT_TEST_FONT"));

    fn setup() -> Backend {
        let mut fonts = FontContext {
            collection: Collection::new(CollectionOptions {
                shared: false,
                system_fonts: false,
            }),
            source_cache: SourceCache::default(),
        };
        let registered = fonts.collection.register_fonts(Blob::new(Arc::new(FONT)), None);
        let name = fonts.collection.family_name(registered[0].0).unwrap().to_owned();
        Backend::new(fonts).font(FontId::default(), [FontFamilyName::Named(name.into())])
    }

    #[test]
    fn line_height_and_baseline_follow_the_text_style() {
        let mut backend = setup();
        for size in [8.0, 14.0, 24.0] {
            let run = backend.text_run("text", TextStyle::new().size(size));
            let area = LogicalRect::new(0.0, 0.0, 200.0, 200.0);
            let request = TextRequest::new(run);
            let measured = backend.measure(run, None, TextOptions::new());
            let mut glyphs = Vec::new();
            backend.resolve(area, &request, 1.0, &mut glyphs);
            let metrics = FontRef::from_index(FONT, 0).unwrap().metrics(&[]).scale(size);
            let height = size * 1.2;
            let baseline = (height - metrics.ascent - metrics.descent) / 2.0 + metrics.ascent;
            assert!((measured.height - height).abs() < 0.001, "{size}");
            assert_eq!(glyphs[0].origin.y, baseline.round() as i32, "{size}");
        }
    }

    #[test]
    fn empty_and_paragraph_text_keep_valid_line_offsets() {
        let mut backend = setup();
        for (text, starts) in [
            ("", &[0][..]),
            ("a\nb", &[0, 2][..]),
            ("a\r\nb", &[0, 3][..]),
            ("a\u{2029}b", &[0, 4][..]),
        ] {
            let run = backend.text_run(text, TextStyle::new());
            let layout = backend.prepare_layout(run, TextWrap::None, None).unwrap();
            assert_eq!(layout.len(), starts.len(), "{text:?}");
            for (index, &start) in starts.iter().enumerate() {
                assert_eq!(layout.get(index).unwrap().text_range().start, start, "{text:?}");
            }
        }
    }

    #[test]
    fn spans_wrap_and_ellipsis() {
        let mut backend = setup();
        let text = "regular bold";
        let run = backend.rich_text(
            text,
            &[
                Span::new(0..8),
                Span::new(8..text.len()).style(SpanStyle::new().size(24.0).weight(700)),
            ],
            TextStyle::new(),
        );
        let area = LogicalRect::new(0.0, 0.0, 40.0, 200.0);
        for wrap in [TextWrap::Word, TextWrap::Character] {
            let layout = backend.prepare_layout(run, wrap, Some(40.0)).unwrap();
            assert!(layout.len() > 1, "{wrap:?}");
        }

        let ellipsis_request = TextRequest::new(run).options(TextOptions::new().overflow(TextOverflow::Ellipsis));
        let mut glyphs = Vec::new();
        backend.resolve(area, &ellipsis_request, 1.0, &mut glyphs);
        let ellipsis = FontRef::from_index(FONT, 0).unwrap().charmap().map('…');
        assert_eq!(glyphs.last().unwrap().key.glyph, ellipsis);
        assert!(backend.rasterize(glyphs[0].key).metrics.width > 0);
    }

    #[test]
    fn measurement_and_carets_follow_layout() {
        let mut backend = setup();
        let text = backend.text_run("hello world", TextStyle::default());
        let unwrapped = backend.measure(text, None, TextOptions::new());
        let spans = [
            Span::new(0..6).style(SpanStyle::new().size(16.0)),
            Span::new(6..11).style(SpanStyle::new().size(32.0)),
        ];
        let mixed = backend.rich_text("hello world", &spans, TextStyle::default());
        let mixed_size = backend.measure(mixed, None, TextOptions::new());
        assert!(mixed_size.height > unwrapped.height);
        let larger_base = backend.rich_text("hello world", &spans, TextStyle::new().size(48.0));
        assert_ne!(mixed, larger_base);
        let larger_size = backend.measure(larger_base, None, TextOptions::new());
        assert!((larger_size.height - 48.0 * 1.2).abs() < 0.001);

        let wrapped_options = TextOptions::new().wrap(TextWrap::Word);
        let wrapped = backend.measure(text, Some(unwrapped.width / 2.0), wrapped_options);
        assert!(wrapped.width < unwrapped.width);
        assert!(wrapped.height > unwrapped.height);
        assert_eq!(
            backend
                .measure(text, Some(unwrapped.width / 2.0), wrapped_options.max_lines(1))
                .height,
            unwrapped.height
        );

        let mut paint_area = LogicalRect::new(10.0, 20.0, 120.0, 48.0);
        let mut paint = TextRequest::new(text);
        let start = backend.cursor_rect(paint_area, &paint, 0, 0.5);
        assert_eq!(start.width, 0.5);
        assert_eq!(
            backend.offset_at_position(paint_area, &paint, LogicalPoint::new(1000.0, 20.0)),
            "hello world".len()
        );
        paint.offset = LogicalPoint::new(4.0, 2.0);
        let scrolled = backend.cursor_rect(paint_area, &paint, 0, 0.5);
        assert_eq!(scrolled.x, start.x - 4.0);
        assert_eq!(scrolled.y, start.y - 2.0);
        paint_area.width += 0.5;
        paint_area.height += 0.25;
        paint.options.horizontal_align = HorizontalAlign::Center;
        paint.options.vertical_align = VerticalAlign::Bottom;
        let placed = backend.cursor_rect(paint_area, &paint, 0, 0.5);
        assert!((placed.x - (start.x - 4.0 + (paint_area.width - unwrapped.width) / 2.0)).abs() < 0.001);
        assert!((placed.y - (start.y - 2.0 + paint_area.height - unwrapped.height)).abs() < 0.001);
        let end = backend.cursor_rect(paint_area, &paint, "hello world".len(), 0.5);
        assert_eq!(
            backend.offset_at_position(paint_area, &paint, LogicalPoint::new(end.x, end.y)),
            "hello world".len()
        );
    }

    #[test]
    fn text_runs_are_keyed_by_content_and_style() {
        use blit_gui::color::Color;

        let mut backend = setup();
        let style = TextStyle::default();
        let text = backend.text_run("same", style);
        let white = backend.text_run("same", style.color(Color::WHITE));
        assert_eq!(
            backend.rich_text(
                "same",
                &[Span::new(0..4).style(SpanStyle::new().color(Color::WHITE))],
                style.color(Color::TRANSPARENT),
            ),
            white
        );
        assert_eq!(white, text);
        assert_ne!(backend.text_run("changed", style), text);
        assert_ne!(backend.text_run("same", style.weight(500)), text);
        for settings in [
            FontSettings::new().features(&[(*b"liga", 0)]),
            FontSettings::new().variations(&[(*b"wght", 550.0)]),
            FontSettings::new().language("tr"),
            FontSettings::new().optical_size(OpticalSize::FontDefault),
        ] {
            let configured = backend.text_run("same", style.settings(settings));
            assert_ne!(configured, text);
            assert_eq!(
                backend.rich_text(
                    "same",
                    &[Span::new(0..4).style(SpanStyle::new().settings(settings))],
                    style.settings(settings)
                ),
                configured
            );
        }
        let owned = {
            let language = String::from("fr");
            backend.text_run("same", style.settings(FontSettings::new().language(language.as_str())))
        };
        assert_eq!(
            backend.text_run("same", style.settings(FontSettings::new().language("fr"))),
            owned
        );
    }
}
