use blit::geometry::PhysicalRect;
use blit_arena::Arena;
use blit_gui::display_list::DisplayList;

use super::{
    clip::ClipStack,
    command::{CommandId, Payload, RasterCommands},
};
use crate::{Pixel, PixelSpan, RenderContext, StoredImage, TextRenderer, render::RenderTarget};

#[inline(always)]
pub fn draw_line<P: Pixel>(
    commands: &mut RasterCommands<P>,
    list: &DisplayList,
    clips: &ClipStack,
    command: CommandId,
    line: i32,
    span: PhysicalRect,
    coverage: u8,
    images: &mut Arena<StoredImage>,
    text: &mut TextRenderer,
    row: PixelSpan<'_, P>,
) {
    if commands.entries[command as usize].is_pending() {
        commands.prepare(command, list, clips, images, text);
    }
    let bounds = commands.bounds(command);
    let clip = commands.clip(command);
    match commands.get(command) {
        Payload::Pending | Payload::Empty => {}
        Payload::Custom => {
            let call = &mut commands.entries[command as usize];
            if call.reads.is_some() {
                return;
            }
            let start = (span.x - row.x) as usize;
            let pixels = &mut row.pixels[start..start + span.width as usize];
            call.render(
                &mut commands.prepared,
                &mut RenderContext {
                    scale: commands.scale,
                    area: list.commands()[commands.metadata[command as usize].source as usize].area,
                    bounds,
                    clips,
                    clip,
                    screen: commands.screen,
                    damage_bounds: commands.damage_bounds,
                    list,
                    text,
                    gradients: &mut commands.gradients,
                    images,
                    source: None,
                    scratch: &commands.scratch,
                    target: RenderTarget::Span {
                        x: span.x,
                        y: line,
                        pixels,
                        coverage,
                    },
                },
            );
        }
        Payload::Clear => {
            let start = (span.x - row.x) as usize;
            let end = start + span.width as usize;
            row.pixels[start..end].fill(P::background());
        }
        Payload::Rectangle(rectangle) => {
            rectangle.draw_line(&commands.gradients, line, span, coverage, row);
        }
        Payload::SolidPair(pair) => {
            pair.draw_line(line, span, row);
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
                request.draw_line(row.pixels, &image.data, &image.alpha_rows, span, row.x, line);
            }
        }
        Payload::Text(command, _) => {
            if line < bounds.y || line >= bounds.y + bounds.height {
                return;
            }
            text.draw_line(command, coverage, line, row, span);
        }
        Payload::Mesh(index) => {
            let index = *index as usize;
            commands.meshes[index].draw_line(list, commands.scale, line, span, coverage, row);
        }
    }
}
