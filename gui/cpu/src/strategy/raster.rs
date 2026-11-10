use blit::geometry::PhysicalRect;
use blit_arena::Arena;
use blit_gui::display_list::DisplayList;

use super::command::{CommandId, Payload, RasterCommands};
use crate::{DrawContext, Pixel, PixelSpan, StoredImage, TextRenderer, raster::DrawTarget};

#[inline(always)]
pub fn draw_line<P: Pixel>(
    commands: &mut RasterCommands<P>,
    list: &DisplayList,
    command: CommandId,
    line: i32,
    clip: PhysicalRect,
    coverage: u8,
    images: &mut Arena<StoredImage>,
    text: &mut TextRenderer,
    row: PixelSpan<'_, P>,
) {
    if commands.entries[command as usize].is_pending() {
        commands.prepare(command, list, images, text);
    }
    let bounds = commands.bounds(command);
    match commands.get(command) {
        Payload::Pending | Payload::Empty => {}
        Payload::Custom => {
            let call = &mut commands.entries[command as usize];
            if call.reads.is_some() {
                return;
            }
            let start = (clip.x - row.x) as usize;
            let pixels = &mut row.pixels[start..start + clip.width as usize];
            call.draw(
                &mut commands.prepared,
                &mut DrawContext {
                    scale: commands.scale,
                    bounds,
                    list,
                    text,
                    gradients: &mut commands.gradients,
                    images,
                    source: None,
                    scratch: &commands.scratch,
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
            let start = (clip.x - row.x) as usize;
            let end = start + clip.width as usize;
            row.pixels[start..end].fill(P::background());
        }
        Payload::Rectangle(rectangle) => {
            rectangle.draw_line(&commands.gradients, line, clip, coverage, row);
        }
        Payload::SolidPair(pair) => {
            pair.draw_line(line, clip, row);
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
                request.draw_line(row.pixels, &image.data, &image.alpha_rows, clip, row.x, line);
            }
        }
        Payload::Text(command, _) => {
            if line < bounds.y || line >= bounds.y + bounds.height {
                return;
            }
            text.draw_line(command, coverage, line, row, clip);
        }
        Payload::Mesh(index) => {
            let index = *index as usize;
            commands.meshes[index].draw_line(list, commands.scale, line, clip, coverage, row);
        }
    }
}
