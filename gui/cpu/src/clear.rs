use blit::{LogicalRect, Paint};
use blit_gui::{clear::Clear, damage::DamageTracker};

use crate::{
    Pixel, PixelBuffer, RenderStrategy, Renderer,
    render::{DrawContext, ErasedCommand},
    strategy::command::{Coverage, Payload},
};

impl<B, S, D> Paint<Clear> for Renderer<B, S, D>
where
    B: PixelBuffer,
    S: RenderStrategy<B>,
    D: DamageTracker,
{
    fn paint(&mut self, area: LogicalRect, clear: &Clear) {
        let mut record = self.record(area);
        record.clip = 0;
        record.push(*clear, draw_clear::<B::Pixel>);
    }
}

fn draw_clear<P: Pixel>(_: &Clear, cx: &mut DrawContext<'_, P>) {
    let target = &mut *cx.commands;
    let call = ErasedCommand::ready(&mut target.prepared, Payload::Clear);
    target.push(call, cx.bounds, 0).coverage = Coverage::Full;
}
