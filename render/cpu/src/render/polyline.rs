use std::simd::{
    Select, Simd, StdFloat,
    cmp::{SimdOrd, SimdPartialEq, SimdPartialOrd},
    num::SimdFloat,
};

use blit::{LogicalPoint, PhysicalRect, Scale2};

use crate::{Pixel, PixelSpan, PremultipliedRgbaColor, color::Color, command_list::Polyline};

type F32x8 = Simd<f32, 8>;
type U8x8 = Simd<u8, 8>;
type U32x8 = Simd<u32, 8>;

const MIN_SIMD_COMPOSITION: usize = 32;

#[derive(Clone, Copy)]
pub struct Prepared {
    pub geometry: PhysicalRect,
    radius: f32,
    color: Color,
    premultiplied: PremultipliedRgbaColor,
}

#[derive(Clone, Copy)]
pub struct Segment {
    start: LogicalPoint,
    delta_x: f32,
    delta_y: f32,
    inverse_length_squared: f32,
    minimum_y: f32,
    maximum_y: f32,
}

impl Segment {
    pub fn new(start: LogicalPoint, end: LogicalPoint) -> Self {
        let delta_x = end.x - start.x;
        let delta_y = end.y - start.y;
        Self {
            start,
            delta_x,
            delta_y,
            inverse_length_squared: 1.0 / (delta_x * delta_x + delta_y * delta_y),
            minimum_y: start.y.min(end.y),
            maximum_y: start.y.max(end.y),
        }
    }
}

impl Prepared {
    pub fn new(polyline: &Polyline<'_>, scale_factor: f32) -> Option<Self> {
        let bounds = polyline.bounds()?;
        if !polyline.opacity.is_finite() || polyline.opacity <= 0.0 {
            return None;
        }
        let opacity = (polyline.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let mut color = polyline.color;
        color.alpha = (color.alpha as u16 * opacity as u16 / 255) as u8;
        if color.alpha == 0 {
            return None;
        }
        Some(Self {
            geometry: bounds.to_physical(Scale2::uniform(scale_factor)),
            radius: polyline.width * scale_factor * 0.5,
            color,
            premultiplied: PremultipliedRgbaColor::new(color, 255),
        })
    }

    pub fn draw_line<P: Pixel>(
        &self,
        segments: &[Segment],
        line: i32,
        clip: PhysicalRect,
        clip_coverage: u8,
        row: PixelSpan<'_, P>,
        rasterizer: &mut Rasterizer,
    ) {
        let width = clip.width as usize;
        rasterizer.begin(width);
        let y = line as f32 + 0.5;
        let outer = self.radius + 0.5;
        let inner = (self.radius - 0.5).max(0.0);
        let inner_squared = inner * inner;
        let outer_squared = outer * outer;
        let mut first = width;
        let mut last = 0;

        for segment in segments {
            if y <= segment.minimum_y - outer || y >= segment.maximum_y + outer {
                continue;
            }
            let mut low = 0.0;
            let mut high = 1.0;
            if segment.delta_y == 0.0 {
                if (y - segment.start.y).abs() >= outer {
                    continue;
                }
            } else {
                low = (y - outer - segment.start.y) / segment.delta_y;
                high = (y + outer - segment.start.y) / segment.delta_y;
                if low > high {
                    std::mem::swap(&mut low, &mut high);
                }
                low = low.max(0.0);
                high = high.min(1.0);
                if low >= high {
                    continue;
                }
            }
            let low_x = segment.start.x + segment.delta_x * low;
            let high_x = segment.start.x + segment.delta_x * high;
            let left = ((low_x.min(high_x) - outer - 0.5).ceil() as i32).max(clip.x);
            let right =
                ((low_x.max(high_x) + outer - 0.5).floor() as i32).min(clip.x + clip.width - 1);
            if left > right {
                continue;
            }
            let mut x = left;
            let end = right + 1;
            if segment.inverse_length_squared.is_finite() {
                let start_x = F32x8::splat(segment.start.x);
                let start_y = F32x8::splat(segment.start.y);
                let delta_x = F32x8::splat(segment.delta_x);
                let delta_y = F32x8::splat(segment.delta_y);
                let inverse_length_squared = F32x8::splat(segment.inverse_length_squared);
                let pixel_y = F32x8::splat(y);
                let inner_squared = F32x8::splat(inner_squared);
                let outer_squared = F32x8::splat(outer_squared);
                let outer = F32x8::splat(outer);
                while x + 8 <= end {
                    let pixel_x = F32x8::from_array([0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5])
                        + F32x8::splat(x as f32);
                    let projection = (((pixel_x - start_x) * delta_x
                        + (pixel_y - start_y) * delta_y)
                        * inverse_length_squared)
                        .simd_max(F32x8::splat(0.0))
                        .simd_min(F32x8::splat(1.0));
                    let distance_x = pixel_x - (start_x + delta_x * projection);
                    let distance_y = pixel_y - (start_y + delta_y * projection);
                    let distance_squared = distance_x * distance_x + distance_y * distance_y;
                    let coverage = distance_squared.simd_le(inner_squared).select(
                        F32x8::splat(255.0),
                        distance_squared.simd_ge(outer_squared).select(
                            F32x8::splat(0.0),
                            ((outer - distance_squared.sqrt()) * F32x8::splat(255.0)).round(),
                        ),
                    );
                    let coverage = coverage.cast::<u8>();
                    let offset = (x - clip.x) as usize;
                    let old_coverage = U8x8::from_slice(&rasterizer.coverage[offset..offset + 8]);
                    let old_generations =
                        U32x8::from_slice(&rasterizer.generations[offset..offset + 8]);
                    let active = coverage.simd_ne(U8x8::splat(0));
                    let lanes = active.to_bitmask();
                    let current = old_generations.simd_eq(U32x8::splat(rasterizer.generation));
                    rasterizer.covered += (lanes & !current.to_bitmask()).count_ones() as usize;
                    active
                        .select(
                            current.select(old_coverage.simd_max(coverage), coverage),
                            old_coverage,
                        )
                        .copy_to_slice(&mut rasterizer.coverage[offset..offset + 8]);
                    active
                        .select(U32x8::splat(rasterizer.generation), old_generations)
                        .copy_to_slice(&mut rasterizer.generations[offset..offset + 8]);
                    if lanes != 0 {
                        first = first.min(offset + lanes.trailing_zeros() as usize);
                        last = last.max(offset + lanes.ilog2() as usize + 1);
                    }
                    x += 8;
                }
            }
            for x in x..end {
                let pixel_x = x as f32 + 0.5;
                let projection = if segment.inverse_length_squared.is_finite() {
                    ((pixel_x - segment.start.x) * segment.delta_x
                        + (y - segment.start.y) * segment.delta_y)
                        * segment.inverse_length_squared
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);
                let nearest_x = segment.start.x + segment.delta_x * projection;
                let nearest_y = segment.start.y + segment.delta_y * projection;
                let distance_x = pixel_x - nearest_x;
                let distance_y = y - nearest_y;
                let distance_squared = distance_x * distance_x + distance_y * distance_y;
                let coverage = if distance_squared <= inner_squared {
                    255
                } else if distance_squared >= outer_squared {
                    0
                } else {
                    ((outer - distance_squared.sqrt()) * 255.0).round() as u8
                };
                if coverage == 0 {
                    continue;
                }
                let offset = (x - clip.x) as usize;
                rasterizer.cover(offset, coverage);
                first = first.min(offset);
                last = last.max(offset + 1);
            }
        }

        let row_start = (clip.x - row.x) as usize;
        if first < last
            && last - first >= MIN_SIMD_COMPOSITION
            && rasterizer.covered == last - first
        {
            for coverage in &mut rasterizer.coverage[first..last] {
                *coverage = (*coverage as u16 * clip_coverage as u16 / 255) as u8;
            }
            P::blend_alpha_slice(
                &mut row.pixels[row_start + first..row_start + last],
                self.color,
                &rasterizer.coverage[first..last],
            );
            return;
        }
        for offset in first..last {
            if rasterizer.generations[offset] == rasterizer.generation {
                let coverage = rasterizer.coverage[offset] as u32 * clip_coverage as u32 / 255;
                row.pixels[row_start + offset].blend(self.premultiplied.coverage(coverage));
            }
        }
    }
}

#[derive(Default)]
pub struct Rasterizer {
    coverage: Vec<u8>,
    generations: Vec<u32>,
    generation: u32,
    covered: usize,
}

impl Rasterizer {
    fn begin(&mut self, width: usize) {
        self.coverage.resize(width, 0);
        self.generations.resize(width, 0);
        self.covered = 0;
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.generations.fill(0);
            self.generation = 1;
        }
    }

    fn cover(&mut self, offset: usize, coverage: u8) {
        if self.generations[offset] != self.generation {
            self.generations[offset] = self.generation;
            self.coverage[offset] = coverage;
            self.covered += 1;
        } else {
            self.coverage[offset] = self.coverage[offset].max(coverage);
        }
    }
}
