use blit::{LogicalRect, Paint};
use blit_gui::damage::DamageTracker;

use crate::{
    PixelBuffer, RenderStrategy, Renderer,
    render::ErasedCommand,
    strategy::command::{Coverage, Payload},
};

impl<B, S, D> Paint<blit_gui::clear::Clear> for Renderer<B, S, D>
where
    B: PixelBuffer,
    S: RenderStrategy<B>,
    D: DamageTracker,
{
    fn paint(&mut self, bounds: LogicalRect, _: blit_gui::clear::Clear) {
        let mut recording = self.record(bounds);
        recording.record_input(recording.bounds, 0, (), |_, _, record, _, context| {
            let target = &mut *context.commands;
            let call = ErasedCommand::ready(&mut target.prepared, Payload::Clear);
            target.push(call, record.bounds, 0).coverage = Coverage::Full;
        });
    }
}
