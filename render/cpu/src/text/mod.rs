mod font;

use std::{ptr::NonNull, sync::Arc};

use crate::{
    Pixel, PixelSpan, RendererConfig,
    color::Color,
    text_types::{TextLayoutRequest, TextRequest, TextRunId, TextStyle},
};
use blit::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect, Scale2};
use blit_text::{
    FontError, FontId, HorizontalAlign, TextId, TextOverflow, TextSystem, VerticalAlign,
};
use font::GlyphCache;

pub struct TextRenderer {
    backend: TextSystem,
    families: Box<[ConfiguredFace]>,
    glyphs: GlyphCache,
    prepared: Vec<PreparedGlyph>,
    lines: Vec<PreparedLine>,
    coverage: Vec<u8>,
}

#[derive(Clone, Copy)]
struct ConfiguredFace {
    family: crate::text_types::FontId,
    weight: u16,
    font: FontId,
}

pub struct PreparedGlyph {
    alpha: NonNull<u8>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

struct PreparedLine {
    glyph_start: u32,
    glyph_end: u32,
    top: i32,
    bottom: i32,
}

#[derive(Clone, Copy)]
pub struct PreparedLines {
    start: u32,
    end: u32,
}

impl PreparedLines {
    const NONE: Self = Self {
        start: u32::MAX,
        end: u32::MAX,
    };
}

impl TextRenderer {
    pub fn new(config: RendererConfig, mut backend: TextSystem) -> Result<Self, FontError> {
        let mut families = Vec::with_capacity(config.fonts.len());
        for face in config.fonts {
            let font = backend
                .register_font(blit_text::FontData::Shared(Arc::from(face.font.bytes())), 0)?;
            families.push(ConfiguredFace {
                family: face.id,
                weight: face.weight,
                font,
            });
        }
        Ok(Self {
            backend,
            families: families.into_boxed_slice(),
            glyphs: GlyphCache::new(config.glyph_cache_capacity),
            prepared: Vec::new(),
            lines: Vec::new(),
            coverage: Vec::new(),
        })
    }

    pub fn text_run(&mut self, text: &str, style: TextStyle, _scale_factor: f32) -> TextRunId {
        let Some(face) = self
            .families
            .iter()
            .filter(|face| face.family == style.font)
            .min_by_key(|face| {
                (
                    face.weight.abs_diff(style.weight),
                    std::cmp::Reverse(face.weight),
                )
            })
        else {
            return TextRunId::default();
        };
        let text = self.backend.text(
            text,
            blit_text::TextStyle {
                font: face.font,
                size: style.size,
                weight: style.weight,
            },
        );
        TextRunId(text.0)
    }

    pub fn prepare(
        &mut self,
        request: &TextRequest,
        scale_factor: f32,
    ) -> (u32, u32, PreparedLines, PhysicalRect) {
        let area = request.area.to_physical(Scale2::uniform(scale_factor));
        let layout = self.backend.layout(Self::paint_request(request));
        let glyph_start = u32::try_from(self.prepared.len()).expect("too many prepared glyphs");
        let line_start = u32::try_from(self.lines.len()).expect("too many prepared lines");
        let mut bounds = PhysicalRect::default();
        let mut has_bounds = false;
        let backend = &self.backend;
        let glyphs = &mut self.glyphs;
        let prepared = &mut self.prepared;
        let lines = &mut self.lines;
        backend.visit_runs(layout, |run| {
            let start = u32::try_from(prepared.len()).expect("too many prepared glyphs");
            for glyph in run.glyphs {
                let cached = glyphs.glyph(backend, run.font, glyph.id, run.size * scale_factor);
                let cached = glyphs.get(cached);
                let x = (glyph.position.x * scale_factor + cached.metrics.bounds.xmin.floor())
                    .round() as i32;
                let y = (glyph.position.y * scale_factor
                    + (-cached.metrics.bounds.height - cached.metrics.bounds.ymin).floor())
                .round() as i32;
                let width = u32::try_from(cached.metrics.width).expect("glyph is too wide");
                let height = u32::try_from(cached.metrics.height).expect("glyph is too tall");
                prepared.push(PreparedGlyph {
                    alpha: NonNull::new(cached.alpha.as_ptr().cast_mut()).unwrap(),
                    x,
                    y,
                    width,
                    height,
                });
                let glyph_bounds = PhysicalRect {
                    x: area.x.saturating_add(x),
                    y: area.y.saturating_add(y),
                    width: width as i32,
                    height: height as i32,
                };
                bounds = if has_bounds {
                    bounds.union(glyph_bounds)
                } else {
                    has_bounds = true;
                    glyph_bounds
                };
            }
            let end = u32::try_from(prepared.len()).expect("too many prepared glyphs");
            if start != end {
                lines.push(PreparedLine {
                    glyph_start: start,
                    glyph_end: end,
                    top: (run.bounds.y * scale_factor).floor() as i32,
                    bottom: ((run.bounds.y + run.bounds.height) * scale_factor).ceil() as i32,
                });
            }
        });
        let glyph_end = u32::try_from(self.prepared.len()).expect("too many prepared glyphs");
        let line_end = u32::try_from(self.lines.len()).expect("too many prepared lines");
        let lines = if line_end.saturating_sub(line_start) > 1 {
            PreparedLines {
                start: line_start,
                end: line_end,
            }
        } else {
            PreparedLines::NONE
        };
        (glyph_start, glyph_end, lines, bounds)
    }

    pub fn draw_line<P: Pixel>(
        &mut self,
        glyph_start: u32,
        glyph_end: u32,
        lines: PreparedLines,
        area: PhysicalRect,
        color: Color,
        line: i32,
        row: PixelSpan<'_, P>,
        clip: PhysicalRect,
    ) {
        if line < clip.y || line >= clip.y.saturating_add(clip.height) {
            return;
        }
        let (glyph_start, glyph_end) = if lines.start == PreparedLines::NONE.start {
            (glyph_start, glyph_end)
        } else {
            let mut start = glyph_end;
            let mut end = glyph_start;
            for prepared_line in &self.lines[lines.start as usize..lines.end as usize] {
                if line >= area.y.saturating_add(prepared_line.top)
                    && line < area.y.saturating_add(prepared_line.bottom)
                {
                    start = start.min(prepared_line.glyph_start);
                    end = end.max(prepared_line.glyph_end);
                }
            }
            if start >= end {
                return;
            }
            (start, end)
        };
        let row_end = row.x.saturating_add(row.pixels.len() as i32);
        if self.coverage.len() < row.pixels.len() {
            self.coverage.resize(row.pixels.len(), 0);
        }
        let coverage = &mut self.coverage[..row.pixels.len()];
        let clear_start = (clip.x - row.x).max(0).min(row.pixels.len() as i32) as usize;
        let clear_end = (clip.x.saturating_add(clip.width) - row.x)
            .max(0)
            .min(row.pixels.len() as i32) as usize;
        coverage[clear_start..clear_end].fill(0);
        let mut touched_start = row.pixels.len();
        let mut touched_end = 0usize;
        for glyph in &self.prepared[glyph_start as usize..glyph_end as usize] {
            let x = area.x.saturating_add(glyph.x);
            let y = area.y.saturating_add(glyph.y);
            if line < y || line >= y.saturating_add(glyph.height as i32) {
                continue;
            }
            let left = x.max(row.x).max(clip.x);
            let right = x
                .saturating_add(glyph.width as i32)
                .min(row_end)
                .min(clip.x.saturating_add(clip.width));
            if left >= right {
                continue;
            }
            let source_x = (left - x) as usize;
            let source_y = (line - y) as usize;
            let len = (right - left) as usize;
            let source = source_y * glyph.width as usize + source_x;
            // safety: glyph alpha allocations remain live until finish_frame
            let alpha =
                unsafe { std::slice::from_raw_parts(glyph.alpha.as_ptr().add(source), len) };
            let destination_start = (left - row.x) as usize;
            let destination_end = destination_start + len;
            let overlap = touched_end.saturating_sub(destination_start).min(len);
            let destination = &mut coverage[destination_start..destination_end];
            for (destination, source) in destination[..overlap].iter_mut().zip(alpha) {
                *destination =
                    (*source as u16 + *destination as u16 * (255 - *source as u16) / 255) as u8;
            }
            destination[overlap..].copy_from_slice(&alpha[overlap..]);
            touched_start = touched_start.min(destination_start);
            touched_end = touched_end.max(destination_end);
        }
        if touched_start < touched_end {
            P::blend_alpha_slice(
                &mut row.pixels[touched_start..touched_end],
                color,
                &coverage[touched_start..touched_end],
            );
        }
    }

    pub fn finish_frame(&mut self) {
        self.prepared.clear();
        self.lines.clear();
        self.backend.finish_frame();
        self.glyphs.finish_frame();
    }

    pub fn offset_at_position(
        &mut self,
        request: &TextRequest,
        position: LogicalPoint,
        _scale_factor: f32,
    ) -> usize {
        let layout = self.backend.layout(Self::paint_request(request));
        self.backend.hit_test(
            layout,
            LogicalPoint {
                x: position.x - request.area.x,
                y: position.y - request.area.y,
            },
        )
    }

    pub fn measure(&mut self, request: &TextLayoutRequest, _scale_factor: f32) -> LogicalSize {
        let layout = self.backend.layout(blit_text::TextLayoutRequest {
            text: TextId(request.text.0),
            max_width: request.max_width,
            max_height: None,
            offset_x: 0.0,
            max_lines: request.max_lines,
            wrap: match request.wrap {
                crate::text_types::TextWrap::None => blit_text::TextWrap::None,
                crate::text_types::TextWrap::Word => blit_text::TextWrap::Word,
                crate::text_types::TextWrap::Character => blit_text::TextWrap::Glyph,
            },
            overflow: TextOverflow::Clip,
            horizontal_align: HorizontalAlign::Start,
            vertical_align: VerticalAlign::Start,
        });
        self.backend.size(layout)
    }

    pub fn cursor_rect(
        &mut self,
        request: &TextRequest,
        byte_offset: usize,
        scale_factor: f32,
    ) -> LogicalRect {
        let layout = self.backend.layout(Self::paint_request(request));
        let rect = self.backend.cursor_rect(layout, byte_offset);
        LogicalRect {
            x: request.area.x + rect.x,
            y: request.area.y + rect.y,
            width: scale_factor.recip(),
            height: rect.height,
        }
    }

    fn paint_request(request: &TextRequest) -> blit_text::TextLayoutRequest {
        blit_text::TextLayoutRequest {
            text: TextId(request.text.0),
            max_width: Some(request.area.width.max(0.0)),
            max_height: Some(request.area.height.max(0.0)),
            offset_x: request.offset_x,
            max_lines: request.options.max_lines,
            wrap: match request.options.wrap {
                crate::text_types::TextWrap::None => blit_text::TextWrap::None,
                crate::text_types::TextWrap::Word => blit_text::TextWrap::Word,
                crate::text_types::TextWrap::Character => blit_text::TextWrap::Glyph,
            },
            overflow: match request.options.overflow {
                crate::text_types::TextOverflow::Clip => TextOverflow::Clip,
                crate::text_types::TextOverflow::Ellipsis => TextOverflow::Ellipsis,
            },
            horizontal_align: match request.options.horizontal_align {
                crate::text_types::HorizontalAlign::Left => HorizontalAlign::Start,
                crate::text_types::HorizontalAlign::Center => HorizontalAlign::Center,
                crate::text_types::HorizontalAlign::Right => HorizontalAlign::End,
            },
            vertical_align: match request.options.vertical_align {
                crate::text_types::VerticalAlign::Top => VerticalAlign::Start,
                crate::text_types::VerticalAlign::Center => VerticalAlign::Center,
                crate::text_types::VerticalAlign::Bottom => VerticalAlign::End,
            },
        }
    }
}
