use blit::geometry::PhysicalRect;

use super::{raster, RenderStrategy};
use crate::{PixelBuffer, RenderContext};

#[derive(Default)]
pub struct Direct {
    normalized: Vec<PhysicalRect>,
    pieces: Vec<PhysicalRect>,
    scratch: Vec<PhysicalRect>,
}

impl<B: PixelBuffer> RenderStrategy<B> for Direct {
    fn render(&mut self, context: &mut RenderContext<B>, damage: &[PhysicalRect]) {
        let screen = PhysicalRect {
            x: context.buffer.x_offset() as i32,
            y: 0,
            width: context.buffer.width() as i32,
            height: context.buffer.height() as i32,
        };
        self.normalized.clear();
        for damage in damage {
            let Some(damage) = damage.intersection(screen) else {
                continue;
            };
            self.pieces.clear();
            self.pieces.push(damage);
            for normalized in &self.normalized {
                self.scratch.clear();
                for piece in self.pieces.drain(..) {
                    subtract(piece, *normalized, &mut self.scratch);
                }
                std::mem::swap(&mut self.pieces, &mut self.scratch);
                if self.pieces.is_empty() {
                    break;
                }
            }
            self.normalized.append(&mut self.pieces);
        }
        let damage = &self.normalized;
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
                            PhysicalRect { y: line, height: 1, ..bounds },
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

fn subtract(area: PhysicalRect, cut: PhysicalRect, output: &mut Vec<PhysicalRect>) {
    let Some(overlap) = area.intersection(cut) else {
        output.push(area);
        return;
    };
    let area_right = area.x + area.width;
    let area_bottom = area.y + area.height;
    let overlap_right = overlap.x + overlap.width;
    let overlap_bottom = overlap.y + overlap.height;
    if area.y < overlap.y {
        output.push(PhysicalRect { height: overlap.y - area.y, ..area });
    }
    if overlap_bottom < area_bottom {
        output.push(PhysicalRect { y: overlap_bottom, height: area_bottom - overlap_bottom, ..area });
    }
    if area.x < overlap.x {
        output.push(PhysicalRect { y: overlap.y, width: overlap.x - area.x, height: overlap.height, ..area });
    }
    if overlap_right < area_right {
        output.push(PhysicalRect {
            x: overlap_right,
            y: overlap.y,
            width: area_right - overlap_right,
            height: overlap.height,
        });
    }
}
