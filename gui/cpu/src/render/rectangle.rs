use std::ops::Range;

use blit::{PhysicalRect, Scale2};
use blit_gui::display_list::Rectangle;

use super::{
    fill::{Gradients, PreparedFill},
    rounded::{Radii, RoundedGradient, RoundedLine, RoundedRectangle, draw_gradient_line, draw_line},
};
use crate::{Pixel, PixelSpan, PremultipliedRgbaColor};

#[derive(Clone, Copy)]
pub struct Prepared {
    pub geometry: PhysicalRect,
    pub radii: Radii,
    inner: PhysicalRect,
    pub background: PreparedFill,
    border: PreparedFill,
    border_width: i32,
}

impl Prepared {
    #[inline]
    pub fn new(rectangle: &Rectangle<'_>, scale: f32, gradients: &mut Gradients) -> Option<Self> {
        let geometry = rectangle.area.to_physical(Scale2::uniform(scale));
        if geometry.width <= 0 || geometry.height <= 0 || rectangle.opacity <= 0.0 {
            return None;
        }
        let transparent = PreparedFill::Solid(PremultipliedRgbaColor::default());
        let background = PreparedFill::new(rectangle.background, geometry, rectangle.opacity, gradients);
        let mut border_width = (rectangle.border.width * scale).round().max(0.0) as i32;
        let mut border = if border_width > 0 {
            PreparedFill::new(rectangle.border.fill, geometry, rectangle.opacity, gradients)
        } else {
            transparent
        };
        if matches!(border, PreparedFill::Solid(color) if color.alpha == 0) {
            border_width = 0;
        }
        if border_width == 0 && matches!(background, PreparedFill::Solid(color) if color.alpha == 0) {
            return None;
        }
        if let PreparedFill::Solid(background) = background {
            match &mut border {
                PreparedFill::Solid(color) => *color = color.over(background),
                PreparedFill::Gradient(index) => {
                    let range = gradients.entries[*index as usize].stops.clone();
                    for stop in &mut gradients.stops[range.start as usize..range.end as usize] {
                        stop.color = stop.color.over(background);
                    }
                }
            }
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

    pub fn overwrites(&self) -> bool {
        matches!(self.background, PreparedFill::Solid(color) if color.alpha == 255)
    }

    pub fn overwrite_span(&self, line: i32) -> Option<Range<i32>> {
        if !self.overwrites() || line < self.geometry.y || line >= self.geometry.y + self.geometry.height {
            return None;
        }
        let rounded = RoundedLine::new(self.geometry, self.radii, line)?;
        let start = rounded.full_start();
        let end = rounded.full_end();
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
        if self.radii.is_zero() {
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
        let clipped = PhysicalRect {
            x: start,
            y: line,
            width: end - start,
            height: 1,
        };
        let pixels = &mut row.pixels[(start - row.x) as usize..(end - row.x) as usize];
        if let (PreparedFill::Solid(background), PreparedFill::Solid(border)) = (self.background, self.border) {
            draw_line(
                clipped,
                line,
                &RoundedRectangle {
                    radii: self.radii,
                    border_width: self.border_width,
                    border_color: border.coverage(coverage as u32),
                    inner_color: background.coverage(coverage as u32),
                    top_clip: line - self.geometry.y,
                    bottom_clip: self.geometry.y + self.geometry.height - line - 1,
                    left_clip: start - self.geometry.x,
                    right_clip: self.geometry.x + self.geometry.width - end,
                },
                pixels,
            );
        } else {
            draw_gradient_line(
                clipped,
                line,
                &RoundedGradient {
                    radii: self.radii,
                    border_width: self.border_width,
                    background: self.background,
                    top_clip: line - self.geometry.y,
                    bottom_clip: self.geometry.y + self.geometry.height - line - 1,
                    left_clip: start - self.geometry.x,
                    right_clip: self.geometry.x + self.geometry.width - end,
                },
                gradients,
                coverage,
                pixels,
                |x| {
                    if self.border_width == 0 {
                        return self.background.sample(gradients, x, line).coverage(coverage as u32);
                    }
                    let mut color = self.border.sample(gradients, x, line);
                    if !matches!(self.background, PreparedFill::Solid(_)) {
                        color = color.over(self.background.sample(gradients, x, line));
                    }
                    color.coverage(coverage as u32)
                },
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
