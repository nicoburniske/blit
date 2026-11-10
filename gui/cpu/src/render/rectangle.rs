use std::ops::Range;

use blit::{LogicalRect, Paint, PhysicalRect, Scale2, internal::bump::Offset};
use blit_gui::{
    Rectangle,
    damage::DamageTracker,
    style::{ConicGradient, Fill, LinearGradient},
};

use super::{
    fill::{Gradients, PreparedFill},
    rounded::{Radii, RoundedLine},
};
use crate::{
    Pixel, PixelBuffer, PixelSpan, PremultipliedRgbaColor, RenderStrategy, Renderer,
    strategy::command::{Coverage, Payload},
};

impl<B, S, D> Paint<Rectangle<'_>> for Renderer<B, S, D>
where
    B: PixelBuffer,
    S: RenderStrategy<B>,
    D: DamageTracker,
{
    fn paint(&mut self, area: LogicalRect, rectangle: Rectangle<'_>) {
        let mut recording = self.record::<()>(area);
        let value = blit_gui::rectangle::Stored::new(area, rectangle, recording.list);
        recording.record_input(
            recording.bounds,
            recording.clip,
            value,
            |input, offset, record, list, context| {
                let scale = context.commands.scale;
                let target = &mut *context.commands;
                let rectangle = input.view(list);
                let Some(bounds) = input
                    .area
                    .to_physical(Scale2::uniform(scale))
                    .intersection(record.bounds)
                else {
                    return;
                };
                if rectangle.opacity <= 0.0 || rectangle.opacity.is_nan() {
                    return;
                }
                let opaque = match rectangle.background {
                    Fill::Solid(color) => PremultipliedRgbaColor::with_opacity(color, rectangle.opacity).alpha == 255,
                    Fill::LinearGradient(LinearGradient { stops, .. })
                    | Fill::ConicGradient(ConicGradient { stops, .. }) => {
                        !stops.is_empty()
                            && stops.iter().all(|stop| {
                                PremultipliedRgbaColor::with_opacity(stop.color, rectangle.opacity).alpha == 255
                            })
                    }
                };
                let coverage = if !opaque {
                    Coverage::None
                } else if rectangle.radius == Default::default() {
                    Coverage::Full
                } else {
                    Coverage::Rounded
                };
                if record.clip == 0
                    && coverage == Coverage::Full
                    && let Some(previous) = target.metadata.last()
                    && previous.clip == 0
                    && previous.bounds == bounds
                {
                    target.metadata.pop();
                    target.entries.pop();
                }
                target
                    .record(bounds, record.clip, offset, |_, offset, prepared, cx| {
                        // safety: recording stores this offset as blit_gui::rectangle::Stored
                        let input = unsafe { cx.list.data().get(Offset::<blit_gui::rectangle::Stored>::new(offset)) };
                        let value = crate::render::rectangle::Prepared::new(
                            input.area,
                            &input.view(cx.list),
                            cx.scale,
                            cx.gradients,
                        )
                        .map(Payload::Rectangle)
                        .unwrap_or(Payload::Empty);
                        prepared.push(value).byte()
                    })
                    .coverage = coverage;
            },
        );
    }
}

#[derive(Clone, Copy)]
pub struct Prepared {
    pub geometry: PhysicalRect,
    pub radii: Radii,
    inner: PhysicalRect,
    pub background: PreparedFill,
    border: PreparedFill,
    pub border_width: i32,
}

impl Prepared {
    #[inline]
    pub fn new(area: LogicalRect, rectangle: &Rectangle<'_>, scale: f32, gradients: &mut Gradients) -> Option<Self> {
        let geometry = area.to_physical(Scale2::uniform(scale));
        if geometry.width <= 0 || geometry.height <= 0 || rectangle.opacity <= 0.0 {
            return None;
        }
        let transparent = PreparedFill::Solid(PremultipliedRgbaColor::default());
        let background = PreparedFill::new(
            rectangle.background,
            geometry,
            rectangle.opacity,
            PremultipliedRgbaColor::default(),
            gradients,
        );
        let mut border_width = (rectangle.border.width * scale).round().max(0.0) as i32;
        let border = if border_width > 0 {
            let background = if let PreparedFill::Solid(color) = background {
                color
            } else {
                PremultipliedRgbaColor::default()
            };
            PreparedFill::new(
                rectangle.border.fill,
                geometry,
                rectangle.opacity,
                background,
                gradients,
            )
        } else {
            transparent
        };
        if matches!(border, PreparedFill::Solid(color) if color.alpha == 0) {
            border_width = 0;
        }
        if border_width == 0 && matches!(background, PreparedFill::Solid(color) if color.alpha == 0) {
            return None;
        }
        let radii = Radii::new(rectangle.radius, scale, geometry.width, geometry.height);
        Some(Self {
            geometry,
            radii,
            background,
            border,
            border_width,
            inner: PhysicalRect {
                x: geometry.x.saturating_add(border_width),
                y: geometry.y.saturating_add(border_width),
                width: (geometry.width - border_width.saturating_mul(2)).max(0),
                height: (geometry.height - border_width.saturating_mul(2)).max(0),
            },
        })
    }

    pub fn opaque_span(&self, line: i32) -> Option<Range<i32>> {
        if line < self.geometry.y || line >= self.geometry.y + self.geometry.height {
            return None;
        }
        let top = line - self.geometry.y;
        let bottom = self.geometry.height - top - 1;
        let inset = |top_radius: i32, bottom_radius: i32| {
            let (radius, y) = if top < top_radius {
                (top_radius as f32, top as f32)
            } else if bottom < bottom_radius {
                (bottom_radius as f32, bottom as f32)
            } else {
                return 0;
            };
            (radius - (radius * radius - (radius - y).powi(2)).max(0.0).sqrt()).ceil() as i32
        };
        let start = self.geometry.x + inset(self.radii.top_left, self.radii.bottom_left);
        let end = self.geometry.x + self.geometry.width - inset(self.radii.top_right, self.radii.bottom_right);
        (start < end).then_some(start..end)
    }

    pub fn fuse(&self, second: &Self) -> Option<SolidPair> {
        let (PreparedFill::Solid(first), PreparedFill::Solid(second_color)) = (self.background, second.background)
        else {
            return None;
        };
        (self.geometry == second.geometry
            && first.alpha == 255
            && second_color.alpha < 255
            && self.radii.is_zero()
            && second.radii.is_zero()
            && self.border_width == 0
            && second.border_width == 0)
            .then_some(SolidPair {
                geometry: self.geometry,
                first,
                second: second_color,
            })
    }

    pub fn draw_line<P: Pixel>(
        &self,
        gradients: &Gradients,
        line: i32,
        clip: PhysicalRect,
        coverage: u8,
        row: PixelSpan<'_, P>,
    ) {
        let start = clip.x.max(row.x).max(self.geometry.x);
        let end = (clip.x + clip.width)
            .min(row.x + row.pixels.len() as i32)
            .min(self.geometry.x + self.geometry.width);
        if start >= end || line < self.geometry.y || line >= self.geometry.y + self.geometry.height {
            return;
        }
        let draw_border = |x: i32, pixels: &mut [P]| {
            if matches!(self.background, PreparedFill::Solid(_)) {
                self.border.draw_span(gradients, x, line, coverage, pixels);
            } else {
                for (index, pixel) in pixels.iter_mut().enumerate() {
                    let x = x + index as i32;
                    pixel.blend(
                        self.border
                            .sample(gradients, x, line)
                            .over(self.background.sample(gradients, x, line))
                            .coverage(coverage as u32),
                    );
                }
            }
        };
        let top = line - self.geometry.y;
        let bottom = self.geometry.height - top - 1;
        if self.radii.is_zero()
            || (top >= self.radii.top_left.max(self.radii.top_right)
                && bottom >= self.radii.bottom_left.max(self.radii.bottom_right))
        {
            let pixels = &mut row.pixels[(start - row.x) as usize..(end - row.x) as usize];
            if self.border_width == 0 {
                self.background.draw_span(gradients, start, line, coverage, pixels);
            } else if line < self.inner.y || line >= self.inner.y + self.inner.height {
                draw_border(start, pixels);
            } else {
                let left = (self.inner.x - start).clamp(0, end - start) as usize;
                let right = (self.inner.x + self.inner.width - start).clamp(left as i32, end - start) as usize;
                draw_border(start, &mut pixels[..left]);
                self.background
                    .draw_span(gradients, start + left as i32, line, coverage, &mut pixels[left..right]);
                draw_border(start + right as i32, &mut pixels[right..]);
            }
            return;
        }
        let Some(outer) = RoundedLine::new(self.geometry, self.radii, line) else {
            return;
        };
        let inner = if self.border_width == 0 {
            Some(outer)
        } else {
            let radii = Radii {
                top_left: (self.radii.top_left - self.border_width).max(0),
                top_right: (self.radii.top_right - self.border_width).max(0),
                bottom_left: (self.radii.bottom_left - self.border_width).max(0),
                bottom_right: (self.radii.bottom_right - self.border_width).max(0),
            };
            RoundedLine::new(self.inner, radii, line)
        };
        let mut draw = |left: i32, right: i32, full_start: i32, full_end: i32| {
            let left = left.max(start);
            let right = right.min(end);
            if left >= right {
                return;
            }
            let full_start = full_start.clamp(left, right);
            let full_end = full_end.clamp(full_start, right);
            for x in (left..full_start).chain(full_end..right) {
                let background = self.background.sample(gradients, x, line);
                let mut border = self.border.sample(gradients, x, line);
                if !matches!(self.background, PreparedFill::Solid(_)) {
                    border = border.over(background);
                }
                let outside = outer.coverage(x) as u32;
                let inside = inner.as_ref().map_or(0, |inner| inner.coverage(x)) as u32;
                let border_coverage = outside.saturating_sub(inside);
                let channel = |a: u8, b: u8| ((a as u32 * border_coverage + b as u32 * inside + 127) / 255) as u8;
                let color = PremultipliedRgbaColor {
                    red: channel(border.red, background.red),
                    green: channel(border.green, background.green),
                    blue: channel(border.blue, background.blue),
                    alpha: channel(border.alpha, background.alpha),
                };
                row.pixels[(x - row.x) as usize].blend(color.coverage(coverage as u32));
            }
            let pixels = &mut row.pixels[(full_start - row.x) as usize..(full_end - row.x) as usize];
            draw_border(full_start, pixels);
        };
        if let Some(inner) = inner {
            let split = inner.full_start().min(inner.full_end());
            draw(outer.visible_start(), split, outer.full_start(), inner.visible_start());
            draw(
                split.max(inner.full_end()),
                outer.visible_end(),
                inner.visible_end(),
                outer.full_end(),
            );
            let left = inner.full_start().max(start);
            let right = inner.full_end().min(end);
            if left < right {
                self.background.draw_span(
                    gradients,
                    left,
                    line,
                    coverage,
                    &mut row.pixels[(left - row.x) as usize..(right - row.x) as usize],
                );
            }
        } else {
            draw(
                outer.visible_start(),
                outer.visible_end(),
                outer.full_start(),
                outer.full_end(),
            );
        }
    }
}

pub struct SolidPair {
    geometry: PhysicalRect,
    first: PremultipliedRgbaColor,
    second: PremultipliedRgbaColor,
}

impl SolidPair {
    pub fn draw_line<P: Pixel>(&self, line: i32, clip: PhysicalRect, row: PixelSpan<'_, P>) {
        let Some(clipped) = self.geometry.intersection(clip).and_then(|area| {
            area.intersection(PhysicalRect {
                x: row.x,
                y: line,
                width: row.pixels.len() as i32,
                height: 1,
            })
        }) else {
            return;
        };
        let start = (clipped.x - row.x) as usize;
        P::blend_solid_pair(
            &mut row.pixels[start..start + clipped.width as usize],
            self.first,
            self.second,
        );
    }
}
