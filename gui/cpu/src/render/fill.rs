use std::ops::Range;

use blit::PhysicalRect;
use blit_gui::style::Fill;

use crate::{Pixel, PremultipliedRgbaColor};

#[derive(Clone, Copy)]
pub enum PreparedFill {
    Solid(PremultipliedRgbaColor),
    Gradient(u32),
}

#[derive(Default)]
pub struct Gradients {
    pub entries: Vec<Gradient>,
    pub stops: Vec<Stop>,
}

pub struct Gradient {
    pub stops: Range<u32>,
    x_step: f32,
    y_step: f32,
    offset: f32,
}

pub struct Stop {
    pub position: f32,
    pub color: PremultipliedRgbaColor,
}

impl PreparedFill {
    pub fn new(fill: Fill<'_>, bounds: PhysicalRect, opacity: f32, gradients: &mut Gradients) -> Self {
        match fill {
            Fill::Solid(color) => Self::Solid(PremultipliedRgbaColor::with_opacity(color, opacity)),
            Fill::LinearGradient(gradient) => {
                let index = u32::try_from(gradients.entries.len()).expect("too many gradients");
                let start = gradients.stops.len();
                gradients.stops.extend(gradient.stops.iter().map(|stop| Stop {
                    position: stop.position,
                    color: PremultipliedRgbaColor::with_opacity(stop.color, opacity),
                }));
                let (dy, dx) = gradient.angle_degrees.to_radians().sin_cos();
                let extent = dx.abs() * bounds.width as f32 + dy.abs() * bounds.height as f32;
                let minimum = dx.min(0.0) * bounds.width as f32 + dy.min(0.0) * bounds.height as f32;
                let end = u32::try_from(gradients.stops.len()).expect("too many gradient stops");
                gradients.entries.push(Gradient {
                    stops: start as u32..end,
                    x_step: dx / extent,
                    y_step: dy / extent,
                    offset: (0.5 * dx + 0.5 * dy - minimum - dx * bounds.x as f32 - dy * bounds.y as f32) / extent,
                });
                Self::Gradient(index)
            }
        }
    }

    #[inline]
    pub fn sample(&self, gradients: &Gradients, x: i32, y: i32) -> PremultipliedRgbaColor {
        match *self {
            Self::Solid(color) => color,
            Self::Gradient(index) => {
                let gradient = &gradients.entries[index as usize];
                let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                let position = gradient.offset + gradient.y_step * y as f32 + gradient.x_step * x as f32;
                let index = stops
                    .partition_point(|stop| stop.position <= position)
                    .saturating_sub(1)
                    .min(stops.len() - 2);
                let amount = ((position - stops[index].position) / (stops[index + 1].position - stops[index].position))
                    .clamp(0.0, 1.0);
                super::rounded::interpolate(
                    (amount * 255.0).round() as u32,
                    stops[index].color,
                    stops[index + 1].color,
                )
            }
        }
    }

    #[inline(always)]
    pub fn draw_span<P: Pixel>(&self, gradients: &Gradients, x: i32, y: i32, coverage: u8, pixels: &mut [P]) {
        match *self {
            Self::Solid(color) => {
                let color = color.coverage(coverage as u32);
                P::blend_slice(pixels, color);
            }
            Self::Gradient(index) => {
                let gradient = &gradients.entries[index as usize];
                let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                let row_position = gradient.offset + gradient.y_step * y as f32;
                let position = row_position + gradient.x_step * x as f32;
                let mut index = stops
                    .partition_point(|stop| stop.position <= position)
                    .saturating_sub(1)
                    .min(stops.len() - 2);
                let mut inverse = 1.0 / (stops[index + 1].position - stops[index].position);
                for (i, pixel) in pixels.iter_mut().enumerate() {
                    let position = row_position + gradient.x_step * (x as f32 + i as f32);
                    let previous = index;
                    while index + 1 < stops.len() - 1 && position >= stops[index + 1].position {
                        index += 1;
                    }
                    while index != 0 && position < stops[index].position {
                        index -= 1;
                    }
                    if index != previous {
                        inverse = 1.0 / (stops[index + 1].position - stops[index].position);
                    }
                    let amount =
                        (((position - stops[index].position) * inverse).clamp(0.0, 1.0) * 255.0).round() as u32;
                    let color = super::rounded::interpolate(amount, stops[index].color, stops[index + 1].color);
                    pixel.blend(color.coverage(coverage as u32));
                }
            }
        }
    }
}
