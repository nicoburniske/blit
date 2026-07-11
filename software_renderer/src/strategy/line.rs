use bullseye::{
    Color, PhysicalRect, TextRequest,
    widgets::{ImageRequest, Rectangle},
};
use slotmap::KeyData;

use crate::{Pixel, PixelBuffer, RenderContext, RendererImageId, image, rectangle};

use super::RenderStrategy;

#[derive(Default)]
pub struct Scanline {
    commands: Vec<Command>,
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

struct Clips {
    regions: [PhysicalRect; 8],
    len: usize,
}

impl Clips {
    fn new(clips: &[PhysicalRect]) -> Self {
        assert!(clips.len() <= 8);
        let mut regions = [PhysicalRect::default(); 8];
        regions[..clips.len()].copy_from_slice(clips);
        Self {
            regions,
            len: clips.len(),
        }
    }

    fn regions(&self) -> &[PhysicalRect] {
        &self.regions[..self.len]
    }

    fn intersects_line(&self, line: i32) -> bool {
        self.regions()
            .iter()
            .any(|clip| line >= clip.y && line < clip.y + clip.height)
    }

    fn line(&self, line: i32) -> ([PhysicalRect; 8], usize) {
        let mut regions = [PhysicalRect::default(); 8];
        let mut len = 0;
        for clip in self.regions() {
            if line >= clip.y && line < clip.y + clip.height {
                regions[len] = PhysicalRect {
                    y: line,
                    height: 1,
                    ..*clip
                };
                len += 1;
            }
        }
        (regions, len)
    }
}

struct LineBuffer<'a, P> {
    pixels: &'a mut [P],
    width: usize,
    height: usize,
    line: usize,
}

impl<P: Pixel> PixelBuffer for LineBuffer<'_, P> {
    type Pixel = P;

    fn width(&self) -> usize {
        self.width
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
        for line in 0..height {
            let line = line as i32;
            if !commands.iter().any(|command| match command {
                Command::Rectangle(_, clips)
                | Command::Image(_, clips)
                | Command::Text { clips, .. } => clips.intersects_line(line),
            }) {
                continue;
            }
            buffer.process_line(line as usize, 0..width, |pixels| {
                let mut buffer = LineBuffer {
                    pixels,
                    width,
                    height,
                    line: line as usize,
                };
                for command in commands {
                    match command {
                        Command::Rectangle(rectangle, clips) => {
                            let (clips, len) = clips.line(line);
                            for clip in &clips[..len] {
                                rectangle.draw_line(line, *clip, buffer.line_mut(line as usize));
                            }
                        }
                        Command::Image(request, clips) => {
                            let (clips, len) = clips.line(line);
                            let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
                            if let (Some(image), true) = (images.get(image), len != 0) {
                                image::draw(
                                    &mut buffer,
                                    request,
                                    &image.data,
                                    &clips[..len],
                                    scale_factor,
                                );
                            }
                        }
                        Command::Text {
                            paragraph,
                            area,
                            color,
                            clips,
                        } => text.draw_line(
                            *paragraph,
                            *area,
                            *color,
                            line,
                            buffer.line_mut(line as usize),
                            clips.regions(),
                        ),
                    }
                }
            });
        }
        self.commands.clear();
    }
}
