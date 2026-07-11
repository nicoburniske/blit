use bullseye::{
    Color, PhysicalRect, TextRequest,
    widgets::{ImageRequest, Rectangle},
};
use slotmap::KeyData;
use std::ops::Range;

use crate::{Pixel, PixelBuffer, PixelSpan, RenderContext, RendererImageId, image, rectangle};

use super::RenderStrategy;

#[derive(Default)]
pub struct Scanline {
    commands: Vec<Command>,
    starts: Vec<usize>,
    active: Vec<usize>,
    ranges: Vec<Range<usize>>,
}

enum Command {
    Rectangle(rectangle::PreparedRectangle, Clips),
    Image(ImageRequest, Clips),
    Text {
        paragraph: usize,
        area: PhysicalRect,
        color: Color,
        clips: Clips,
    },
}

impl Command {
    fn clips(&self) -> &Clips {
        match self {
            Self::Rectangle(_, clips) | Self::Image(_, clips) | Self::Text { clips, .. } => clips,
        }
    }
}

struct Clips {
    regions: [PhysicalRect; 8],
    len: usize,
    top: i32,
    bottom: i32,
}

impl Clips {
    fn new(clips: &[PhysicalRect]) -> Self {
        assert!(!clips.is_empty());
        assert!(clips.len() <= 8);
        let mut regions = [PhysicalRect::default(); 8];
        regions[..clips.len()].copy_from_slice(clips);
        Self {
            regions,
            len: clips.len(),
            top: clips.iter().map(|clip| clip.y).min().unwrap(),
            bottom: clips
                .iter()
                .map(|clip| clip.y.saturating_add(clip.height))
                .max()
                .unwrap(),
        }
    }

    fn regions(&self) -> &[PhysicalRect] {
        &self.regions[..self.len]
    }

    fn top(&self) -> i32 {
        self.top
    }

    fn bottom(&self) -> i32 {
        self.bottom
    }

    fn line(&self, line: i32, range: Range<usize>) -> ([PhysicalRect; 8], usize) {
        let mut regions = [PhysicalRect::default(); 8];
        let mut len = 0;
        let line = PhysicalRect {
            x: range.start as i32,
            y: line,
            width: range.len() as i32,
            height: 1,
        };
        for clip in self.regions() {
            if let Some(clip) = clip.intersection(line) {
                regions[len] = clip;
                len += 1;
            }
        }
        (regions, len)
    }
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

impl<B: PixelBuffer> RenderStrategy<B> for Scanline {
    fn begin_frame(&mut self, _: &mut RenderContext<B>) {
        assert!(self.commands.is_empty());
    }

    fn draw_rectangle(
        &mut self,
        context: &mut RenderContext<B>,
        rectangle: &Rectangle,
        clips: &[PhysicalRect],
    ) {
        if let Some(rectangle) = rectangle::PreparedRectangle::new(rectangle, context.scale_factor)
        {
            self.commands
                .push(Command::Rectangle(rectangle, Clips::new(clips)));
        }
    }

    fn draw_image(
        &mut self,
        _: &mut RenderContext<B>,
        image: &ImageRequest,
        clips: &[PhysicalRect],
    ) {
        self.commands
            .push(Command::Image(*image, Clips::new(clips)));
    }

    fn draw_text(
        &mut self,
        context: &mut RenderContext<B>,
        text: &TextRequest<'_>,
        clips: &[PhysicalRect],
    ) {
        self.commands.push(Command::Text {
            paragraph: context.text.prepare(text, context.scale_factor),
            area: text.area.to_physical(context.scale_factor),
            color: text.color,
            clips: Clips::new(clips),
        });
    }

    fn end_frame(&mut self, context: &mut RenderContext<B>) {
        let width = context.buffer.width();
        let height = context.buffer.height();
        let scale_factor = context.scale_factor;
        let commands = &self.commands;
        let images = &context.images;
        let text = &context.text;
        let buffer = &mut context.buffer;

        self.starts.extend(0..commands.len());
        self.starts.sort_unstable_by(|left, right| {
            commands[*left]
                .clips()
                .top()
                .cmp(&commands[*right].clips().top())
                .then(left.cmp(right))
        });
        let mut next = 0;
        for line in 0..height {
            let line = line as i32;
            self.active
                .retain(|command| commands[*command].clips().bottom() > line);
            while next < self.starts.len() && commands[self.starts[next]].clips().top() <= line {
                let command = self.starts[next];
                if commands[command].clips().bottom() > line {
                    let position = self.active.binary_search(&command).unwrap_err();
                    self.active.insert(position, command);
                }
                next += 1;
            }
            if self.active.is_empty() {
                continue;
            }

            self.ranges.clear();
            for command in &self.active {
                for clip in commands[*command].clips().regions() {
                    if line >= clip.y && line < clip.y.saturating_add(clip.height) {
                        let start = clip.x.max(0).min(width as i32) as usize;
                        let end =
                            clip.x.saturating_add(clip.width).max(0).min(width as i32) as usize;
                        if start < end {
                            self.ranges.push(start..end);
                        }
                    }
                }
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

            for range in &self.ranges {
                buffer.process_line(line as usize, range.clone(), |pixels| {
                    let mut buffer = LineBuffer {
                        pixels,
                        x: range.start,
                        height,
                        line: line as usize,
                    };
                    for command in &self.active {
                        let command = &commands[*command];
                        let clips = command.clips();
                        let (line_clips, len) = clips.line(line, range.clone());
                        if len == 0 {
                            continue;
                        }
                        match command {
                            Command::Rectangle(rectangle, _) => {
                                for clip in &line_clips[..len] {
                                    rectangle.draw_line(
                                        line,
                                        *clip,
                                        PixelSpan {
                                            x: range.start as i32,
                                            pixels: buffer.line_mut(line as usize),
                                        },
                                    );
                                }
                            }
                            Command::Image(request, _) => {
                                let image =
                                    RendererImageId::from(KeyData::from_ffi(request.image.0));
                                if let Some(image) = images.get(image) {
                                    image::draw(
                                        &mut buffer,
                                        request,
                                        &image.data,
                                        &line_clips[..len],
                                        scale_factor,
                                    );
                                }
                            }
                            Command::Text {
                                paragraph,
                                area,
                                color,
                                clips: _,
                            } => text.draw_line(
                                *paragraph,
                                *area,
                                *color,
                                line,
                                PixelSpan {
                                    x: range.start as i32,
                                    pixels: buffer.line_mut(line as usize),
                                },
                                &line_clips[..len],
                            ),
                        }
                    }
                });
            }
        }
        self.commands.clear();
        self.starts.clear();
        self.active.clear();
        self.ranges.clear();
    }
}
