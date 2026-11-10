use std::ops::Range;

use blit::geometry::PhysicalRect;
use blit_arena::Arena;
use blit_gui::display_list::DisplayList;

use super::{
    RenderStrategy,
    clip::{ClipLine, ClipSpan},
    command::{CommandId, Coverage, RasterCommands},
    raster,
};
use crate::{DamageTracker, Pixel, PixelBuffer, PixelSpan, RenderContext, StoredImage, TextRenderer};

pub struct Scanline {
    span_occlusion: bool,
    starts: Vec<CommandId>,
    active: Vec<CommandId>,
    ranges: Vec<Range<usize>>,
    clip_ranges: Vec<Option<ClipLine>>,
}

impl Scanline {
    /// controls additional span occlusion which is enabled by default
    pub fn span_occlusion(mut self, enabled: bool) -> Self {
        self.span_occlusion = enabled;
        self
    }
}

impl Default for Scanline {
    fn default() -> Self {
        Self {
            span_occlusion: true,
            starts: Vec::new(),
            active: Vec::new(),
            ranges: Vec::new(),
            clip_ranges: Vec::new(),
        }
    }
}

impl<B: PixelBuffer> RenderStrategy<B> for Scanline {
    fn render(&mut self, context: &mut RenderContext<B>, damage: &impl DamageTracker) {
        let width = context.buffer.width();
        let height = context.buffer.height();
        let screen = PhysicalRect::new(0, 0, width as i32, height as i32);
        let commands = &mut context.raster;
        let list = &context.list;
        let clips = &context.clips;
        let images = &mut context.images;
        let text = &mut context.text;
        let buffer = &mut context.buffer;
        let mut clipped = false;
        let mut partial_opaque = false;
        self.starts.extend(
            commands
                .range
                .clone()
                .filter(|id| damage.intersects(commands.bounds(*id), commands.clip(*id))),
        );
        for &id in &self.starts {
            let metadata = &commands.metadata[id as usize];
            clipped |= metadata.clip != 0;
            partial_opaque |= matches!(metadata.coverage, Coverage::PartialImage | Coverage::Rect);
        }
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
                ranges_valid_until = damage.line_ranges(line, screen, &mut self.ranges);
                if self.ranges.is_empty() {
                    line = ranges_valid_until;
                    continue;
                }
            }

            self.active
                .retain(|command| commands.vertical_bounds(*command).end > line);
            while next < self.starts.len() && commands.vertical_bounds(self.starts[next]).start <= line {
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

            let clip_span = |clip: u32, mut span: Range<i32>| {
                if clip != 0 {
                    let clip = self.clip_ranges[clip as usize - 1].as_ref()?;
                    span.start = span.start.max(clip.full_start);
                    span.end = span.end.min(clip.full_end);
                }
                (span.start < span.end).then_some(span)
            };

            for range in &self.ranges {
                let mut first = 0;
                let mut overwrite = None;
                let split = range.len() >= 128 || partial_opaque;
                if self.span_occlusion {
                    for (index, &command) in self.active.iter().enumerate().rev() {
                        if commands.metadata[command as usize].coverage == Coverage::None {
                            continue;
                        }
                        let bounds = commands.horizontal_bounds(command);
                        let spans_range = bounds.start <= range.start as i32 && bounds.end >= range.end as i32;
                        if bounds.end <= range.start as i32
                            || bounds.start >= range.end as i32
                            || (!spans_range && (!split || overwrite.is_some()))
                        {
                            continue;
                        }
                        let Some(span) = commands
                            .opaque_span(command, line, list, images, text)
                            .and_then(|span| clip_span(commands.clip(command), span))
                        else {
                            continue;
                        };
                        let start = span.start.max(range.start as i32);
                        let end = span.end.min(range.end as i32);
                        if start >= end {
                            continue;
                        }
                        let (start, end) = (start as usize, end as usize);
                        if start == range.start && end == range.end {
                            first = index;
                            break;
                        }
                        if split && overwrite.is_none() {
                            overwrite = Some((start, end, index));
                        }
                    }
                }
                let pixels = buffer.write_line(line as usize, range.clone());
                let mut draw = |first: usize, start: usize, end: usize| {
                    if start >= end {
                        return;
                    }
                    let offset = start - range.start;
                    let row = PixelSpan {
                        x: start as i32,
                        pixels: &mut pixels[offset..offset + end - start],
                    };
                    if clipped {
                        draw_commands::<true, _>(
                            commands,
                            list,
                            &self.active[first..],
                            &self.clip_ranges,
                            images,
                            text,
                            row,
                            line,
                        );
                    } else {
                        draw_commands::<false, _>(
                            commands,
                            list,
                            &self.active[first..],
                            &self.clip_ranges,
                            images,
                            text,
                            row,
                            line,
                        );
                    }
                };
                if let Some((start, end, overwrite)) = overwrite {
                    // only the uncovered edges need commands behind the opaque span
                    draw(first, range.start, start);
                    draw(overwrite, start, end);
                    draw(first, end, range.end);
                } else {
                    draw(first, range.start, range.end);
                }
            }
            line += 1;
        }
        self.starts.clear();
        self.active.clear();
        self.ranges.clear();
        self.clip_ranges.clear();
    }
}

fn draw_commands<const CLIPPED: bool, P: Pixel>(
    commands: &mut RasterCommands<P>,
    list: &DisplayList,
    active: &[CommandId],
    clip_ranges: &[Option<ClipLine>],
    images: &mut Arena<StoredImage>,
    text: &mut TextRenderer,
    row: PixelSpan<'_, P>,
    line: i32,
) {
    let range = row.x as usize..row.x as usize + row.pixels.len();
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
                            raster::draw_line(
                                commands,
                                list,
                                *command,
                                line,
                                clip,
                                coverage,
                                images,
                                text,
                                PixelSpan {
                                    x: row.x,
                                    pixels: row.pixels,
                                },
                            );
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
            commands,
            list,
            *command,
            line,
            clip,
            255,
            images,
            text,
            PixelSpan {
                x: row.x,
                pixels: row.pixels,
            },
        );
    }
}
