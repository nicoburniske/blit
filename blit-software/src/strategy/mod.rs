mod clip;
mod command;
mod direct;
mod line;

pub use direct::Direct;
pub use line::Scanline;

use blit::{
    LogicalRect, PhysicalRect, TextRequest,
    widgets::{BorderRadius, BoxShadowRequest, ImageRequest, Rectangle},
};

use crate::{PixelBuffer, RenderContext};

pub trait RenderStrategy<B: PixelBuffer> {
    fn begin_frame(&mut self, context: &mut RenderContext<B>, damage: &[PhysicalRect]);
    fn add_damage(&mut self, area: PhysicalRect);
    fn push_rounded_clip(&mut self, area: LogicalRect, radius: BorderRadius, scale_factor: f32);
    fn pop_rounded_clip(&mut self);
    fn draw_rectangle(
        &mut self,
        context: &mut RenderContext<B>,
        rectangle: &Rectangle<'_>,
        clip: PhysicalRect,
    );
    fn draw_box_shadow(
        &mut self,
        context: &mut RenderContext<B>,
        shadow: &BoxShadowRequest,
        clip: PhysicalRect,
    );
    fn draw_image(
        &mut self,
        context: &mut RenderContext<B>,
        image: &ImageRequest,
        clip: PhysicalRect,
    );
    fn draw_text(
        &mut self,
        context: &mut RenderContext<B>,
        text: &TextRequest<'_>,
        clip: PhysicalRect,
    );
    fn end_frame(&mut self, context: &mut RenderContext<B>);
}
