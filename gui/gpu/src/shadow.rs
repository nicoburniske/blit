use std::hash::Hash;

use blit::{LogicalRect, Paint, PhysicalRect, Scale2};
use blit_gui::{Shadow, display_list::DisplayList, style::BorderRadius};
use bytemuck::Zeroable;

use crate::{
    INSET_SHADOW, Instance, OUTSET_SHADOW, Pipeline, Renderer, handler::hash_area, physical_radii, physical_rect,
    premultiplied,
};

impl Paint<Shadow> for Renderer {
    fn paint(&mut self, area: LogicalRect, shadow: Shadow) {
        self.record_native(
            shadow.bounds(area),
            self.paint_clip,
            StoredShadow { area, shadow },
            |value, _, renderer, bounds, clip| {
                let shadow = value.shadow;

                let scale = renderer.scale;
                let start = renderer.instances.len() as u32;
                if shadow.color.alpha == 0 {
                    return;
                }
                if shadow.inset {
                    let shape = value.area.to_physical(Scale2::uniform(scale));
                    if shape.width <= 0 || shape.height <= 0 {
                        return;
                    }
                    let Some(draw) = shape.intersection(bounds) else {
                        return;
                    };
                    let blur = (shadow.blur.max(0.0) * scale).ceil() as i32;
                    let offset_x = (shadow.offset_x * scale).round() as i32;
                    let offset_y = (shadow.offset_y * scale).round() as i32;
                    let spread = (shadow.spread * scale).round() as i32;
                    renderer.instances.push(Instance {
                        shape: physical_rect(shape),
                        draw: physical_rect(draw),
                        radii: physical_radii(shadow.radius, scale, shape.width, shape.height),
                        inner_color: premultiplied(shadow.color, 1.0),
                        params: [blur as f32, offset_x as f32, offset_y as f32, spread as f32],
                        data: [clip, 0, 0, INSET_SHADOW],
                        ..Instance::zeroed()
                    });
                } else {
                    let area = LogicalRect {
                        x: value.area.x + shadow.offset_x - shadow.spread,
                        y: value.area.y + shadow.offset_y - shadow.spread,
                        width: value.area.width + shadow.spread * 2.0,
                        height: value.area.height + shadow.spread * 2.0,
                    };
                    if area.width <= 0.0 || area.height <= 0.0 {
                        return;
                    }
                    let shape = area.to_physical(Scale2::uniform(scale));
                    if shape.width <= 0 || shape.height <= 0 {
                        return;
                    }
                    let blur = (shadow.blur.max(0.0) * scale).ceil() as i32;
                    let support = if blur == 0 {
                        shape
                    } else {
                        let Some(diameter) = blur.checked_mul(2) else {
                            return;
                        };
                        let (Some(width), Some(height)) =
                            (shape.width.checked_add(diameter), shape.height.checked_add(diameter))
                        else {
                            return;
                        };
                        PhysicalRect {
                            x: shape.x.saturating_sub(blur),
                            y: shape.y.saturating_sub(blur),
                            width,
                            height,
                        }
                    };
                    let Some(draw) = support.intersection(bounds) else {
                        return;
                    };
                    let radius = BorderRadius {
                        top_left: (shadow.radius.top_left + shadow.spread).max(0.0),
                        top_right: (shadow.radius.top_right + shadow.spread).max(0.0),
                        bottom_right: (shadow.radius.bottom_right + shadow.spread).max(0.0),
                        bottom_left: (shadow.radius.bottom_left + shadow.spread).max(0.0),
                    };
                    renderer.instances.push(Instance {
                        shape: physical_rect(shape),
                        draw: physical_rect(draw),
                        radii: physical_radii(radius, scale, shape.width, shape.height),
                        inner_color: premultiplied(shadow.color, 1.0),
                        params: [blur as f32, 0.0, 0.0, 0.0],
                        data: [clip, 0, 0, if blur == 0 { 0 } else { OUTSET_SHADOW }],
                        ..Instance::zeroed()
                    });
                }

                renderer.push_instances(start, Pipeline::Content(None));
            },
        );
    }
}

struct StoredShadow {
    area: LogicalRect,
    shadow: Shadow,
}

impl blit_gui::display_list::Compare for StoredShadow {
    fn equal(a: &Self, _: &DisplayList, b: &Self, _: &DisplayList) -> bool {
        a.area == b.area && a.shadow == b.shadow
    }
    fn hash(value: &Self, _: &DisplayList, state: &mut impl std::hash::Hasher) {
        hash_area(value.area, state);
        value.shadow.hash(state);
    }
}
