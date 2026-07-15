use blit::PhysicalRect;

use crate::{PixelBuffer, RenderContext};

use super::{RenderStrategy, raster};

#[derive(Default)]
pub struct Direct;

impl<B: PixelBuffer> RenderStrategy<B> for Direct {
    fn render(&mut self, context: &mut RenderContext<B>, damage: &[PhysicalRect]) {
        let screen = PhysicalRect {
            x: context.buffer.x_offset() as i32,
            y: 0,
            width: context.buffer.width() as i32,
            height: context.buffer.height() as i32,
        };
        let commands = &context.commands;
        let clips = &context.clips;
        let images = &context.images;
        let text = &context.text;
        let buffer = &mut context.buffer;

        for offset in commands.offsets() {
            let payload = commands.get(offset);
            let clip_id = commands.clip(offset);
            for damage in damage {
                let Some(bounds) = commands
                    .bounds(offset)
                    .intersection(*damage)
                    .and_then(|bounds| bounds.intersection(screen))
                else {
                    continue;
                };
                for line in bounds.y..bounds.y + bounds.height {
                    if clip_id == 0 {
                        raster::draw_line(
                            &payload,
                            line,
                            PhysicalRect {
                                y: line,
                                height: 1,
                                ..bounds
                            },
                            255,
                            images,
                            text,
                            buffer,
                        );
                    } else {
                        clips.for_each(
                            clip_id,
                            line,
                            bounds.x..bounds.x + bounds.width,
                            |range, coverage| {
                                raster::draw_line(
                                    &payload,
                                    line,
                                    PhysicalRect {
                                        x: range.start,
                                        y: line,
                                        width: range.end - range.start,
                                        height: 1,
                                    },
                                    coverage,
                                    images,
                                    text,
                                    buffer,
                                );
                            },
                        );
                    }
                }
            }
        }
    }
}
