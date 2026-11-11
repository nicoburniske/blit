use std::ops::Range;

use blit::PhysicalRect;
use blit_gui::style::Fill;

use crate::{
    Pixel, PremultipliedRgbaColor,
    pixel::{BAYER, interpolate_gradient},
};

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

#[repr(C)]
pub struct Gradient {
    pub stops: Range<u32>,
    // linear step and bias or conic center and starting turn
    x: f32,
    y: f32,
    offset: f32,
    clamp: bool,
    inverse: f64,
    // rgb bias and increments along x and y
    ramp: Option<[[i32; 3]; 3]>,
}

pub struct Stop {
    pub position: f32,
    pub color: PremultipliedRgbaColor,
}

impl PreparedFill {
    pub fn new(
        fill: Fill<'_>,
        bounds: PhysicalRect,
        opacity: f32,
        background: PremultipliedRgbaColor,
        gradients: &mut Gradients,
    ) -> Self {
        let fill = fill.normalized();
        let (stops, x, y, offset) = match fill {
            Fill::Solid(color) => {
                let color = PremultipliedRgbaColor::with_opacity(color, opacity);
                return Self::Solid(if color.alpha == 0 {
                    color
                } else {
                    color.over(background)
                });
            }
            Fill::LinearGradient(gradient) => {
                let (dy, dx) = match gradient.angle_degrees {
                    0.0 => (0.0, 1.0),
                    90.0 => (1.0, 0.0),
                    180.0 => (0.0, -1.0),
                    270.0 => (-1.0, 0.0),
                    _ => gradient.angle_degrees.to_radians().sin_cos(),
                };
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
            color: PremultipliedRgbaColor::with_opacity(stop.color, opacity).over(background),
        }));
        let end = u32::try_from(gradients.stops.len()).expect("too many gradient stops");
        let stops = &gradients.stops[start..end as usize];
        let mut ramp = None;
        let mut clamp = true;
        if matches!(fill, Fill::LinearGradient(_))
            && stops.len() == 2
            && stops.iter().all(|stop| stop.color.alpha == 255)
        {
            let first = stops[0].color;
            let second = stops[1].color;
            let first = [first.red, first.green, first.blue].map(f64::from);
            let second = [second.red, second.green, second.blue].map(f64::from);
            let inverse = 1.0 / (stops[1].position as f64 - stops[0].position as f64);
            let position = offset as f64 + x as f64 * bounds.x as f64 + y as f64 * bounds.y as f64;
            let right = x as f64 * (bounds.width - 1) as f64;
            let bottom = y as f64 * (bounds.height - 1) as f64;
            clamp = position + right.min(0.0) + bottom.min(0.0) < stops[0].position as f64
                || position + right.max(0.0) + bottom.max(0.0) > stops[1].position as f64;
            let mut color = [[0; 3]; 3];
            let mut exact = true;
            for i in 0..3 {
                let delta = (second[i] - first[i]) * inverse * (1 << 20) as f64;
                let values = [
                    first[i] * (1 << 20) as f64 + delta * (offset as f64 - stops[0].position as f64),
                    delta * x as f64,
                    delta * y as f64,
                ];
                for (j, value) in values.into_iter().enumerate() {
                    exact &= value.abs() < (1_u64 << 52) as f64;
                    color[j][i] = value.round() as i64 as i32;
                }
            }
            ramp = exact.then_some(color);
        }
        gradients.entries.push(Gradient {
            stops: start as u32..end,
            x,
            y,
            offset,
            inverse: 1.0 / x as f64,
            ramp,
            clamp,
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
                if let Some(ramp) = &gradient.ramp {
                    if gradient.clamp {
                        let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                        let position =
                            gradient.offset as f64 + gradient.y as f64 * y as f64 + gradient.x as f64 * x as f64;
                        if position <= stops[0].position as f64 {
                            return stops[0].color;
                        }
                        if position >= stops[1].position as f64 {
                            return stops[1].color;
                        }
                    }
                    let rounding = BAYER[((y & 3) * 4 + (x & 3)) as usize] as i32 * 65536 + 32768;
                    let channel = |i: usize| {
                        let color = ramp[0][i]
                            .wrapping_add(ramp[1][i].wrapping_mul(x))
                            .wrapping_add(ramp[2][i].wrapping_mul(y));
                        ((color + rounding) >> 20).clamp(0, 255) as u8
                    };
                    return PremultipliedRgbaColor {
                        red: channel(0),
                        green: channel(1),
                        blue: channel(2),
                        alpha: 255,
                    };
                }
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

    #[inline]
    pub fn draw_span<P: Pixel>(&self, gradients: &Gradients, x: i32, y: i32, coverage: u8, pixels: &mut [P]) {
        match *self {
            Self::Solid(color) => {
                let color = color.coverage(coverage as u32);
                P::blend_slice(pixels, color);
            }
            Self::Gradient(index) => {
                let gradient = &gradients.entries[index as usize];
                if let Some(ramp) = &gradient.ramp
                    && coverage == 255
                {
                    let (begin, end) = if !gradient.clamp {
                        (0, pixels.len())
                    } else {
                        let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                        let position = gradient.offset as f64 + gradient.y as f64 * y as f64;
                        if gradient.x == 0.0 {
                            if position <= stops[0].position as f64 || position >= stops[1].position as f64 {
                                let color = if position <= stops[0].position as f64 {
                                    stops[0].color
                                } else {
                                    stops[1].color
                                };
                                pixels.fill(P::from_rgba(color));
                                return;
                            }
                            (0, pixels.len())
                        } else {
                            let a = ((stops[0].position as f64 - position) * gradient.inverse).ceil();
                            let b = ((stops[1].position as f64 - position) * gradient.inverse).ceil();
                            let begin = (a.min(b) - x as f64).clamp(0.0, pixels.len() as f64) as usize;
                            let end = (a.max(b) - x as f64).clamp(0.0, pixels.len() as f64) as usize;
                            let edges = if gradient.x < 0.0 {
                                [stops[1].color, stops[0].color]
                            } else {
                                [stops[0].color, stops[1].color]
                            };
                            pixels[..begin].fill(P::from_rgba(edges[0]));
                            pixels[end..].fill(P::from_rgba(edges[1]));
                            (begin, end)
                        }
                    };
                    let color = std::array::from_fn(|i| {
                        ramp[0][i]
                            .wrapping_add(ramp[1][i].wrapping_mul(x.wrapping_add(begin as i32)))
                            .wrapping_add(ramp[2][i].wrapping_mul(y))
                    });
                    let rounding = std::array::from_fn(|i| {
                        BAYER[((y & 3) * 4 + ((((x + begin as i32) & 3) + i as i32) & 3)) as usize] as i32 * 65536
                            + 32768
                    });
                    P::write_ramp(&mut pixels[begin..end], color, ramp[1], rounding);
                    return;
                }
                let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                let row_position = gradient.offset + gradient.y * y as f32;
                let position = row_position + gradient.x * x as f32;
                let mut index = stops
                    .partition_point(|stop| stop.position <= position)
                    .saturating_sub(1)
                    .min(stops.len() - 2);
                let mut inverse = 1.0 / (stops[index + 1].position - stops[index].position);
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
                    let color =
                        interpolate_gradient(amount, stops[index].color, stops[index + 1].color, x + i as i32, y);
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
    let inverse = 1.0 / (stops[index + 1].position - stops[index].position);
    let amount = ((position - stops[index].position) * inverse).clamp(0.0, 1.0);
    interpolate_gradient(amount, stops[index].color, stops[index + 1].color, x, y)
}
