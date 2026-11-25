use blit::{LogicalRect, Paint};
use blit_gui::clear::Clear;
use bytemuck::Zeroable;

use crate::{DrawContext, Instance, Pipeline, Renderer, physical_rect};

impl Paint<Clear> for Renderer {
    fn paint(&mut self, bounds: LogicalRect, value: &Clear) {
        let mut record = self.record(bounds);
        record.clip = 0;
        record.push(*value, draw);
    }
}

fn draw(_: &Clear, cx: &mut DrawContext<'_>) {
    let renderer = &mut *cx.renderer;
    let start = renderer.instances.len() as u32;
    renderer.instances.push(Instance {
        draw: physical_rect(cx.bounds),
        inner_color: [
            renderer.clear_color.r as f32,
            renderer.clear_color.g as f32,
            renderer.clear_color.b as f32,
            renderer.clear_color.a as f32,
        ],
        ..Instance::zeroed()
    });

    renderer.push_instances(start, Pipeline::Clear);
}
