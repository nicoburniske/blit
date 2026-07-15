use blit::PhysicalRect;
use slotmap::{KeyData, SlotMap};

use crate::{PixelBuffer, PixelSpan, RendererImageId, StoredImage, TextRenderer};

use super::command::Payload;

#[inline(always)]
pub fn draw_line<B: PixelBuffer>(
    payload: &Payload<'_>,
    line: i32,
    clip: PhysicalRect,
    coverage: u8,
    images: &SlotMap<RendererImageId, StoredImage>,
    text: &TextRenderer,
    buffer: &mut B,
) {
    match payload {
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
                request.draw(buffer, &image.data, clip);
            }
        }
        Payload::Text(command) => {
            let mut color = command.color;
            if coverage != 255 {
                color.alpha = (color.alpha as u16 * coverage as u16 / 255) as u8;
            }
            let x = buffer.x_offset() as i32;
            text.draw_line(
                command.paragraph,
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
