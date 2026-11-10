use blit::{LogicalRect, Paint, Scale2};
use blit_gui::{
    Rectangle,
    display_list::Compare,
    style::{BorderRadius, Fill},
};

use crate::{GradientStop, Instance, MeshData, Pipeline, Renderer, physical_radii, physical_rect, premultiplied};

impl Paint<Rectangle<'_>> for Renderer {
    fn paint(&mut self, area: LogicalRect, rectangle: Rectangle<'_>) {
        let value = blit_gui::rectangle::Stored::new(area, rectangle, &mut self.list);
        self.record_native(
            area,
            self.paint_clip,
            value,
            blit_gui::rectangle::Stored::equal,
            blit_gui::rectangle::Stored::hash,
            |value, list, renderer, bounds, clip| {
                let rectangle = value.view(list);

                let scale = renderer.scale;
                let start = renderer.instances.len() as u32;
                let opacity = ((rectangle.opacity.clamp(0.0, 1.0) * 255.0).round() as u8) as f32 / 255.0;
                if opacity == 0.0 {
                    return;
                }
                let area = value.area.to_physical(Scale2::uniform(scale));
                let Some(draw) = area.intersection(bounds) else {
                    return;
                };
                if let Fill::Solid(background) = rectangle.background
                    && opacity == 1.0
                    && background.alpha > 0
                    && rectangle.radius == BorderRadius::default()
                    && rectangle.border.width <= 0.0
                {
                    assert!(
                        renderer.mesh_data.len() <= u32::MAX as usize - 3,
                        "too many GPU mesh vertices"
                    );
                    let vertex = renderer.mesh_data.len() as u32;
                    let color =
                        u32::from_le_bytes([background.red, background.green, background.blue, background.alpha]);
                    let left = draw.x as f32;
                    let top = draw.y as f32;
                    let right = draw.x.saturating_add(draw.width) as f32;
                    let bottom = draw.y.saturating_add(draw.height) as f32;
                    renderer.mesh_data.extend([
                        MeshData([left.to_bits(), top.to_bits(), color, clip]),
                        MeshData([right.to_bits(), top.to_bits(), color, clip]),
                        MeshData([left.to_bits(), bottom.to_bits(), color, clip]),
                        MeshData([right.to_bits(), bottom.to_bits(), color, clip]),
                    ]);
                    renderer.push_mesh_primitive([vertex, vertex + 1, vertex + 2, vertex + 3]);
                    return;
                }
                let mut colors = [[0.0; 4]; 2];
                let mut border_width = (rectangle.border.width * scale).round().max(0.0) as i32 as f32;
                let mut data = [clip, 0, 0, 0];
                for (index, fill) in [rectangle.background, rectangle.border.fill].into_iter().enumerate() {
                    if index == 1 && border_width == 0.0 {
                        continue;
                    }
                    match fill {
                        Fill::Solid(color) => colors[index] = premultiplied(color, opacity),
                        Fill::LinearGradient(gradient) => {
                            gradient.validate();
                            let (direction_y, direction_x) = gradient.angle_degrees.to_radians().sin_cos();
                            let extent = direction_x.abs() * area.width as f32 + direction_y.abs() * area.height as f32;
                            let end = u32::try_from(renderer.stops.len() + gradient.stops.len())
                                .expect("too many GPU gradient stops");
                            data[index + 1] = renderer.stops.len() as u32 + 1;
                            renderer.stops.extend(gradient.stops.iter().map(|stop| GradientStop {
                                color: premultiplied(stop.color, opacity),
                                position: stop.position,
                                x_step: direction_x / extent,
                                y_step: direction_y / extent,
                                end,
                            }));
                        }
                    }
                }
                if data[2] == 0 && colors[1][3] == 0.0 {
                    border_width = 0.0;
                }
                if data[1] == 0 && colors[0][3] == 0.0 && border_width == 0.0 {
                    return;
                }
                renderer.instances.push(Instance {
                    shape: physical_rect(area),
                    draw: physical_rect(draw),
                    radii: physical_radii(rectangle.radius, scale, area.width, area.height),
                    inner_color: colors[0],
                    border_color: colors[1],
                    params: [border_width, 0.0, 0.0, 0.0],
                    data,
                });

                renderer.push_instances(start, Pipeline::Content(None));
            },
        );
    }
}
