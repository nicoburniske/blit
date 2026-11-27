use alloc::{boxed::Box, vec::Vec};
use core::{mem::size_of, ops::Range, ptr::NonNull};

use blit::{LogicalRect, Paint, PhysicalRect, Scale2};
use blit_gui::{
    color::Color,
    damage::DamageTracker,
    display_list::DisplayList,
    text::{FrameText, GlyphFormat, GlyphImage, GlyphKey, PositionedGlyph, TextSystem},
};
use blit_util::cache::{DeferredCache, Scale};

use crate::{
    Pixel, PixelBuffer, PixelSpan, RenderStrategy, Renderer, RendererConfig, render::DrawContext,
    strategy::command::Payload,
};

impl<B, S, D> Paint<FrameText> for Renderer<B, S, D>
where
    B: PixelBuffer,
    S: RenderStrategy<B>,
    D: DamageTracker,
{
    fn paint(&mut self, area: LogicalRect, value: &FrameText) {
        self.record(area).push(*value, draw_text::<B::Pixel>);
    }
}

fn draw_text<P: Pixel>(_: &FrameText, cx: &mut DrawContext<'_, P>) {
    let scale = cx.scale;
    let target = &mut *cx.commands;
    let area = cx.area.to_physical(Scale2::uniform(scale));
    if let Some(bounds) = area.intersection(cx.bounds) {
        target.record(bounds, cx.clip, cx.offset, |_, offset, prepared, cx| {
            // safety: recording stores this offset as FrameText
            let input = unsafe { cx.list.store().get::<FrameText>(offset) };
            let (text, bounds) = cx.text.prepare(cx.area, input, cx.list, cx.scale);
            prepared.push(Payload::Text(text, bounds))
        });
    }
}

pub struct TextRenderer {
    pub system: Box<dyn TextSystem>,
    glyphs: DeferredCache<GlyphKey, GlyphImage, GlyphScale>,
    resolved: Vec<PositionedGlyph>,
    prepared: Vec<PreparedGlyph>,
    runs: Vec<PreparedRun>,
    coverage: Vec<u8>,
}

impl TextRenderer {
    pub fn new(config: &RendererConfig, system: Box<dyn TextSystem>) -> Self {
        Self {
            system,
            glyphs: DeferredCache::new(GlyphScale, config.glyph_cache_capacity),
            resolved: Vec::new(),
            prepared: Vec::new(),
            runs: Vec::new(),
            coverage: Vec::new(),
        }
    }

    pub fn prepare(
        &mut self,
        placement: LogicalRect,
        text: &FrameText,
        list: &DisplayList,
        scale_factor: f32,
    ) -> (PreparedText, PhysicalRect) {
        let request = text.request();
        let area = placement.to_physical(Scale2::uniform(scale_factor));
        self.resolved.clear();
        self.system
            .resolve(placement, &request, scale_factor, &mut self.resolved);
        let glyph_start = self.prepared.len() as u32;
        let run_start = self.runs.len() as u32;
        let mut bounds = None::<PhysicalRect>;
        let mut current_span = None::<u32>;
        let mut current_baseline = i32::MIN;
        let mut color = Color::TRANSPARENT;
        let mut start = self.prepared.len() as u32;
        let mut top = i32::MAX;
        let mut bottom = i32::MIN;
        let mut disjoint = true;
        let mut prev_end = i32::MIN;
        for &prepared in &self.resolved {
            if current_span != Some(prepared.span) || prepared.origin.y != current_baseline {
                if start != self.prepared.len() as u32 {
                    self.runs.push(PreparedRun {
                        glyphs: start..self.prepared.len() as u32,
                        top,
                        bottom,
                        color,
                        disjoint,
                    });
                }
                if current_span != Some(prepared.span) {
                    current_span = Some(prepared.span);
                    color = text.span_color(list, prepared.span as usize);
                }
                current_baseline = prepared.origin.y;
                start = self.prepared.len() as u32;
                top = i32::MAX;
                bottom = i32::MIN;
                disjoint = true;
                prev_end = i32::MIN;
            }
            if color.alpha == 0 {
                continue;
            }
            let (cached, _) = self.glyphs.get_or_insert(prepared.key, |key| {
                let image = self.system.rasterize(key);
                let required = usize::from(image.metrics.width)
                    .saturating_mul(usize::from(image.metrics.height))
                    .saturating_mul(image.format.bytes_per_pixel());
                let image = if required <= image.pixels.len() {
                    image
                } else {
                    GlyphImage::default()
                };
                (key, image)
            });
            if cached.metrics.width == 0 || cached.metrics.height == 0 {
                continue;
            }
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
            let x = shape.x.saturating_sub(area.x);
            let y = shape.y.saturating_sub(area.y);
            top = top.min(y);
            bottom = bottom.max(y.saturating_add(shape.height));
            let color_glyph = cached.format == GlyphFormat::Rgba8;
            if !color_glyph {
                disjoint &= x >= prev_end;
                prev_end = prev_end.max(x.saturating_add(shape.width));
            }
            self.prepared.push(PreparedGlyph {
                pixels: NonNull::new(cached.pixels.as_ptr().cast_mut()).unwrap(),
                x,
                y,
                width: cached.metrics.width,
                height: cached.metrics.height,
                color: color_glyph,
            });
        }
        if start != self.prepared.len() as u32 {
            self.runs.push(PreparedRun {
                glyphs: start..self.prepared.len() as u32,
                top,
                bottom,
                color,
                disjoint,
            });
        }
        let (color, disjoint) = self
            .runs
            .get(run_start as usize)
            .map_or((Color::TRANSPARENT, true), |run| (run.color, run.disjoint));
        let runs = if self.runs.len() - run_start as usize > 1 {
            run_start..self.runs.len() as u32
        } else {
            self.runs.truncate(run_start as usize);
            0..0
        };
        (
            PreparedText {
                glyphs: glyph_start..self.prepared.len() as u32,
                runs,
                area,
                color,
                disjoint,
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
            ref glyphs,
            ref runs,
            area,
            color,
            disjoint,
        } = *command;
        let row_end = row.x.saturating_add(row.pixels.len() as i32);
        let clip_end = clip.x.saturating_add(clip.width);
        let mut draw = |glyphs: Range<u32>, mut color: Color, disjoint: bool| {
            color.alpha = (color.alpha as u16 * clip_coverage as u16 / 255) as u8;
            if !disjoint {
                if self.coverage.len() < row.pixels.len() {
                    self.coverage.resize(row.pixels.len(), 0);
                }
                let clear_start = (clip.x - row.x).max(0).min(row.pixels.len() as i32) as usize;
                let clear_end = (clip_end - row.x).max(0).min(row.pixels.len() as i32) as usize;
                self.coverage[clear_start..clear_end].fill(0);
            }
            let mut touched_start = row.pixels.len();
            let mut touched_end = 0usize;
            for glyph in &self.prepared[glyphs.start as usize..glyphs.end as usize] {
                let x = area.x.saturating_add(glyph.x);
                let y = area.y.saturating_add(glyph.y);
                if line < y || line >= y.saturating_add(glyph.height as i32) {
                    continue;
                }
                let left = x.max(row.x).max(clip.x);
                let right = x.saturating_add(glyph.width as i32).min(row_end).min(clip_end);
                if left >= right {
                    continue;
                }
                let len = (right - left) as usize;
                let source = (line - y) as usize * glyph.width as usize + (left - x) as usize;
                let destination_start = (left - row.x) as usize;
                let destination_end = destination_start + len;
                if glyph.color {
                    if touched_start < touched_end {
                        P::blend_alpha_slice(
                            &mut row.pixels[touched_start..touched_end],
                            color,
                            &self.coverage[touched_start..touched_end],
                        );
                        self.coverage[touched_start..touched_end].fill(0);
                        touched_start = row.pixels.len();
                        touched_end = 0;
                    }
                    // safety: rgba pixels have byte alignment and remain live until finish_frame
                    let pixels = unsafe {
                        core::slice::from_raw_parts(
                            glyph
                                .pixels
                                .as_ptr()
                                .add(source * 4)
                                .cast::<crate::pixel::PremultipliedRgbaColor>(),
                            len,
                        )
                    };
                    P::blend_texture_slice_rgba(
                        &mut row.pixels[destination_start..destination_end],
                        pixels,
                        color.alpha,
                    );
                    continue;
                }
                // safety: glyph pixels remain live until finish_frame
                let alpha = unsafe { core::slice::from_raw_parts(glyph.pixels.as_ptr().add(source), len) };
                if disjoint {
                    P::blend_alpha_slice(&mut row.pixels[destination_start..destination_end], color, alpha);
                    continue;
                }
                let overlap = touched_end.saturating_sub(destination_start).min(len);
                let destination = &mut self.coverage[destination_start..destination_end];
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
                    &self.coverage[touched_start..touched_end],
                );
            }
        };
        if runs.is_empty() {
            draw(glyphs.clone(), color, disjoint);
        } else {
            for run in &self.runs[runs.start as usize..runs.end as usize] {
                if line >= area.y.saturating_add(run.top) && line < area.y.saturating_add(run.bottom) {
                    draw(run.glyphs.clone(), run.color, run.disjoint);
                }
            }
        }
    }

    pub fn finish_frame(&mut self) {
        self.prepared.clear();
        self.runs.clear();
        self.glyphs.trim_to_weight();
        self.system.finish_frame();
    }
}

pub struct PreparedText {
    pub glyphs: Range<u32>,
    pub runs: Range<u32>,
    pub area: PhysicalRect,
    pub color: Color,
    pub disjoint: bool,
}

pub struct PreparedGlyph {
    pixels: NonNull<u8>,
    x: i32,
    y: i32,
    width: u16,
    height: u16,
    color: bool,
}

struct PreparedRun {
    glyphs: Range<u32>,
    top: i32,
    bottom: i32,
    color: Color,
    disjoint: bool,
}

struct GlyphScale;

impl Scale<GlyphKey, GlyphImage> for GlyphScale {
    fn weight(&self, _key: &GlyphKey, glyph: &GlyphImage) -> usize {
        size_of::<GlyphImage>() + glyph.pixels.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Xrgb8888;

    #[test]
    fn mixed_glyphs_preserve_color_opacity_and_painter_order() {
        let system = Box::new(blit_text_parley::Backend::new(blit_text_parley::parley::FontContext {
            collection: blit_text_parley::parley::fontique::Collection::new(
                blit_text_parley::parley::fontique::CollectionOptions {
                    shared: false,
                    system_fonts: false,
                },
            ),
            source_cache: Default::default(),
        }));
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
                glyphs: 0..3,
                runs: 0..0,
                area,
                color: Color::rgba(0, 255, 0, 128),
                disjoint: false,
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
