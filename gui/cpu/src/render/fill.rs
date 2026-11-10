use std::ops::Range;

use blit::PhysicalRect;
use blit_gui::style::Fill;

use crate::{Pixel, PremultipliedRgbaColor};

#[derive(Clone, Copy)]
pub enum PreparedFill {
    Solid(PremultipliedRgbaColor),
    Gradient(u32),
    ConicGradient(u32),
}

#[derive(Default)]
pub struct Gradients {
    pub entries: Vec<Gradient>,
    pub stops: Vec<Stop>,
}

pub struct Gradient {
    pub stops: Range<u32>,
    // linear step and bias or conic center and starting turn
    x: f32,
    y: f32,
    offset: f32,
}

pub struct Stop {
    pub position: f32,
    pub color: PremultipliedRgbaColor,
}

impl PreparedFill {
    pub fn new(fill: Fill<'_>, bounds: PhysicalRect, opacity: f32, gradients: &mut Gradients) -> Self {
        let fill = fill.normalized();
        let (stops, x, y, offset) = match fill {
            Fill::Solid(color) => return Self::Solid(PremultipliedRgbaColor::with_opacity(color, opacity)),
            Fill::LinearGradient(gradient) => {
                let (dy, dx) = gradient.angle_degrees.to_radians().sin_cos();
                let extent = dx.abs() * bounds.width as f32 + dy.abs() * bounds.height as f32;
                let minimum = dx.min(0.0) * bounds.width as f32 + dy.min(0.0) * bounds.height as f32;
                (
                    gradient.stops,
                    dx / extent,
                    dy / extent,
                    (0.5 * dx + 0.5 * dy - minimum - dx * bounds.x as f32 - dy * bounds.y as f32) / extent,
                )
            }
            Fill::ConicGradient(gradient) => (
                gradient.stops,
                bounds.x as f32 + gradient.center[0] * bounds.width as f32,
                bounds.y as f32 + gradient.center[1] * bounds.height as f32,
                gradient.angle_degrees / 360.0,
            ),
        };
        let index = u32::try_from(gradients.entries.len()).expect("too many gradients");
        let start = gradients.stops.len();
        gradients.stops.extend(stops.iter().map(|stop| Stop {
            position: stop.position,
            color: PremultipliedRgbaColor::with_opacity(stop.color, opacity),
        }));
        let end = u32::try_from(gradients.stops.len()).expect("too many gradient stops");
        gradients.entries.push(Gradient {
            stops: start as u32..end,
            x,
            y,
            offset,
        });
        if matches!(fill, Fill::ConicGradient(_)) {
            Self::ConicGradient(index)
        } else {
            Self::Gradient(index)
        }
    }

    #[inline]
    pub fn sample(&self, gradients: &Gradients, x: i32, y: i32) -> PremultipliedRgbaColor {
        match *self {
            Self::Solid(color) => color,
            Self::Gradient(index) | Self::ConicGradient(index) => {
                let gradient = &gradients.entries[index as usize];
                let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                let position = if matches!(*self, Self::ConicGradient(_)) {
                    conic_position(gradient, x, y)
                } else {
                    gradient.offset + gradient.y * y as f32 + gradient.x * x as f32
                };
                sample_stops(stops, position, x, y)
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
                let row_position = gradient.offset + gradient.y * y as f32;
                let position = row_position + gradient.x * x as f32;
                let mut index = stops
                    .partition_point(|stop| stop.position <= position)
                    .saturating_sub(1)
                    .min(stops.len() - 2);
                let mut inverse = 1.0 / (stops[index + 1].position - stops[index].position);
                if stops.len() == 2 && coverage == 255 && stops.iter().all(|stop| stop.color.alpha == 255) {
                    let amount = (position - stops[0].position) * inverse;
                    let last_position = row_position + gradient.x * (x as f32 + pixels.len().saturating_sub(1) as f32);
                    let last_amount = (last_position - stops[0].position) * inverse;
                    if (0.0..=1.0).contains(&amount) && (0.0..=1.0).contains(&last_amount) {
                        let first = stops[0].color;
                        let second = stops[1].color;
                        let color = [first.red, first.green, first.blue].map(f32::from);
                        let delta = [
                            second.red as f32 - color[0],
                            second.green as f32 - color[1],
                            second.blue as f32 - color[2],
                        ];
                        let rounding = std::array::from_fn(|i| {
                            (BAYER[((y & 3) * 4 + ((x + i as i32) & 3)) as usize] as f32 + 0.5) / 16.0
                        });
                        P::write_opaque_gradient(
                            pixels,
                            std::array::from_fn(|i| color[i] + delta[i] * amount),
                            delta.map(|value| value * gradient.x * inverse),
                            rounding,
                        );
                        return;
                    }
                }
                for (i, pixel) in pixels.iter_mut().enumerate() {
                    let position = row_position + gradient.x * (x as f32 + i as f32);
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
                    let amount = ((position - stops[index].position) * inverse).clamp(0.0, 1.0);
                    let color = interpolate(amount, stops[index].color, stops[index + 1].color, x + i as i32, y);
                    pixel.blend(color.coverage(coverage as u32));
                }
            }
            Self::ConicGradient(index) => {
                let gradient = &gradients.entries[index as usize];
                let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                for (i, pixel) in pixels.iter_mut().enumerate() {
                    let x = x + i as i32;
                    let color = sample_stops(stops, conic_position(gradient, x, y), x, y);
                    pixel.blend(color.coverage(coverage as u32));
                }
            }
        }
    }
}

fn conic_position(gradient: &Gradient, x: i32, y: i32) -> f32 {
    let dx = x as f32 + 0.5 - gradient.x;
    let dy = y as f32 + 0.5 - gradient.y;
    if dx == 0.0 && dy == 0.0 {
        return 0.0;
    }
    let position = dy.atan2(dx) / std::f32::consts::TAU - gradient.offset;
    position - position.floor()
}

fn sample_stops(stops: &[Stop], position: f32, x: i32, y: i32) -> PremultipliedRgbaColor {
    let index = stops
        .partition_point(|stop| stop.position <= position)
        .saturating_sub(1)
        .min(stops.len() - 2);
    let amount =
        ((position - stops[index].position) / (stops[index + 1].position - stops[index].position)).clamp(0.0, 1.0);
    interpolate(amount, stops[index].color, stops[index + 1].color, x, y)
}

const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

fn interpolate(
    amount: f32,
    first: PremultipliedRgbaColor,
    second: PremultipliedRgbaColor,
    x: i32,
    y: i32,
) -> PremultipliedRgbaColor {
    // interpolate with sixteen fractional bits before converting to bytes
    let threshold = BAYER[((y & 3) * 4 + (x & 3)) as usize] as i32 * 4096 + 2048;
    let amount = (amount * 65536.0).round() as i32;
    let alpha = (((first.alpha as i32) << 16) + (second.alpha as i32 - first.alpha as i32) * amount + 32768) >> 16;
    let channel =
        |a: u8, b: u8| ((((a as i32) << 16) + (b as i32 - a as i32) * amount + threshold) >> 16).min(alpha) as u8;
    PremultipliedRgbaColor {
        alpha: alpha as u8,
        red: channel(first.red, second.red),
        green: channel(first.green, second.green),
        blue: channel(first.blue, second.blue),
    }
}
