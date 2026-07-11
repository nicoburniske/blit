use bullseye::{
    PhysicalRect, TextRequest,
    widgets::{ImageRequest, Rectangle},
};
use slotmap::KeyData;

use crate::{PixelBuffer, RenderContext, RendererImageId, image, rectangle};

use super::RenderStrategy;

pub struct Direct;

impl<B: PixelBuffer> RenderStrategy<B> for Direct {
    fn begin_frame(&mut self, _: &mut RenderContext<B>) {}

    fn draw_rectangle(
        &mut self,
        context: &mut RenderContext<B>,
        rectangle: &Rectangle,
        clips: &[PhysicalRect],
    ) {
        rectangle::draw(&mut context.buffer, rectangle, clips, context.scale_factor);
    }

    fn draw_image(
        &mut self,
        context: &mut RenderContext<B>,
        request: &ImageRequest,
        clips: &[PhysicalRect],
    ) {
        let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
        if let Some(image) = context.images.get(image) {
            image::draw(
                &mut context.buffer,
                request,
                &image.data,
                clips,
                context.scale_factor,
            );
        }
    }

    fn draw_text(
        &mut self,
        context: &mut RenderContext<B>,
        request: &TextRequest<'_>,
        clips: &[PhysicalRect],
    ) {
        let paragraph = context.text.prepare(request, context.scale_factor);
        let area = request.area.to_physical(context.scale_factor);
        let height = context.buffer.height() as i32;
        for clip in clips {
            let start = clip.y.max(0);
            let end = clip.y.saturating_add(clip.height).min(height);
            for line in start..end {
                context.text.draw_line(
                    paragraph,
                    area,
                    request.color,
                    line,
                    context.buffer.line_mut(line as usize),
                    std::slice::from_ref(clip),
                );
            }
        }
    }

    fn end_frame(&mut self, _: &mut RenderContext<B>) {}
}
