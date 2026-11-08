use std::ptr::NonNull;

use blit::{PhysicalRect, Point, Scale2};
use blit_gui::{TextSystem, color::Color, text::TextRequest};
use blit_text::GlyphRequest;

use crate::{Pixel, PixelSpan, RendererConfig, glyph::GlyphCache, strategy::command::PreparedText};

pub struct TextRenderer {
    pub system: TextSystem,
    glyphs: GlyphCache,
    prepared: Vec<PreparedGlyph>,
    runs: Vec<PreparedRun>,
    coverage: Vec<u8>,
}

pub struct PreparedGlyph {
    pixels: NonNull<u8>,
    color: bool,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

struct PreparedRun {
    glyph_start: u32,
    glyph_end: u32,
    top: i32,
    bottom: i32,
    color: Color,
}

#[derive(Clone, Copy)]
pub struct PreparedRuns {
    start: u32,
    end: u32,
}

impl PreparedRuns {
    const NONE: Self = Self {
        start: u32::MAX,
        end: u32::MAX,
    };
}

impl TextRenderer {
    pub fn new(config: &RendererConfig, system: TextSystem) -> Self {
        Self {
            system,
            glyphs: GlyphCache::new(config.glyph_cache_capacity),
            prepared: Vec::new(),
            runs: Vec::new(),
            coverage: Vec::new(),
        }
    }

    pub fn prepare(&mut self, request: &TextRequest<'_>, scale_factor: f32) -> (PreparedText, PhysicalRect) {
        let area = request.area.to_physical(Scale2::uniform(scale_factor));
        let mut resolved = self.system.paint_layout(request);
        let layout = resolved.layout;
        let glyph_start = self.prepared.len() as u32;
        let run_start = self.runs.len() as u32;
        let mut bounds = None::<PhysicalRect>;
        for run in &layout.runs {
            let color = request.span_color(run.span);
            if color.alpha == 0 {
                continue;
            }
            let start = self.prepared.len() as u32;
            let offset = resolved.line_offset(run.line as usize);
            let mut top = i32::MAX;
            let mut bottom = i32::MIN;
            for glyph in &layout.glyphs[run.glyphs.start as usize..run.glyphs.end as usize] {
                let prepared = resolved.prepare_glyph(GlyphRequest {
                    font: run.font,
                    glyph: glyph.id,
                    scale: scale_factor,
                    origin: Point::new(
                        (request.area.x + glyph.position.x + offset.x - request.offset_x) * scale_factor,
                        (request.area.y + glyph.position.y + offset.y) * scale_factor,
                    ),
                });
                let Some(index) = self.glyphs.glyph(&mut resolved, prepared.key) else {
                    continue;
                };
                let cached = self.glyphs.get(index);
                let shape = PhysicalRect::new(
                    prepared.origin.x.saturating_add(i32::from(cached.metrics.left)),
                    prepared.origin.y.saturating_add(i32::from(cached.metrics.top)),
                    i32::from(cached.metrics.width),
                    i32::from(cached.metrics.height),
                );
                let Some(visible) = shape.intersection(area) else {
                    continue;
                };
                bounds = Some(bounds.map_or(visible, |bounds| bounds.union(visible)));
                let y = shape.y.saturating_sub(area.y);
                top = top.min(y);
                bottom = bottom.max(y.saturating_add(shape.height));
                self.prepared.push(PreparedGlyph {
                    pixels: NonNull::new(cached.pixels.data().as_ptr().cast_mut()).unwrap(),
                    color: cached.pixels.is_color(),
                    x: shape.x.saturating_sub(area.x),
                    y,
                    width: cached.metrics.width.into(),
                    height: cached.metrics.height.into(),
                });
            }
            if start != self.prepared.len() as u32 {
                self.runs.push(PreparedRun {
                    glyph_start: start,
                    glyph_end: self.prepared.len() as u32,
                    top,
                    bottom,
                    color,
                });
            }
        }
        let color = self
            .runs
            .get(run_start as usize)
            .map_or(Color::TRANSPARENT, |run| run.color);
        let runs = if self.runs.len() - run_start as usize > 1 {
            PreparedRuns {
                start: run_start,
                end: self.runs.len() as u32,
            }
        } else {
            self.runs.truncate(run_start as usize);
            PreparedRuns::NONE
        };
        (
            PreparedText {
                glyph_start,
                glyph_end: self.prepared.len() as u32,
                runs,
                area,
                color,
            },
            bounds.unwrap_or_default(),
        )
    }

    pub fn draw_line<P: Pixel>(
        &mut self,
        command: &PreparedText,
        clip_coverage: u8,
        line: i32,
        row: PixelSpan<'_, P>,
        clip: PhysicalRect,
    ) {
        let PreparedText {
            glyph_start,
            glyph_end,
            runs,
            area,
            color,
        } = *command;
        if line < clip.y || line >= clip.y.saturating_add(clip.height) {
            return;
        }
        let row_end = row.x.saturating_add(row.pixels.len() as i32);
        if self.coverage.len() < row.pixels.len() {
            self.coverage.resize(row.pixels.len(), 0);
        }
        let coverage = &mut self.coverage[..row.pixels.len()];
        let clear_start = (clip.x - row.x).max(0).min(row.pixels.len() as i32) as usize;
        let clear_end = (clip.x.saturating_add(clip.width) - row.x)
            .max(0)
            .min(row.pixels.len() as i32) as usize;
        let mut draw = |glyph_start: u32, glyph_end: u32, mut color: Color| {
            color.alpha = (color.alpha as u16 * clip_coverage as u16 / 255) as u8;
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
                if glyph.color {
                    if touched_start < touched_end {
                        P::blend_alpha_slice(
                            &mut row.pixels[touched_start..touched_end],
                            color,
                            &coverage[touched_start..touched_end],
                        );
                        coverage[touched_start..touched_end].fill(0);
                    }
                    touched_start = row.pixels.len();
                    touched_end = 0;
                    // safety: rgba pixels have byte alignment and remain live until finish_frame
                    let pixels = unsafe {
                        std::slice::from_raw_parts(
                            glyph
                                .pixels
                                .as_ptr()
                                .add(source * 4)
                                .cast::<crate::pixel::PremultipliedRgbaColor>(),
                            len,
                        )
                    };
                    P::blend_texture_slice_rgba(
                        &mut row.pixels[(left - row.x) as usize..(right - row.x) as usize],
                        pixels,
                        color.alpha,
                    );
                    continue;
                }
                // safety: glyph pixels remain live until finish_frame
                let alpha = unsafe { std::slice::from_raw_parts(glyph.pixels.as_ptr().add(source), len) };
                let destination_start = (left - row.x) as usize;
                let destination_end = destination_start + len;
                let overlap = touched_end.saturating_sub(destination_start).min(len);
                let destination = &mut coverage[destination_start..destination_end];
                for (destination, source) in destination[..overlap].iter_mut().zip(alpha) {
                    *destination = (*source as u16 + *destination as u16 * (255 - *source as u16) / 255) as u8;
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
        };
        if runs.start == PreparedRuns::NONE.start {
            draw(glyph_start, glyph_end, color);
        } else {
            for run in &self.runs[runs.start as usize..runs.end as usize] {
                if line >= area.y.saturating_add(run.top) && line < area.y.saturating_add(run.bottom) {
                    draw(run.glyph_start, run.glyph_end, run.color);
                }
            }
        }
    }

    pub fn finish_frame(&mut self) {
        self.prepared.clear();
        self.runs.clear();
        self.glyphs.finish_frame(&mut self.system);
        self.system.finish_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Xrgb8888;

    #[test]
    fn mixed_glyphs_preserve_color_opacity_and_painter_order() {
        let system = TextSystem::new(Default::default(), blit_text_parley::Backend::without_system_fonts()).unwrap();
        let mut renderer = TextRenderer::new(&RendererConfig::default(), system);
        let mut alpha = [255u8];
        let mut rgba = [128u8, 0, 0, 128];
        renderer.prepared = [false, true, false]
            .map(|color| PreparedGlyph {
                pixels: NonNull::new(if color { rgba.as_mut_ptr() } else { alpha.as_mut_ptr() }).unwrap(),
                color,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            })
            .into();
        let area = PhysicalRect::new(0, 0, 1, 1);
        let mut pixels = [Xrgb8888::default()];
        renderer.draw_line(
            &PreparedText {
                glyph_start: 0,
                glyph_end: 3,
                runs: PreparedRuns::NONE,
                area,
                color: Color::rgba(0, 255, 0, 128),
            },
            128,
            0,
            PixelSpan {
                x: 0,
                pixels: &mut pixels,
            },
            area,
        );
        assert_eq!(pixels[0], Xrgb8888::from_rgb(23, 105, 0));
    }
}
