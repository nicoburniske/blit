use blit::geometry::PhysicalRect;
use blit_arena::Arena;

use super::command::{CommandId, CommandList, Payload};
use crate::{DrawContext, Pixel, PixelBuffer, PixelSpan, StoredImage, TextRenderer, paint::DrawTarget};

#[inline(always)]
pub fn draw_line<B: PixelBuffer>(
    commands: &mut CommandList<B::Pixel>,
    command: CommandId,
    line: i32,
    clip: PhysicalRect,
    coverage: u8,
    images: &mut Arena<StoredImage>,
    text: &mut TextRenderer,
    buffer: &mut B,
) {
    if matches!(commands.get(command), Payload::Pending) {
        commands.prepare(command, images, text);
    }
    let bounds = commands.bounds(command);
    match commands.get(command) {
        Payload::Pending | Payload::Empty => {}
        Payload::Custom => {
            let call = &mut commands.entries[command as usize];
            if call.reads.is_some() {
                return;
            }
            let start = (clip.x - buffer.x_offset() as i32) as usize;
            let pixels = &mut buffer.line_mut(line as usize)[start..start + clip.width as usize];
            let data = &commands.data;
            call.draw(
                data,
                &mut commands.prepared,
                &mut DrawContext {
                    bounds,
                    data,
                    text,
                    gradients: &mut commands.gradients,
                    images,
                    source: None,
                    target: DrawTarget::Span {
                        x: clip.x,
                        y: line,
                        pixels,
                        coverage,
                    },
                },
            );
        }
        Payload::Clear => {
            let x = buffer.x_offset() as i32;
            let start = (clip.x - x) as usize;
            let end = start + clip.width as usize;
            buffer.line_mut(line as usize)[start..end].fill(B::Pixel::background());
        }
        Payload::Rectangle(rectangle) => {
            let x = buffer.x_offset() as i32;
            rectangle.draw_line(
                &commands.gradients,
                line,
                clip,
                coverage,
                PixelSpan {
                    x,
                    pixels: buffer.line_mut(line as usize),
                },
            );
        }
        Payload::SolidPair(pair) => {
            let x = buffer.x_offset() as i32;
            pair.draw_line(
                line,
                clip,
                PixelSpan {
                    x,
                    pixels: buffer.line_mut(line as usize),
                },
            );
        }
        Payload::Image(request) => {
            if let Some(image) = images.get(request.image.into()) {
                let covered;
                let request = if coverage == 255 {
                    request
                } else {
                    covered = {
                        let mut request = *request;
                        request.opacity = (request.opacity as u16 * coverage as u16 / 255) as u8;
                        request
                    };
                    &covered
                };
                let screen_x = buffer.x_offset() as i32;
                request.draw_line(
                    buffer.line_mut(line as usize),
                    &image.data,
                    &image.alpha_rows,
                    clip,
                    screen_x,
                    line,
                );
            }
        }
        Payload::Text(command, _) => {
            if line < bounds.y || line >= bounds.y + bounds.height {
                return;
            }
            let x = buffer.x_offset() as i32;
            text.draw_line(
                command,
                coverage,
                line,
                PixelSpan {
                    x,
                    pixels: buffer.line_mut(line as usize),
                },
                clip,
            );
        }
        Payload::Triangle(triangle) => {
            let x = buffer.x_offset() as i32;
            triangle.draw_line(
                line,
                clip,
                coverage,
                PixelSpan {
                    x,
                    pixels: buffer.line_mut(line as usize),
                },
            );
        }
    }
}
