use blit::{
    DirtyRegions, PhysicalRect, TextRequest,
    widgets::{ImageRequest, Rectangle},
};
use slotmap::KeyData;

use crate::{PixelBuffer, PixelSpan, RenderContext, RendererImageId, image, rectangle};

use super::RenderStrategy;

#[derive(Default)]
pub struct Direct {
    damage: DirtyRegions,
}

impl<B: PixelBuffer> RenderStrategy<B> for Direct {
    fn begin_frame(&mut self, _: &mut RenderContext<B>, damage: &[PhysicalRect]) {
        for area in damage {
            self.damage.add(*area);
        }
    }

    fn draw_rectangle(
        &mut self,
        context: &mut RenderContext<B>,
        rectangle: &Rectangle,
        clip: PhysicalRect,
    ) {
        for damage in self.damage.regions() {
            if let Some(clip) = clip.intersection(*damage) {
                rectangle::draw(&mut context.buffer, rectangle, clip, context.scale_factor);
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
        if let Some(image) = context.images.get(image) {
            for damage in self.damage.regions() {
                if let Some(clip) = clip.intersection(*damage) {
                    image::draw(
                        &mut context.buffer,
                        request,
                        &image.data,
                        clip,
                        context.scale_factor,
                    );
                }
            }
        }
    }

    fn draw_text(
        &mut self,
        context: &mut RenderContext<B>,
        request: &TextRequest<'_>,
        clip: PhysicalRect,
    ) {
        let paragraph = context.text.prepare(request, context.scale_factor);
        let area = request.area.to_physical(context.scale_factor);
        let height = context.buffer.height() as i32;
        for damage in self.damage.regions() {
            let Some(clip) = clip.intersection(*damage) else {
                continue;
            };
            let start = clip.y.max(0);
            let end = clip.y.saturating_add(clip.height).min(height);
            for line in start..end {
                context.text.draw_line(
                    paragraph,
                    area,
                    request.color,
                    line,
                    PixelSpan {
                        x: 0,
                        pixels: context.buffer.line_mut(line as usize),
                    },
                    clip,
                );
            }
        }
    }

    fn end_frame(&mut self, _: &mut RenderContext<B>) {
        self.damage = DirtyRegions::default();
    }
}
