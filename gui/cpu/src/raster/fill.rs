// the atan polynomial in conic_position is adapted from tiny-skia 0.12.0
// https://github.com/linebender/tiny-skia/tree/5d4754777746eef0828be166896eaf482c49f8f2
//
// Copyright (c) 2011 Google Inc. All rights reserved.
// Copyright (c) 2020 Yevhenii Reizner All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
//   * Redistributions of source code must retain the above copyright
//     notice, this list of conditions and the following disclaimer.
//
//   * Redistributions in binary form must reproduce the above copyright
//     notice, this list of conditions and the following disclaimer in
//     the documentation and/or other materials provided with the
//     distribution.
//
//   * Neither the name of the copyright holder nor the names of its
//     contributors may be used to endorse or promote products derived
//     from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use alloc::vec::Vec;
use core::ops::Range;

use blit::PhysicalRect;
use blit_gui::style::Fill;

use crate::{
    Pixel, PremultipliedRgbaColor, math,
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
    // rgba bias and increments along x and y for each span between stops
    pub ramps: Vec<[[i32; 4]; 3]>,
}

#[repr(C)]
pub struct Gradient {
    pub stops: Range<u32>,
    ramps: Range<u32>,
    // linear step and bias or conic center and starting turn
    x: f32,
    y: f32,
    offset: f32,
    opaque: bool,
    inverse: f64,
    // copy of the ramp covering every pixel in bounds which skips stop checks
    ramp: Option<[[i32; 4]; 3]>,
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
                let (dy, dx) = match gradient.angle {
                    0.0 => (0.0, 1.0),
                    90.0 => (1.0, 0.0),
                    180.0 => (0.0, -1.0),
                    270.0 => (-1.0, 0.0),
                    _ => math::sincosf(gradient.angle.to_radians()),
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
                gradient.angle / 360.0,
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
        let opaque = stops.iter().all(|stop| stop.color.alpha == 255);
        let ramps = gradients.ramps.len();
        if matches!(fill, Fill::LinearGradient(_)) {
            let mut exact = true;
            gradients.ramps.extend(stops.windows(2).map(|pair| {
                let first = [
                    pair[0].color.red,
                    pair[0].color.green,
                    pair[0].color.blue,
                    pair[0].color.alpha,
                ]
                .map(f64::from);
                let second = [
                    pair[1].color.red,
                    pair[1].color.green,
                    pair[1].color.blue,
                    pair[1].color.alpha,
                ]
                .map(f64::from);
                let inverse = 1.0 / (pair[1].position as f64 - pair[0].position as f64);
                let mut ramp = [[0; 4]; 3];
                for i in 0..4 {
                    let delta = (second[i] - first[i]) * inverse * (1 << 20) as f64;
                    let values = [
                        first[i] * (1 << 20) as f64 + delta * (offset as f64 - pair[0].position as f64),
                        delta * x as f64,
                        delta * y as f64,
                    ];
                    for (j, value) in values.into_iter().enumerate() {
                        exact &= value.abs() < (1_u64 << 52) as f64;
                        ramp[j][i] = math::round(value) as i64 as i32;
                    }
                }
                ramp
            }));
            if !exact {
                gradients.ramps.truncate(ramps);
            }
        }
        let mut ramp = None;
        if gradients.ramps.len() > ramps {
            let position = offset as f64 + x as f64 * bounds.x as f64 + y as f64 * bounds.y as f64;
            let right = x as f64 * (bounds.width - 1) as f64;
            let bottom = y as f64 * (bounds.height - 1) as f64;
            let low = position + right.min(0.0) + bottom.min(0.0);
            let high = position + right.max(0.0) + bottom.max(0.0);
            ramp = stops
                .windows(2)
                .position(|pair| low >= pair[0].position as f64 && high <= pair[1].position as f64)
                .map(|span| gradients.ramps[ramps + span]);
        }
        gradients.entries.push(Gradient {
            stops: start as u32..end,
            ramps: ramps as u32..gradients.ramps.len() as u32,
            x,
            y,
            offset,
            opaque,
            inverse: 1.0 / x as f64,
            ramp,
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
                if !gradient.ramps.is_empty() {
                    let ramp = match &gradient.ramp {
                        Some(ramp) => ramp,
                        None => {
                            let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                            let last = stops.len() - 1;
                            let position =
                                gradient.offset as f64 + gradient.y as f64 * y as f64 + gradient.x as f64 * x as f64;
                            if position <= stops[0].position as f64 {
                                return stops[0].color;
                            }
                            if position >= stops[last].position as f64 {
                                return stops[last].color;
                            }
                            let span = stops[1..last].partition_point(|stop| stop.position as f64 <= position);
                            &gradients.ramps[gradient.ramps.start as usize + span]
                        }
                    };
                    let rounding = BAYER[((y & 3) * 4 + (x & 3)) as usize] as i32 * 65536 + 32768;
                    let raw = |i: usize| {
                        ramp[0][i]
                            .wrapping_add(ramp[1][i].wrapping_mul(x))
                            .wrapping_add(ramp[2][i].wrapping_mul(y))
                    };
                    let alpha = if gradient.opaque {
                        255
                    } else {
                        ((raw(3).clamp(0, 255 << 20) + (1 << 19)) >> 20) as u8
                    };
                    let max_rgb = i32::from(alpha) << 20;
                    let channel = |i: usize| ((raw(i).clamp(0, max_rgb) + rounding) >> 20) as u8;
                    return PremultipliedRgbaColor {
                        red: channel(0),
                        green: channel(1),
                        blue: channel(2),
                        alpha,
                    };
                }
                let position = if matches!(*self, Self::ConicGradient(_)) {
                    conic_position(gradient, x, y)
                } else {
                    gradient.offset + gradient.y * y as f32 + gradient.x * x as f32
                };
                let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                let index = stops
                    .partition_point(|stop| stop.position <= position)
                    .saturating_sub(1)
                    .min(stops.len() - 2);
                let inverse = 1.0 / (stops[index + 1].position - stops[index].position);
                let amount = ((position - stops[index].position) * inverse).clamp(0.0, 1.0);
                interpolate_gradient(amount, stops[index].color, stops[index + 1].color, x, y)
            }
        }
    }

    #[inline]
    pub fn draw_span<P: Pixel>(&self, gradients: &Gradients, x: i32, y: i32, coverage: u8, pixels: &mut [P]) {
        if pixels.is_empty() {
            return;
        }
        match *self {
            Self::Solid(color) => {
                let color = color.coverage(coverage as u32);
                P::blend_slice(pixels, color);
            }
            Self::Gradient(index) | Self::ConicGradient(index) => {
                let gradient = &gradients.entries[index as usize];
                let ramps = &gradients.ramps[gradient.ramps.start as usize..gradient.ramps.end as usize];
                if !ramps.is_empty() {
                    let write = |pixels: &mut [P], begin: usize, ramp: &[[i32; 4]; 3]| {
                        if pixels.is_empty() {
                            return;
                        }
                        let start_x = x.wrapping_add(begin as i32);
                        let rounding = core::array::from_fn(|i| {
                            BAYER[((y & 3) * 4 + (((start_x & 3) + i as i32) & 3)) as usize] as i32 * 65536 + 32768
                        });
                        if gradient.opaque && coverage == 255 {
                            let color = core::array::from_fn(|i| {
                                ramp[0][i]
                                    .wrapping_add(ramp[1][i].wrapping_mul(start_x))
                                    .wrapping_add(ramp[2][i].wrapping_mul(y))
                            });
                            P::write_ramp(pixels, color, [ramp[1][0], ramp[1][1], ramp[1][2]], rounding);
                        } else {
                            let color = core::array::from_fn(|i| {
                                ramp[0][i]
                                    .wrapping_add(ramp[1][i].wrapping_mul(start_x))
                                    .wrapping_add(ramp[2][i].wrapping_mul(y))
                            });
                            P::blend_ramp(pixels, color, ramp[1], rounding, coverage);
                        }
                    };
                    if let Some(ramp) = &gradient.ramp {
                        write(pixels, 0, ramp);
                        return;
                    }
                    let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                    let last = stops.len() - 1;
                    let position = gradient.offset as f64 + gradient.y as f64 * y as f64;
                    if gradient.x == 0.0 {
                        if position <= stops[0].position as f64 {
                            P::blend_slice(pixels, stops[0].color.coverage(coverage as u32));
                        } else if position >= stops[last].position as f64 {
                            P::blend_slice(pixels, stops[last].color.coverage(coverage as u32));
                        } else {
                            let span = stops[1..last].partition_point(|stop| stop.position as f64 <= position);
                            write(pixels, 0, &ramps[span]);
                        }
                        return;
                    }
                    let length = pixels.len() as f64;
                    let boundary = |stop: &Stop| {
                        (math::ceil((stop.position as f64 - position) * gradient.inverse) - x as f64).clamp(0.0, length)
                            as usize
                    };
                    let (first, end) = (boundary(&stops[0]), boundary(&stops[last]));
                    let edges = if gradient.x < 0.0 {
                        [stops[last].color, stops[0].color]
                    } else {
                        [stops[0].color, stops[last].color]
                    };
                    P::blend_slice(&mut pixels[..first.min(end)], edges[0].coverage(coverage as u32));
                    P::blend_slice(&mut pixels[first.max(end)..], edges[1].coverage(coverage as u32));
                    let mut previous = first;
                    for (ramp, stop) in ramps.iter().zip(&stops[1..]) {
                        let next = boundary(stop);
                        let begin = previous.min(next);
                        write(&mut pixels[begin..previous.max(next)], begin, ramp);
                        previous = next;
                    }
                    return;
                }
                let stops = &gradients.stops[gradient.stops.start as usize..gradient.stops.end as usize];
                let conic = matches!(*self, Self::ConicGradient(_));
                let row_position = gradient.offset + gradient.y * y as f32;
                // neighboring pixels usually share a stop span so the search walks from the last one
                let mut index = 0;
                let mut inverse = 1.0 / (stops[1].position - stops[0].position);
                blend_colors(pixels, x, coverage, |x| {
                    let position = if conic {
                        conic_position(gradient, x, y)
                    } else {
                        row_position + gradient.x * x as f32
                    };
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
                    interpolate_gradient(amount, stops[index].color, stops[index + 1].color, x, y)
                });
            }
        }

        // computes colors a chunk at a time so blending runs in simd lanes
        fn blend_colors<P: Pixel>(
            pixels: &mut [P],
            x: i32,
            coverage: u8,
            mut color: impl FnMut(i32) -> PremultipliedRgbaColor,
        ) {
            let mut colors = [PremultipliedRgbaColor::default(); 64];
            for (index, chunk) in pixels.chunks_mut(colors.len()).enumerate() {
                let start = x + (index * colors.len()) as i32;
                let colors = &mut colors[..chunk.len()];
                for (offset, slot) in colors.iter_mut().enumerate() {
                    *slot = color(start + offset as i32);
                }
                P::blend_texture_slice_rgba(chunk, colors, coverage);
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
    // atan in turns
    let (horizontal, vertical) = (dx.abs(), dy.abs());
    let slope = horizontal.min(vertical) / horizontal.max(vertical);
    let square = slope * slope;
    let mut turn = slope * (0.159_121_17 + square * (-5.185_397e-2 + square * (2.476_102e-2 + square * -7.054_738e-3)));
    if horizontal < vertical {
        turn = 0.25 - turn;
    }
    if dx < 0.0 {
        turn = 0.5 - turn;
    }
    if dy < 0.0 {
        turn = 1.0 - turn;
    }
    let position = turn - gradient.offset;
    position - math::floorf(position)
}
