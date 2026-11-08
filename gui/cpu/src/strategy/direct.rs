use std::ops::Range;

use blit::geometry::PhysicalRect;

use super::{RenderStrategy, raster};
use crate::{DamageTracker, PixelBuffer, RenderContext};

#[derive(Default)]
pub struct Direct {
    ranges: Vec<Range<usize>>,
}

impl<B: PixelBuffer> RenderStrategy<B> for Direct {
    fn render<H>(&mut self, context: &mut RenderContext<B, H>, damage: &impl DamageTracker) {
        let screen = PhysicalRect {
            x: context.buffer.x_offset() as i32,
            y: 0,
            width: context.buffer.width() as i32,
            height: context.buffer.height() as i32,
        };
        let commands = &context.commands;
        let clips = &context.clips;
        let images = &mut context.images;
        let shadow = &mut context.shadow;
        let text = &mut context.text;
        let data = &commands.data;
        let handlers = &mut context.handlers;
        let buffer = &mut context.buffer;

        for offset in commands.offsets() {
            let draw_bounds = commands.bounds(offset);
            let Some(bounds) = draw_bounds.intersection(screen) else {
                continue;
            };
            if !damage.intersects(bounds, commands.clip(offset)) {
                continue;
            }
            let payload = commands.get(offset);
            let clip_id = commands.clip(offset);
            let mut line = bounds.y;
            while line < bounds.y + bounds.height {
                let end = damage.line_ranges(line, bounds, &mut self.ranges);
                for line in line..end {
                    for range in &self.ranges {
                        let clip = PhysicalRect::new(range.start as i32, line, (range.end - range.start) as i32, 1);
                        if clip_id == 0 {
                            raster::draw_line(
                                &payload,
                                line,
                                draw_bounds,
                                clip,
                                255,
                                images,
                                shadow,
                                text,
                                data,
                                handlers,
                                buffer,
                            );
                        } else {
                            clips.for_each(clip_id, line, clip.x..clip.x + clip.width, |range, coverage| {
                                raster::draw_line(
                                    &payload,
                                    line,
                                    draw_bounds,
                                    PhysicalRect::new(range.start, line, range.end - range.start, 1),
                                    coverage,
                                    images,
                                    shadow,
                                    text,
                                    data,
                                    handlers,
                                    buffer,
                                );
                            });
                        }
                    }
                }
                line = end;
            }
        }
        self.ranges.clear();
    }
}
