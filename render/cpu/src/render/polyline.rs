use blit::{LogicalPoint, PhysicalRect, Scale2};

use crate::{Pixel, PixelSpan, PremultipliedRgbaColor, command_list::Polyline};

#[derive(Clone, Copy)]
pub struct Prepared {
    pub geometry: PhysicalRect,
    radius: f32,
    color: PremultipliedRgbaColor,
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
        let color = PremultipliedRgbaColor::new(polyline.color, opacity);
        if color.alpha == 0 {
            return None;
        }
        Some(Self {
            geometry: bounds.to_physical(Scale2::uniform(scale_factor)),
            radius: polyline.width * scale_factor * 0.5,
            color,
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
            for x in left..=right {
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
        for offset in first..last {
            if let Some(coverage) = rasterizer.coverage(offset) {
                let coverage = coverage as u32 * clip_coverage as u32 / 255;
                row.pixels[row_start + offset].blend(self.color.coverage(coverage));
            }
        }
    }
}

#[derive(Default)]
pub struct Rasterizer {
    coverage: Vec<u8>,
    generations: Vec<u32>,
    generation: u32,
}

impl Rasterizer {
    fn begin(&mut self, width: usize) {
        self.coverage.resize(width, 0);
        self.generations.resize(width, 0);
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
        } else {
            self.coverage[offset] = self.coverage[offset].max(coverage);
        }
    }

    fn coverage(&self, offset: usize) -> Option<u8> {
        (self.generations[offset] == self.generation).then_some(self.coverage[offset])
    }
}
