use blit::{LogicalRect, Paint};
use blit_gui::image::{Image, ImageSampling};

use crate::{Instance, Pipeline, Renderer, image_axis, image_flag, physical_rect, premultiplied};

impl Paint<Image> for Renderer {
    fn paint(&mut self, area: LogicalRect, request: Image) {
        self.record_native(
            area,
            area,
            self.paint_clip,
            request,
            |request, command, _, renderer, bounds| {
                let clip = command.clip;

                let scale = renderer.scale;
                let start = renderer.instances.len() as u32;
                let opacity = ((request.opacity.clamp(0.0, 1.0) * 255.0).round() as u8) as f32 / 255.0;
                if opacity == 0.0 {
                    return;
                }
                let Some(image) = renderer.images.get(request.image.id().into()) else {
                    return;
                };
                let mask_color = image.mask_color.map(|color| premultiplied(color, 1.0));
                let colorize = request.colorize.map(|color| premultiplied(color, 1.0));
                request.for_each_patch(command.area, scale, |patch| {
                    let Some(draw) = patch.bounds.intersection(bounds) else {
                        return;
                    };
                    let offset_x = (i64::from(draw.x) - i64::from(patch.display.x)) as u64;
                    let offset_y = (i64::from(draw.y) - i64::from(patch.display.y)) as u64;
                    let (fixed_x, phase_x, step_x, bilinear_x, wrap_x) = image_axis(
                        patch.source.width,
                        patch.display.width,
                        patch.horizontal_tiling,
                        scale,
                        offset_x,
                    );
                    let (fixed_y, phase_y, step_y, bilinear_y, wrap_y) = image_axis(
                        patch.source.height,
                        patch.display.height,
                        patch.vertical_tiling,
                        scale,
                        offset_y,
                    );
                    let mut flags = (u32::from(wrap_x) * image_flag::WRAP_X) | (u32::from(wrap_y) * image_flag::WRAP_Y);
                    if request.sampling == ImageSampling::Bilinear {
                        flags |= image_flag::BILINEAR;
                    }
                    if mask_color.is_some() {
                        flags |= image_flag::MASK;
                    }
                    if request.colorize.is_some() {
                        flags |= image_flag::COLORIZE;
                    }
                    let (shape, params, axes, source_size) = if request.sampling == ImageSampling::Bilinear {
                        (
                            [0.0; 4],
                            [step_x, step_y, opacity, 0.0],
                            [bilinear_x.to_bits(), bilinear_y.to_bits()],
                            [patch.source.width as f32, patch.source.height as f32],
                        )
                    } else {
                        (
                            [
                                f32::from_bits((fixed_x >> 32) as u32),
                                f32::from_bits((fixed_y >> 32) as u32),
                                f32::from_bits((phase_x >> 32) as u32),
                                f32::from_bits((phase_y >> 32) as u32),
                            ],
                            [
                                f32::from_bits(phase_x as u32),
                                f32::from_bits(phase_y as u32),
                                opacity,
                                0.0,
                            ],
                            [fixed_x as u32, fixed_y as u32],
                            [f32::from_bits(patch.source.width), f32::from_bits(patch.source.height)],
                        )
                    };
                    renderer.instances.push(Instance {
                        shape,
                        draw: physical_rect(draw),
                        // source offsets are signed bit patterns consumed by the shader
                        radii: [
                            f32::from_bits(patch.source.x.wrapping_sub(image.texture_origin[0])),
                            f32::from_bits(patch.source.y.wrapping_sub(image.texture_origin[1])),
                            source_size[0],
                            source_size[1],
                        ],
                        inner_color: colorize.unwrap_or([0.0; 4]),
                        border_color: mask_color.unwrap_or([0.0; 4]),
                        params,
                        data: [clip, flags, axes[0], axes[1]],
                    });
                });

                renderer.push_instances(start, Pipeline::Image(request.image.id()));
            },
        );
    }
}
