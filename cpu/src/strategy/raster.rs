use blit::geometry::PhysicalRect;
use slotmap::{KeyData, SlotMap};

use super::command::Payload;
use crate::{Pixel, PixelBuffer, PixelSpan, RendererImageId, StoredImage, TextRenderer};

#[inline(always)]
pub fn draw_line<B: PixelBuffer>(
    payload: &Payload<'_>,
    line: i32,
    clip: PhysicalRect,
    coverage: u8,
    images: &SlotMap<RendererImageId, StoredImage>,
    text: &mut TextRenderer,
    buffer: &mut B,
) {
    match payload {
        Payload::Clear => {
            let x = buffer.x_offset() as i32;
            let start = (clip.x - x) as usize;
            let end = start + clip.width as usize;
            buffer.line_mut(line as usize)[start..end].fill(B::Pixel::background());
        }
        Payload::Rectangle(rectangle) => {
            let covered;
            let rectangle = if coverage == 255 {
                *rectangle
            } else {
                covered = {
                    let mut rectangle = **rectangle;
                    rectangle.border_color = rectangle.border_color.coverage(coverage as u32);
                    rectangle.inner_color = rectangle.inner_color.coverage(coverage as u32);
                    rectangle
                };
                &covered
            };
            let x = buffer.x_offset() as i32;
            rectangle.draw_line(
                line,
                clip,
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
        Payload::GradientRectangle(rectangle, stops) => {
            let x = buffer.x_offset() as i32;
            rectangle.draw_line(
                stops,
                line,
                clip,
                coverage,
                PixelSpan {
                    x,
                    pixels: buffer.line_mut(line as usize),
                },
            );
        }
        Payload::Image(request) => {
            let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
            if let Some(image) = images.get(image) {
                let covered;
                let request = if coverage == 255 {
                    *request
                } else {
                    covered = {
                        let mut request = **request;
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
        Payload::Text(command) => {
            let mut color = command.color;
            if coverage != 255 {
                color.alpha = (color.alpha as u16 * coverage as u16 / 255) as u8;
            }
            let x = buffer.x_offset() as i32;
            text.draw_line(
                command.glyph_start,
                command.glyph_end,
                command.lines,
                command.area,
                color,
                line,
                PixelSpan {
                    x,
                    pixels: buffer.line_mut(line as usize),
                },
                clip,
            );
        }
    }
}
