use blit::{LogicalRect, Paint};
use blit_gui::clear::Clear;
use bytemuck::Zeroable;

use crate::{Instance, Pipeline, Renderer, physical_rect};

impl Paint<Clear> for Renderer {
    fn paint(&mut self, bounds: LogicalRect, _: Clear) {
        self.record_native(bounds, 0, (), |_, _, renderer, bounds, _| {
            let start = renderer.instances.len() as u32;
            let draw = bounds;
            renderer.instances.push(Instance {
                draw: physical_rect(draw),
                inner_color: [
                    renderer.clear_color.r as f32,
                    renderer.clear_color.g as f32,
                    renderer.clear_color.b as f32,
                    renderer.clear_color.a as f32,
                ],
                ..Instance::zeroed()
            });

            renderer.push_instances(start, Pipeline::Clear);
        });
    }
}
