use std::ops::Range;

use blit::geometry::PhysicalRect;

use super::{RenderStrategy, raster};
use crate::{Damage, PixelBuffer, RenderContext};

#[derive(Default)]
pub struct Direct {
    ranges: Vec<Range<usize>>,
}

impl<B: PixelBuffer> RenderStrategy<B> for Direct {
    fn render(&mut self, context: &mut RenderContext<B>, damage: &Damage) {
        let screen = PhysicalRect {
            x: context.buffer.x_offset() as i32,
            y: 0,
            width: context.buffer.width() as i32,
            height: context.buffer.height() as i32,
        };
        let commands = &context.commands;
        let clips = &context.clips;
        let images = &context.images;
        let text = &mut context.text;
        let buffer = &mut context.buffer;

        for offset in commands.offsets() {
            let Some(bounds) = commands.bounds(offset).intersection(screen) else {
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
                            raster::draw_line(&payload, line, clip, 255, images, text, buffer);
                        } else {
                            clips.for_each(clip_id, line, clip.x..clip.x + clip.width, |range, coverage| {
                                raster::draw_line(
                                    &payload,
                                    line,
                                    PhysicalRect::new(range.start, line, range.end - range.start, 1),
                                    coverage,
                                    images,
                                    text,
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
