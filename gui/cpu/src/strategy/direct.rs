use std::ops::Range;

use blit::geometry::PhysicalRect;

use super::{RenderStrategy, raster};
use crate::{DamageTracker, PixelBuffer, PixelSpan, RenderState};

#[derive(Default)]
pub struct Direct {
    ranges: Vec<Range<usize>>,
}

impl<B: PixelBuffer> RenderStrategy<B> for Direct {
    fn render(&mut self, context: &mut RenderState<B>, damage: &impl DamageTracker) {
        let screen = context.raster.screen;
        let commands = &mut context.raster;
        let list = &context.list;
        let clips = &context.clips;
        let images = &mut context.images;
        let text = &mut context.text;
        let buffer = &mut context.buffer;

        for offset in commands.range.clone() {
            let clip = commands.clip(offset);
            let Some(bounds) = clips
                .bounds(clip, commands.bounds(offset))
                .and_then(|bounds| bounds.intersection(screen))
            else {
                continue;
            };
            if !damage.intersects(bounds, clip) {
                continue;
            }
            let mut line = bounds.y;
            while line < bounds.y + bounds.height {
                let end = damage.line_ranges(line, bounds, &mut self.ranges);
                for line in line..end {
                    let clip_line = clips.line(clip, line);
                    for range in &self.ranges {
                        clip_line.for_each(range.start as i32..range.end as i32, |range, coverage| {
                            raster::draw_line(
                                commands,
                                list,
                                clips,
                                offset,
                                line,
                                PhysicalRect::new(range.start, line, range.end - range.start, 1),
                                coverage,
                                images,
                                text,
                                PixelSpan {
                                    x: screen.x,
                                    pixels: buffer.line_mut(line as usize),
                                },
                            );
                        });
                    }
                }
                line = end;
            }
        }
        self.ranges.clear();
    }
}
