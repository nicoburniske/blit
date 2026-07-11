mod command;
mod direct;
mod line;

pub use direct::Direct;
pub use line::Scanline;

use blit::{
    PhysicalRect, TextRequest,
    widgets::{ImageRequest, Rectangle},
};

use crate::{PixelBuffer, RenderContext};

pub trait RenderStrategy<B: PixelBuffer> {
    fn begin_frame(&mut self, context: &mut RenderContext<B>);
    fn draw_rectangle(
        &mut self,
        context: &mut RenderContext<B>,
        rectangle: &Rectangle,
        clips: &[PhysicalRect],
    );
    fn draw_image(
        &mut self,
        context: &mut RenderContext<B>,
        image: &ImageRequest,
        clips: &[PhysicalRect],
    );
    fn draw_text(
        &mut self,
        context: &mut RenderContext<B>,
        text: &TextRequest<'_>,
        clips: &[PhysicalRect],
    );
    fn end_frame(&mut self, context: &mut RenderContext<B>);
}
