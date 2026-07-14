use blit::{
    DirtyRegions, LogicalRect, PhysicalRect, TextRequest,
    widgets::{Border, BorderRadius, BoxShadowRequest, ImageRequest, Rectangle},
};
use slotmap::{KeyData, SlotMap};
use std::ops::Range;

use crate::{
    Pixel, PixelBuffer, PixelSpan, RenderContext, RendererImageId, StoredImage, TextRenderer,
    image, rectangle, shadow,
};

use super::{
    RenderStrategy,
    clip::{ClipLine, ClipSpan, ClipStack},
    command::{CommandList, Payload, PreparedText},
};

const MAX_SEGMENTS: usize = 32;

#[derive(Default)]
pub struct Scanline {
    commands: CommandList,
    damage: DirtyRegions,
    starts: Vec<usize>,
    active: Vec<usize>,
    ranges: Vec<Range<usize>>,
    segments: Vec<Segment>,
    clips: ClipStack,
    clip_ranges: Vec<Option<ClipLine>>,
}

#[derive(Clone, Copy)]
struct Segment {
    start: usize,
    end: usize,
    first: Option<usize>,
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
                            match &payload {
                                Payload::Rectangle(rectangle) => {
                                    let mut rectangle = **rectangle;
                                    if coverage != 255 {
                                        rectangle.border_color =
                                            rectangle.border_color.coverage(coverage as u32);
                                        rectangle.inner_color =
                                            rectangle.inner_color.coverage(coverage as u32);
                                    }
                                    let row = PixelSpan {
                                        x: buffer.x as i32,
                                        pixels: buffer.line_mut(line as usize),
                                    };
                                    rectangle.draw_line(line, clip, row);
                                }
                                Payload::GradientRectangle(rectangle, stops) => {
                                    let row = PixelSpan {
                                        x: buffer.x as i32,
                                        pixels: buffer.line_mut(line as usize),
                                    };
                                    rectangle.draw_line(stops, line, clip, coverage, row);
                                }
                                Payload::Image(request) => {
                                    let image =
                                        RendererImageId::from(KeyData::from_ffi(request.image.0));
                                    if let Some(image) = images.get(image) {
                                        let mut request = **request;
                                        if coverage != 255 {
                                            request.opacity =
                                                (request.opacity as u16 * coverage as u16 / 255)
                                                    as u8;
                                        }
                                        request.draw(buffer, &image.data, clip);
                                    }
                                }
                                Payload::Text(text_command) => {
                                    let mut color = text_command.color;
                                    if coverage != 255 {
                                        color.alpha =
                                            (color.alpha as u16 * coverage as u16 / 255) as u8;
                                    }
                                    let row = PixelSpan {
                                        x: buffer.x as i32,
                                        pixels: buffer.line_mut(line as usize),
                                    };
                                    text.draw_line(
                                        text_command.paragraph,
                                        text_command.area,
                                        color,
                                        line,
                                        row,
                                        clip,
                                    );
                                }
                            }
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
        match commands.get(*command) {
            Payload::Rectangle(rectangle) => {
                rectangle.draw_line(
                    line,
                    clip,
                    PixelSpan {
                        x: range.start as i32,
                        pixels: buffer.line_mut(line as usize),
                    },
                );
            }
            Payload::GradientRectangle(rectangle, stops) => {
                rectangle.draw_line(
                    stops,
                    line,
                    clip,
                    255,
                    PixelSpan {
                        x: range.start as i32,
                        pixels: buffer.line_mut(line as usize),
                    },
                );
            }
            Payload::Image(request) => {
                let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
                if let Some(image) = images.get(image) {
                    request.draw(buffer, &image.data, clip);
                }
            }
            Payload::Text(text_command) => text.draw_line(
                text_command.paragraph,
                text_command.area,
                text_command.color,
                line,
                PixelSpan {
                    x: range.start as i32,
                    pixels: buffer.line_mut(line as usize),
                },
                clip,
            ),
        }
    }
}

impl<B: PixelBuffer> RenderStrategy<B> for Scanline {
    fn begin_frame(&mut self, _: &mut RenderContext<B>, damage: &[PhysicalRect]) {
        assert!(self.commands.is_empty());
        assert!(self.damage.is_empty());
        for area in damage {
            self.damage.add(*area);
        }
    }

    fn add_damage(&mut self, area: PhysicalRect) {
        self.damage.add(area)
    }

    fn push_rounded_clip(&mut self, area: LogicalRect, radius: BorderRadius, scale_factor: f32) {
        self.clips.push(area, radius, scale_factor)
    }

    fn pop_rounded_clip(&mut self) {
        self.clips.pop()
    }

    fn draw_rectangle(
        &mut self,
        context: &mut RenderContext<B>,
        rectangle: &Rectangle<'_>,
        clip: PhysicalRect,
    ) {
        if let Border::Gradient { width, gradient } = rectangle.border
            && let Some(prepared) =
                rectangle::Gradient::new(rectangle, width, gradient, context.scale_factor)
            && let Some(bounds) = prepared.geometry.intersection(clip)
        {
            if self.commands.push_gradient_rectangle(
                prepared,
                gradient.stops,
                bounds,
                self.clips.current(),
            ) {
                return;
            }
        }
        if let Some(rectangle) = rectangle::Prepared::new(rectangle, context.scale_factor) {
            if let Some(bounds) = rectangle.geometry.intersection(clip) {
                self.commands
                    .push_rectangle(rectangle, bounds, self.clips.current());
            }
        }
    }

    fn draw_image(
        &mut self,
        context: &mut RenderContext<B>,
        request: &ImageRequest,
        clip: PhysicalRect,
    ) {
        let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
        if let Some(texture) = context.images.get(image) {
            image::prepare(
                request,
                &texture.data,
                clip,
                context.scale_factor,
                |image, bounds| {
                    self.commands
                        .push_image(image, bounds, self.clips.current(), texture.opaque)
                },
            );
        }
    }

    fn draw_box_shadow(
        &mut self,
        context: &mut RenderContext<B>,
        request: &BoxShadowRequest,
        clip: PhysicalRect,
    ) {
        let Some(request) =
            context
                .shadows
                .prepare(&mut context.images, request, context.scale_factor)
        else {
            return;
        };
        match request {
            shadow::Prepared::Rectangle(rectangle) => {
                self.draw_rectangle(context, &rectangle, clip)
            }
            shadow::Prepared::Image(request) => {
                let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
                if let Some(texture) = context.images.get(image) {
                    image::prepare(&request, &texture.data, clip, 1.0, |image, bounds| {
                        self.commands.push_image(
                            image,
                            bounds,
                            self.clips.current(),
                            texture.opaque,
                        )
                    });
                }
            }
        }
    }

    fn draw_text(
        &mut self,
        context: &mut RenderContext<B>,
        text: &TextRequest<'_>,
        clip: PhysicalRect,
    ) {
        let area = text.area.to_physical(context.scale_factor);
        let Some(bounds) = area.intersection(clip) else {
            return;
        };
        self.commands.push_text(
            PreparedText {
                paragraph: context.text.prepare(text, context.scale_factor),
                area,
                color: text.color,
            },
            bounds,
            self.clips.current(),
        );
    }

    fn end_frame(&mut self, context: &mut RenderContext<B>) {
        let width = context.buffer.width();
        let height = context.buffer.height();
        let commands = &self.commands;
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
                for region in self.damage.regions() {
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
                self.clips.line_ranges(line, &mut self.clip_ranges);
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
                let hides_expensive = |command: usize, start: i32, end: i32| {
                    commands
                        .expensive_offsets()
                        .iter()
                        .take_while(|expensive| **expensive < command)
                        .any(|expensive| {
                            let vertical = commands.vertical_bounds(*expensive);
                            let bounds = commands.horizontal_bounds(*expensive);
                            vertical.start <= line
                                && vertical.end > line
                                && bounds.start < end
                                && bounds.end > start
                        })
                };
                let partial = first == 0
                    && !commands.expensive_offsets().is_empty()
                    && commands.opaque_offsets().iter().rev().any(|command| {
                        let vertical = commands.vertical_bounds(*command);
                        if vertical.start > line || vertical.end <= line {
                            return false;
                        }
                        let Some(span) = commands.opaque_span(*command, line) else {
                            return false;
                        };
                        let start = span.start.max(range.start as i32);
                        let end = span.end.min(range.end as i32);
                        start < end
                            && (end - start) * 2 >= range.len() as i32
                            && hides_expensive(*command, start, end)
                    });

                let split = first == 0 && partial;
                if split {
                    self.segments.clear();
                    self.segments.push(Segment {
                        start: range.start,
                        end: range.end,
                        first: None,
                    });
                    for command in commands.opaque_offsets().iter().rev() {
                        let vertical = commands.vertical_bounds(*command);
                        if vertical.start > line || vertical.end <= line {
                            continue;
                        }
                        let Some(span) = commands.opaque_span(*command, line) else {
                            continue;
                        };
                        let start = span.start.max(range.start as i32) as usize;
                        let end = span.end.min(range.end as i32) as usize;
                        if start >= end {
                            continue;
                        }
                        if !hides_expensive(*command, start as i32, end as i32) {
                            continue;
                        }
                        let first = self.active.binary_search(command).unwrap();
                        if first == 0 {
                            continue;
                        }
                        let mut index = 0;
                        while index < self.segments.len() {
                            let segment = self.segments[index];
                            if segment.first.is_some() {
                                index += 1;
                                continue;
                            }
                            let start = start.max(segment.start);
                            let end = end.min(segment.end);
                            if start >= end {
                                index += 1;
                                continue;
                            }
                            let split_left = start > segment.start;
                            let split_right = end < segment.end;
                            if self.segments.len()
                                + usize::from(split_left)
                                + usize::from(split_right)
                                > MAX_SEGMENTS
                            {
                                index += 1;
                                continue;
                            }
                            match (split_left, split_right) {
                                (false, false) => {
                                    self.segments[index].first = Some(first);
                                    index += 1;
                                }
                                (false, true) => {
                                    self.segments[index] = Segment {
                                        start: segment.start,
                                        end,
                                        first: Some(first),
                                    };
                                    self.segments.insert(
                                        index + 1,
                                        Segment {
                                            start: end,
                                            end: segment.end,
                                            first: None,
                                        },
                                    );
                                    index += 2;
                                }
                                (true, false) => {
                                    self.segments[index].end = start;
                                    self.segments.insert(
                                        index + 1,
                                        Segment {
                                            start,
                                            end: segment.end,
                                            first: Some(first),
                                        },
                                    );
                                    index += 2;
                                }
                                (true, true) => {
                                    self.segments[index].end = start;
                                    self.segments.insert(
                                        index + 1,
                                        Segment {
                                            start,
                                            end,
                                            first: Some(first),
                                        },
                                    );
                                    self.segments.insert(
                                        index + 2,
                                        Segment {
                                            start: end,
                                            end: segment.end,
                                            first: None,
                                        },
                                    );
                                    index += 3;
                                }
                            }
                        }
                    }
                }

                let active = &self.active;
                let segments = &self.segments;
                buffer.process_line(line as usize, range.clone(), |pixels| {
                    let mut draw = |first, start, end| {
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
                    if split {
                        for segment in segments {
                            draw(segment.first.unwrap_or(0), segment.start, segment.end);
                        }
                    } else {
                        draw(first, range.start, range.end);
                    }
                });
            }
            line += 1;
        }
        self.commands.clear();
        self.damage = DirtyRegions::default();
        self.starts.clear();
        self.active.clear();
        self.ranges.clear();
        self.segments.clear();
        self.clips.clear();
        self.clip_ranges.clear();
    }
}
