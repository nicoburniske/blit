use blit::{
    DirtyRegions, LogicalRect, PhysicalRect, TextRequest,
    widgets::{Border, BorderRadius, BoxShadowRequest, ImageRequest, Rectangle},
};
use slotmap::KeyData;

use crate::{PixelBuffer, PixelSpan, RenderContext, RendererImageId, image, rectangle, shadow};

use super::{RenderStrategy, clip::ClipStack};

#[derive(Default)]
pub struct Direct {
    damage: DirtyRegions,
    clips: ClipStack,
}

impl Direct {
    fn draw_image_with_scale<B: PixelBuffer>(
        &mut self,
        context: &mut RenderContext<B>,
        request: &ImageRequest,
        clip: PhysicalRect,
        scale_factor: f32,
    ) {
        let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
        if let Some(image) = context.images.get(image) {
            if self.clips.is_active() {
                let screen = PhysicalRect {
                    x: context.buffer.x_offset() as i32,
                    y: 0,
                    width: context.buffer.width() as i32,
                    height: context.buffer.height() as i32,
                };
                let clip_id = self.clips.current();
                let texture = &image.data;
                let buffer = &mut context.buffer;
                for damage in self.damage.regions() {
                    let Some(clip) = clip.intersection(*damage) else {
                        continue;
                    };
                    image::prepare(request, texture, clip, scale_factor, |image, bounds| {
                        let Some(bounds) = bounds.intersection(screen) else {
                            return;
                        };
                        for line in bounds.y..bounds.y + bounds.height {
                            self.clips.for_each(
                                clip_id,
                                line,
                                bounds.x..bounds.x + bounds.width,
                                |range, coverage| {
                                    let mut image = image;
                                    if coverage != 255 {
                                        image.opacity =
                                            (image.opacity as u16 * coverage as u16 / 255) as u8;
                                    }
                                    let clip = PhysicalRect {
                                        x: range.start,
                                        y: line,
                                        width: range.end - range.start,
                                        height: 1,
                                    };
                                    image.draw(buffer, texture, clip);
                                },
                            );
                        }
                    });
                }
                return;
            }
            for damage in self.damage.regions() {
                if let Some(clip) = clip.intersection(*damage) {
                    image::draw(
                        &mut context.buffer,
                        request,
                        &image.data,
                        clip,
                        scale_factor,
                    );
                }
            }
        }
    }
}

impl<B: PixelBuffer> RenderStrategy<B> for Direct {
    fn begin_frame(&mut self, _: &mut RenderContext<B>, damage: &[PhysicalRect]) {
        for area in damage {
            self.damage.add(*area);
        }
    }

    fn push_rounded_clip(&mut self, area: LogicalRect, radius: BorderRadius, scale_factor: f32) {
        self.clips.push(area, radius, scale_factor)
    }

    fn pop_rounded_clip(&mut self) {
        self.clips.pop()
    }

    fn draw_rectangle(
        &mut self,
        context: &mut RenderContext<B>,
        rectangle: &Rectangle<'_>,
        clip: PhysicalRect,
    ) {
        if self.clips.is_active() {
            let screen = PhysicalRect {
                x: 0,
                y: 0,
                width: context.buffer.width() as i32,
                height: context.buffer.height() as i32,
            };
            let clip_id = self.clips.current();
            if let Border::Gradient { width, gradient } = rectangle.border
                && let Some(rectangle) =
                    rectangle::Gradient::new(rectangle, width, gradient, context.scale_factor)
            {
                for damage in self.damage.regions() {
                    let Some(bounds) = rectangle
                        .geometry
                        .intersection(clip)
                        .and_then(|area| area.intersection(*damage))
                        .and_then(|area| area.intersection(screen))
                    else {
                        continue;
                    };
                    for line in bounds.y..bounds.y + bounds.height {
                        self.clips.for_each(
                            clip_id,
                            line,
                            bounds.x..bounds.x + bounds.width,
                            |range, coverage| {
                                let clip = PhysicalRect {
                                    x: range.start,
                                    y: line,
                                    width: range.end - range.start,
                                    height: 1,
                                };
                                let row = PixelSpan {
                                    x: 0,
                                    pixels: context.buffer.line_mut(line as usize),
                                };
                                rectangle.draw_line(gradient.stops, line, clip, coverage, row);
                            },
                        );
                    }
                }
                return;
            }
            let Some(rectangle) = rectangle::Prepared::new(rectangle, context.scale_factor) else {
                return;
            };
            for damage in self.damage.regions() {
                let Some(bounds) = rectangle
                    .geometry
                    .intersection(clip)
                    .and_then(|area| area.intersection(*damage))
                    .and_then(|area| area.intersection(screen))
                else {
                    continue;
                };
                for line in bounds.y..bounds.y + bounds.height {
                    self.clips.for_each(
                        clip_id,
                        line,
                        bounds.x..bounds.x + bounds.width,
                        |range, coverage| {
                            let mut rectangle = rectangle;
                            if coverage != 255 {
                                rectangle.border_color =
                                    rectangle.border_color.coverage(coverage as u32);
                                rectangle.inner_color =
                                    rectangle.inner_color.coverage(coverage as u32);
                            }
                            let clip = PhysicalRect {
                                x: range.start,
                                y: line,
                                width: range.end - range.start,
                                height: 1,
                            };
                            let row = PixelSpan {
                                x: 0,
                                pixels: context.buffer.line_mut(line as usize),
                            };
                            rectangle.draw_line(line, clip, row);
                        },
                    );
                }
            }
            return;
        }
        for damage in self.damage.regions() {
            if let Some(clip) = clip.intersection(*damage) {
                rectangle::draw(&mut context.buffer, rectangle, clip, context.scale_factor);
            }
        }
    }

    fn draw_image(
        &mut self,
        context: &mut RenderContext<B>,
        request: &ImageRequest,
        clip: PhysicalRect,
    ) {
        self.draw_image_with_scale(context, request, clip, context.scale_factor)
    }

    fn draw_box_shadow(
        &mut self,
        context: &mut RenderContext<B>,
        request: &BoxShadowRequest,
        clip: PhysicalRect,
    ) {
        let Some(request) =
            context
                .shadows
                .prepare(&mut context.images, request, context.scale_factor)
        else {
            return;
        };
        match request {
            shadow::Prepared::Rectangle(rectangle) => {
                self.draw_rectangle(context, &rectangle, clip)
            }
            shadow::Prepared::Image(image) => {
                self.draw_image_with_scale(context, &image, clip, 1.0)
            }
        }
    }

    fn draw_text(
        &mut self,
        context: &mut RenderContext<B>,
        request: &TextRequest<'_>,
        clip: PhysicalRect,
    ) {
        let paragraph = context.text.prepare(request, context.scale_factor);
        let area = request.area.to_physical(context.scale_factor);
        let height = context.buffer.height() as i32;
        if self.clips.is_active() {
            let width = context.buffer.width() as i32;
            let clip_id = self.clips.current();
            for damage in self.damage.regions() {
                let Some(clip) = clip.intersection(*damage) else {
                    continue;
                };
                let start = clip.y.max(0);
                let end = clip.y.saturating_add(clip.height).min(height);
                for line in start..end {
                    self.clips.for_each(
                        clip_id,
                        line,
                        clip.x.max(0)..clip.x.saturating_add(clip.width).min(width),
                        |range, coverage| {
                            let mut color = request.color;
                            if coverage != 255 {
                                color.alpha = (color.alpha as u16 * coverage as u16 / 255) as u8;
                            }
                            let row = PixelSpan {
                                x: 0,
                                pixels: context.buffer.line_mut(line as usize),
                            };
                            let clip = PhysicalRect {
                                x: range.start,
                                y: line,
                                width: range.end - range.start,
                                height: 1,
                            };
                            context
                                .text
                                .draw_line(paragraph, area, color, line, row, clip);
                        },
                    );
                }
            }
            return;
        }
        for damage in self.damage.regions() {
            let Some(clip) = clip.intersection(*damage) else {
                continue;
            };
            let start = clip.y.max(0);
            let end = clip.y.saturating_add(clip.height).min(height);
            for line in start..end {
                context.text.draw_line(
                    paragraph,
                    area,
                    request.color,
                    line,
                    PixelSpan {
                        x: 0,
                        pixels: context.buffer.line_mut(line as usize),
                    },
                    clip,
                );
            }
        }
    }

    fn end_frame(&mut self, _: &mut RenderContext<B>) {
        self.damage = DirtyRegions::default();
        self.clips.clear();
    }
}
