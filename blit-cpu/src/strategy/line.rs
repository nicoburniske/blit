use blit::PhysicalRect;
use slotmap::{KeyData, SlotMap};
use std::ops::Range;

use crate::{Pixel, PixelBuffer, RenderContext, RendererImageId, StoredImage, TextRenderer};

use super::{
    RenderStrategy,
    clip::{ClipLine, ClipSpan},
    command::{CommandList, Payload},
    raster,
};

#[derive(Default)]
pub struct Scanline {
    starts: Vec<usize>,
    active: Vec<usize>,
    ranges: Vec<Range<usize>>,
    clip_ranges: Vec<Option<ClipLine>>,
}

struct LineBuffer<'a, P> {
    pixels: &'a mut [P],
    x: usize,
    height: usize,
    line: usize,
}

impl<P: Pixel> PixelBuffer for LineBuffer<'_, P> {
    type Pixel = P;

    fn x_offset(&self) -> usize {
        self.x
    }

    fn width(&self) -> usize {
        self.pixels.len()
    }

    fn height(&self) -> usize {
        self.height
    }

    fn line_mut(&mut self, line: usize) -> &mut [P] {
        assert_eq!(line, self.line);
        self.pixels
    }
}

fn draw_commands<const CLIPPED: bool, P: Pixel>(
    commands: &CommandList,
    active: &[usize],
    clip_ranges: &[Option<ClipLine>],
    images: &SlotMap<RendererImageId, StoredImage>,
    text: &TextRenderer,
    buffer: &mut LineBuffer<'_, P>,
) {
    let line = buffer.line as i32;
    let range = buffer.x..buffer.x + buffer.pixels.len();
    for command in active {
        let bounds = commands.horizontal_bounds(*command);
        if bounds.end <= range.start as i32 || bounds.start >= range.end as i32 {
            continue;
        }
        let mut start = bounds.start.max(range.start as i32);
        let mut end = bounds.end.min(range.end as i32);
        if CLIPPED {
            let clip_id = commands.clip(*command);
            if clip_id != 0 {
                let Some(clipped) = &clip_ranges[clip_id as usize - 1] else {
                    continue;
                };
                start = start.max(clipped.start);
                end = end.min(clipped.end);
                if start >= end {
                    continue;
                }
                if start < clipped.full_start || end > clipped.full_end {
                    let payload = commands.get(*command);
                    ClipSpan {
                        start,
                        end,
                        full_start: clipped.full_start,
                        full_end: clipped.full_end,
                    }
                    .for_each(
                        |x| {
                            let mut id = clip_id;
                            let mut coverage = 255u32;
                            while id != 0 {
                                let Some(line) = &clip_ranges[id as usize - 1] else {
                                    return 0;
                                };
                                coverage = (coverage * line.rounded.coverage(x) as u32 + 127) / 255;
                                id = line.parent;
                            }
                            coverage as u8
                        },
                        |range, coverage| {
                            let clip = PhysicalRect {
                                x: range.start,
                                y: line,
                                width: range.end - range.start,
                                height: 1,
                            };
                            raster::draw_line(&payload, line, clip, coverage, images, text, buffer);
                        },
                    );
                    continue;
                }
            }
        }
        let clip = PhysicalRect {
            x: start,
            y: line,
            width: end - start,
            height: 1,
        };
        raster::draw_line(
            &commands.get(*command),
            line,
            clip,
            255,
            images,
            text,
            buffer,
        );
    }
}

impl<B: PixelBuffer> RenderStrategy<B> for Scanline {
    fn render(&mut self, context: &mut RenderContext<B>, damage: &[PhysicalRect]) {
        let width = context.buffer.width();
        let height = context.buffer.height();
        let commands = &context.commands;
        let clips = &context.clips;
        let images = &context.images;
        let text = &context.text;
        let buffer = &mut context.buffer;
        let clipped = commands.has_clips;

        self.starts.extend(commands.offsets());
        self.starts.sort_unstable_by(|left, right| {
            commands
                .vertical_bounds(*left)
                .start
                .cmp(&commands.vertical_bounds(*right).start)
                .then(left.cmp(right))
        });
        let mut next = 0;
        let mut line = 0;
        let mut ranges_valid_until = 0;
        while line < height as i32 {
            if line >= ranges_valid_until {
                self.ranges.clear();
                let mut next_boundary = height as i32;
                for region in damage {
                    let top = region.y.max(0).min(height as i32);
                    let bottom = region
                        .y
                        .saturating_add(region.height)
                        .max(0)
                        .min(height as i32);
                    if bottom <= line {
                        continue;
                    }
                    if top > line {
                        next_boundary = next_boundary.min(top);
                        continue;
                    }
                    next_boundary = next_boundary.min(bottom);
                    let start = region.x.max(0).min(width as i32) as usize;
                    let end = region
                        .x
                        .saturating_add(region.width)
                        .max(0)
                        .min(width as i32) as usize;
                    if start < end {
                        self.ranges.push(start..end);
                    }
                }
                if self.ranges.is_empty() {
                    line = next_boundary;
                    ranges_valid_until = line;
                    continue;
                }
                self.ranges.sort_unstable_by_key(|range| range.start);
                let mut merged = 0;
                for index in 0..self.ranges.len() {
                    let start = self.ranges[index].start;
                    let end = self.ranges[index].end;
                    if merged != 0 && start <= self.ranges[merged - 1].end {
                        self.ranges[merged - 1].end = self.ranges[merged - 1].end.max(end);
                    } else {
                        self.ranges[merged] = start..end;
                        merged += 1;
                    }
                }
                self.ranges.truncate(merged);
                ranges_valid_until = next_boundary;
            }

            self.active
                .retain(|command| commands.vertical_bounds(*command).end > line);
            while next < self.starts.len()
                && commands.vertical_bounds(self.starts[next]).start <= line
            {
                let command = self.starts[next];
                if commands.vertical_bounds(command).end > line {
                    let position = self.active.binary_search(&command).unwrap_err();
                    self.active.insert(position, command);
                }
                next += 1;
            }
            if self.active.is_empty() {
                line = self.starts.get(next).map_or(ranges_valid_until, |command| {
                    commands
                        .vertical_bounds(*command)
                        .start
                        .max(line + 1)
                        .min(ranges_valid_until)
                });
                continue;
            }

            if clipped {
                clips.line_ranges(line, &mut self.clip_ranges);
            }

            for range in &self.ranges {
                let first = commands
                    .opaque_offsets()
                    .iter()
                    .rev()
                    .find(|command| {
                        let vertical = commands.vertical_bounds(**command);
                        vertical.start <= line
                            && vertical.end > line
                            && commands.opaque_span(**command, line).is_some_and(|span| {
                                span.start <= range.start as i32 && span.end >= range.end as i32
                            })
                    })
                    .map(|command| self.active.binary_search(command).unwrap())
                    .unwrap_or(0);
                if commands.partial_opaque_offsets().is_empty() {
                    let active = &self.active[first..];
                    buffer.process_line(line as usize, range.clone(), |pixels| {
                        let mut buffer = LineBuffer {
                            pixels,
                            x: range.start,
                            height,
                            line: line as usize,
                        };
                        if clipped {
                            draw_commands::<true, _>(
                                commands,
                                active,
                                &self.clip_ranges,
                                images,
                                text,
                                &mut buffer,
                            );
                        } else {
                            draw_commands::<false, _>(
                                commands,
                                active,
                                &self.clip_ranges,
                                images,
                                text,
                                &mut buffer,
                            );
                        }
                    });
                    continue;
                }
                let partial_opaque = commands.partial_opaque_offsets();
                let partial_opaque = &partial_opaque
                    [partial_opaque.partition_point(|command| *command <= self.active[first])..];
                let occluder = partial_opaque.iter().rev().find_map(|command| {
                    let command_first = self.active.binary_search(command).ok()?;
                    let Payload::Image(image) = commands.get(*command) else {
                        unreachable!()
                    };
                    let image_id = RendererImageId::from(KeyData::from_ffi(image.image.0));
                    let texture = images.get(image_id)?;
                    let span = image.opaque_span(line, &texture.alpha_rows)?;
                    let start = span.start.max(range.start as i32);
                    let end = span.end.min(range.end as i32);
                    (start < end).then_some((start as usize, end as usize, command_first))
                });
                let active = &self.active;
                buffer.process_line(line as usize, range.clone(), |pixels| {
                    let mut draw = |first: usize, start: usize, end: usize| {
                        if start >= end {
                            return;
                        }
                        let offset = start - range.start;
                        let mut buffer = LineBuffer {
                            pixels: &mut pixels[offset..offset + end - start],
                            x: start,
                            height,
                            line: line as usize,
                        };
                        if clipped {
                            draw_commands::<true, _>(
                                commands,
                                &active[first..],
                                &self.clip_ranges,
                                images,
                                text,
                                &mut buffer,
                            );
                        } else {
                            draw_commands::<false, _>(
                                commands,
                                &active[first..],
                                &self.clip_ranges,
                                images,
                                text,
                                &mut buffer,
                            );
                        }
                    };
                    if let Some((start, end, occluder)) = occluder {
                        draw(first, range.start, start);
                        draw(occluder, start, end);
                        draw(first, end, range.end);
                    } else {
                        draw(first, range.start, range.end);
                    }
                });
            }
            line += 1;
        }
        self.starts.clear();
        self.active.clear();
        self.ranges.clear();
        self.clip_ranges.clear();
    }
}
