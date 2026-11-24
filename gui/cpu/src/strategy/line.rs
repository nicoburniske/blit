use alloc::vec::Vec;
use core::ops::Range;

use blit::PhysicalRect;

use super::{
    RenderStrategy,
    clip::{ClipId, ClipLine},
    command::{CommandId, Payload},
    raster,
};
use crate::{DamageTracker, Pixel, PixelBuffer, PixelSpan, PremultipliedRgbaColor, RenderState};

pub struct Scanline {
    span_occlusion: bool,
    starts: Vec<CommandId>,
    active: Vec<CommandId>,
    ranges: Vec<Range<usize>>,
    plan: Vec<Draw>,
    uncovered: Vec<Range<usize>>,
    remaining: Vec<Range<usize>>,
    edges: Vec<i32>,
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
            plan: Vec::new(),
            uncovered: Vec::new(),
            remaining: Vec::new(),
            edges: Vec::new(),
        }
    }
}

impl<B: PixelBuffer> RenderStrategy<B> for Scanline {
    fn render(&mut self, context: &mut RenderState<B>, damage: &impl DamageTracker) {
        let width = context.buffer.width();
        let height = context.buffer.height();
        let screen = PhysicalRect::new(0, 0, width as i32, height as i32);
        let commands = &mut context.raster;
        let list = &context.list;
        let clips = &context.clips;
        let images = &mut context.images;
        let text = &mut context.text;
        let buffer = &mut context.buffer;
        self.starts.extend(
            commands
                .range
                .clone()
                .filter(|id| damage.intersects(commands.bounds(*id), commands.clip(*id))),
        );
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

            // reuse the plan until command bounds or coverage can change
            let mut until = ranges_valid_until;
            if let Some(&command) = self.starts.get(next) {
                until = until.min(commands.vertical_bounds(command).start);
            }

            // plan front to back then replay in painter order
            self.plan.clear();
            self.uncovered.clear();
            self.uncovered.extend(self.ranges.iter().cloned());
            for &command in self.active.iter().rev() {
                if self.uncovered.is_empty() {
                    break;
                }
                let id = commands.clip(command);
                let Some(bounds) = clips.bounds(id, commands.bounds(command)) else {
                    continue;
                };
                let bounds = bounds.x..bounds.x + bounds.width;
                let first_gap = self.uncovered.partition_point(|gap| gap.end as i32 <= bounds.start);
                if first_gap == self.uncovered.len() || self.uncovered[first_gap].start as i32 >= bounds.end {
                    continue;
                }
                // corners only move one way within a zone so its first and last rows bound the opaque span
                let clip = clips.line(id, line);
                until = until.min(clip.until);
                let last = clips.line(id, clip.until.saturating_sub(1).max(line));
                let opaque = if clip.range.is_empty() || last.range.is_empty() {
                    0..0
                } else {
                    clip.opaque.start.max(last.opaque.start)..clip.opaque.end.min(last.opaque.end)
                };
                let first = self.plan.len();
                for gap in &self.uncovered[first_gap..] {
                    if gap.start as i32 >= bounds.end {
                        break;
                    }
                    let range = (gap.start as i32).max(bounds.start)..(gap.end as i32).min(bounds.end);
                    // spans reaching past the opaque span resolve clip coverage per row
                    let clip = if opaque.start <= range.start && range.end <= opaque.end {
                        0
                    } else {
                        id
                    };
                    self.plan.push(Draw {
                        command,
                        range,
                        row: Row::Clip(clip),
                    });
                }
                if first == self.plan.len() {
                    continue;
                }
                until = until.min(commands.vertical_bounds(command).end);
                if !self.span_occlusion {
                    continue;
                }
                let (span, zone) = commands.opaque_span(command, line, until, list, clips, images, text);
                until = until.min(zone);
                let (last, _) = commands.opaque_span(
                    command,
                    until.saturating_sub(1).max(line),
                    until,
                    list,
                    clips,
                    images,
                    text,
                );
                let start = span.start.max(last.start).max(opaque.start).max(0) as usize;
                let end = span.end.min(last.end).min(opaque.end).max(0) as usize;
                if start >= end {
                    continue;
                }
                self.remaining.clear();
                for gap in self.uncovered.drain(..) {
                    let claimed = gap.start.max(start)..gap.end.min(end);
                    if claimed.is_empty() {
                        self.remaining.push(gap);
                        continue;
                    }
                    if gap.start < claimed.start {
                        self.remaining.push(gap.start..claimed.start);
                    }
                    if claimed.end < gap.end {
                        self.remaining.push(claimed.end..gap.end);
                    }
                }
                core::mem::swap(&mut self.uncovered, &mut self.remaining);
            }

            // resolve band wide colors in painter order since preparation fuses neighbors
            for draw in self.plan.iter_mut().rev() {
                if let Row::Clip(0) = draw.row
                    && let Payload::Rectangle(rectangle) = commands.prepare(draw.command, list, clips, images, text)
                    && let Some(color) = rectangle.uniform(line..until, draw.range.clone())
                {
                    draw.row = Row::Fill(color);
                }
            }
            // sort by x for memory locality when writing rows left to right
            // split overlaps within the draw budget or replay whole spans in painter order
            self.plan.sort_unstable_by_key(|draw| draw.range.start);
            if self.plan.windows(2).any(|pair| pair[0].range.end > pair[1].range.start) {
                self.edges.clear();
                self.edges
                    .extend(self.plan.iter().flat_map(|draw| [draw.range.start, draw.range.end]));
                self.edges.sort_unstable();
                self.edges.dedup();
                let edges = &self.edges;
                let cuts = |range: &Range<i32>| {
                    let first = edges.partition_point(|edge| *edge <= range.start);
                    let end = edges.partition_point(|edge| *edge < range.end);
                    &edges[first..end]
                };
                let original = self.plan.len();
                // each cut adds a draw, so allow at most one extra draw per original span
                let within_budget = self
                    .plan
                    .iter()
                    .try_fold(original, |budget, draw| budget.checked_sub(cuts(&draw.range).len()));
                if within_budget.is_some() {
                    for index in 0..original {
                        let Draw {
                            command,
                            ref range,
                            row,
                        } = self.plan[index];
                        let mut start = range.start;
                        for &end in cuts(range) {
                            self.plan.push(Draw {
                                command,
                                range: start..end,
                                row,
                            });
                            start = end;
                        }
                        self.plan[index].range.start = start;
                    }
                    self.plan.sort_unstable_by_key(|draw| (draw.range.start, draw.command));
                } else {
                    self.plan.sort_unstable_by_key(|draw| (draw.command, draw.range.start));
                }
            }
            let mut draw_line = |command, line, range: Range<i32>, coverage, row: PixelSpan<'_, B::Pixel>| {
                let span = PhysicalRect::new(range.start, line, range.end - range.start, 1);
                raster::draw_line(commands, list, clips, command, line, span, coverage, images, text, row);
            };
            for line in line..until {
                // draws sharing a clip sit next to each other so the last resolved clip row is reused
                let mut shared: Option<(ClipId, ClipLine)> = None;
                for draw in &self.plan {
                    let (command, x) = (draw.command, draw.range.start);
                    let pixels = buffer.write_line(line as usize, x as usize..draw.range.end as usize);
                    match draw.row {
                        Row::Fill(color) => B::Pixel::blend_slice(pixels, color),
                        Row::Clip(0) => draw_line(command, line, draw.range.clone(), 255, PixelSpan { x, pixels }),
                        Row::Clip(clip) => {
                            shared.take_if(|(id, _)| *id != clip);
                            let (_, clip) = shared.get_or_insert_with(|| (clip, clips.line(clip, line)));
                            clip.for_each(draw.range.clone(), |range, coverage| {
                                draw_line(
                                    command,
                                    line,
                                    range,
                                    coverage,
                                    PixelSpan {
                                        x,
                                        pixels: &mut *pixels,
                                    },
                                )
                            });
                        }
                    }
                }
            }
            line = until;
        }
        self.starts.clear();
        self.active.clear();
        self.ranges.clear();
        self.plan.clear();
    }
}

struct Draw {
    command: CommandId,
    range: Range<i32>,
    row: Row,
}

/// how a planned span draws each row of its band
#[derive(Clone, Copy)]
enum Row {
    /// one color resolved for the whole band
    Fill(PremultipliedRgbaColor),
    /// draws through a clip resolved per row where zero means fully inside
    Clip(ClipId),
}
