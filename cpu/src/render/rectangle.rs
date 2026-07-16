use std::ops::Range;

use blit::{
    geometry::PhysicalRect,
    paint::{Border, GradientStop, LinearGradient, Rectangle},
};

use super::rounded::{
    draw_gradient_line, draw_line, interpolate, Radii, RoundedGradient, RoundedLine, RoundedRectangle,
};
use crate::{Pixel, PixelSpan, PremultipliedRgbaColor};

#[derive(Clone, Copy)]
pub struct Prepared {
    pub geometry: PhysicalRect,
    inner: PhysicalRect,
    radii: Radii,
    border_width: i32,
    pub border_color: PremultipliedRgbaColor,
    pub inner_color: PremultipliedRgbaColor,
}

impl Prepared {
    pub fn new(rectangle: &Rectangle<'_>, scale_factor: f32) -> Option<Self> {
        let geometry = rectangle.area.to_physical(scale_factor);
        if geometry.width <= 0 || geometry.height <= 0 || rectangle.opacity <= 0.0 {
            return None;
        }
        let (width, color) = match rectangle.border {
            Border::Solid { width, color } => (width, color),
            Border::None | Border::Gradient { .. } => (0.0, blit::color::Color::TRANSPARENT),
        };
        let mut border_width = (width * scale_factor).round().max(0.0) as i32;
        let opacity = (rectangle.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let inner_color = PremultipliedRgbaColor::new(rectangle.background, opacity);
        let border = PremultipliedRgbaColor::new(color, opacity);
        if border.alpha == 0 {
            border_width = 0;
        }
        let border_color = prepare_border_color(color, opacity, inner_color);
        if inner_color.alpha == 0 && border_width == 0 {
            return None;
        }
        let radii = Radii::new(rectangle.radius, scale_factor, geometry.width, geometry.height);
        let inner = PhysicalRect {
            x: geometry.x + border_width,
            y: geometry.y + border_width,
            width: (geometry.width - border_width * 2).max(0),
            height: (geometry.height - border_width * 2).max(0),
        };
        Some(Self { geometry, inner, radii, border_width, border_color, inner_color })
    }

    pub fn is_opaque(&self) -> bool {
        self.inner_color.alpha == 255 && (self.border_width == 0 || self.border_color.alpha == 255)
    }

    pub fn opaque_span(&self, line: i32) -> Option<Range<i32>> {
        if !self.is_opaque() {
            return None;
        }
        let rounded = RoundedLine::new(self.geometry, self.radii, line)?;
        let start = rounded.full_start();
        let end = rounded.full_end();
        (start < end).then_some(start..end)
    }

    pub fn draw_line<P: Pixel>(&self, line: i32, clip: PhysicalRect, row: PixelSpan<'_, P>) {
        let Some(clipped) = self.geometry.intersection(clip).and_then(|area| {
            area.intersection(PhysicalRect { x: row.x, y: line, width: row.pixels.len() as i32, height: 1 })
        }) else {
            return;
        };
        let pixels = &mut row.pixels[(clipped.x - row.x) as usize..][..clipped.width as usize];
        if self.radii.is_zero() {
            if self.border_width == 0 || line < self.inner.y || line >= self.inner.y + self.inner.height {
                P::blend_slice(
                    pixels,
                    if self.border_width == 0 { self.inner_color } else { self.border_color },
                );
                return;
            }
            let left = (self.inner.x - clipped.x).clamp(0, clipped.width) as usize;
            let right = (self.inner.x + self.inner.width - clipped.x).clamp(0, clipped.width) as usize;
            P::blend_slice(&mut pixels[..left], self.border_color);
            P::blend_slice(&mut pixels[left..right], self.inner_color);
            P::blend_slice(&mut pixels[right..], self.border_color);
            return;
        }
        draw_line(
            clipped,
            line,
            &RoundedRectangle {
                radii: self.radii,
                border_width: self.border_width,
                border_color: self.border_color,
                inner_color: self.inner_color,
                top_clip: clipped.y - self.geometry.y,
                bottom_clip: self.geometry.y + self.geometry.height - clipped.y - clipped.height,
                left_clip: clipped.x - self.geometry.x,
                right_clip: self.geometry.x + self.geometry.width - clipped.x - clipped.width,
            },
            pixels,
        );
    }
}

#[derive(Clone, Copy)]
pub struct Gradient {
    pub geometry: PhysicalRect,
    inner: PhysicalRect,
    radii: Radii,
    border_width: i32,
    inner_color: PremultipliedRgbaColor,
    opacity: u8,
    x_step: f32,
    y_step: f32,
    offset: f32,
}

impl Gradient {
    pub fn new(
        rectangle: &Rectangle<'_>,
        width: f32,
        gradient: LinearGradient<'_>,
        scale_factor: f32,
    ) -> Option<Self> {
        if gradient.stops.len() < 2
            || !gradient.angle_degrees.is_finite()
            || gradient
                .stops
                .iter()
                .any(|stop| !stop.position.is_finite() || stop.position < 0.0 || stop.position > 1.0)
            || gradient.stops.windows(2).any(|stops| stops[0].position >= stops[1].position)
        {
            return None;
        }
        let geometry = rectangle.area.to_physical(scale_factor);
        let border_width = (width * scale_factor).round().max(0.0) as i32;
        if geometry.width <= 0 || geometry.height <= 0 || rectangle.opacity <= 0.0 || border_width == 0 {
            return None;
        }
        let opacity = (rectangle.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let inner_color = PremultipliedRgbaColor::new(rectangle.background, opacity);
        let radii = Radii::new(rectangle.radius, scale_factor, geometry.width, geometry.height);
        let inner = PhysicalRect {
            x: geometry.x + border_width,
            y: geometry.y + border_width,
            width: (geometry.width - border_width * 2).max(0),
            height: (geometry.height - border_width * 2).max(0),
        };
        let angle = gradient.angle_degrees.to_radians();
        let direction_x = angle.cos();
        let direction_y = angle.sin();
        let extent = direction_x.abs() * geometry.width as f32 + direction_y.abs() * geometry.height as f32;
        let minimum =
            direction_x.min(0.0) * geometry.width as f32 + direction_y.min(0.0) * geometry.height as f32;
        Some(Self {
            geometry,
            inner,
            radii,
            border_width,
            inner_color,
            opacity,
            x_step: direction_x / extent,
            y_step: direction_y / extent,
            offset: (0.5 * direction_x + 0.5 * direction_y - minimum) / extent,
        })
    }

    pub fn draw_line<P: Pixel>(
        &self,
        stops: &[GradientStop],
        line: i32,
        clip: PhysicalRect,
        coverage: u8,
        row: PixelSpan<'_, P>,
    ) {
        let Some(clipped) = self.geometry.intersection(clip).and_then(|area| {
            area.intersection(PhysicalRect { x: row.x, y: line, width: row.pixels.len() as i32, height: 1 })
        }) else {
            return;
        };
        let coverage = coverage as u32;
        let pixels = &mut row.pixels[(clipped.x - row.x) as usize..][..clipped.width as usize];
        if self.radii.is_zero() {
            if line < self.inner.y || line >= self.inner.y + self.inner.height {
                self.blend_span(stops, line, clipped.x, coverage, pixels);
                return;
            }
            let left = (self.inner.x - clipped.x).clamp(0, clipped.width) as usize;
            let right = (self.inner.x + self.inner.width - clipped.x).clamp(0, clipped.width) as usize;
            self.blend_span(stops, line, clipped.x, coverage, &mut pixels[..left]);
            P::blend_slice(&mut pixels[left..right], self.inner_color.coverage(coverage));
            self.blend_span(stops, line, clipped.x + right as i32, coverage, &mut pixels[right..]);
            return;
        }
        let position = self.offset
            + self.x_step * (clipped.x - self.geometry.x) as f32
            + self.y_step * (line - self.geometry.y) as f32;
        let mut sampler = GradientSampler::new(self, stops, position, clipped.x);
        draw_gradient_line(
            clipped,
            line,
            &RoundedGradient {
                radii: self.radii,
                border_width: self.border_width,
                inner_color: self.inner_color.coverage(coverage),
                top_clip: clipped.y - self.geometry.y,
                bottom_clip: self.geometry.y + self.geometry.height - clipped.y - clipped.height,
                left_clip: clipped.x - self.geometry.x,
                right_clip: self.geometry.x + self.geometry.width - clipped.x - clipped.width,
            },
            pixels,
            |x| sampler.sample(x).coverage(coverage),
        );
    }

    fn blend_span<P: Pixel>(
        &self,
        stops: &[GradientStop],
        line: i32,
        start: i32,
        coverage: u32,
        pixels: &mut [P],
    ) {
        let position = self.offset
            + self.x_step * (start - self.geometry.x) as f32
            + self.y_step * (line - self.geometry.y) as f32;
        let mut sampler = GradientSampler::new(self, stops, position, start);
        for (index, pixel) in pixels.iter_mut().enumerate() {
            let x = start + index as i32;
            let color = sampler.sample(x).coverage(coverage);
            pixel.blend(color);
        }
    }
}

struct GradientSampler<'a> {
    gradient: &'a Gradient,
    stops: &'a [GradientStop],
    position: f32,
    x: i32,
    index: usize,
    first: PremultipliedRgbaColor,
    second: PremultipliedRgbaColor,
    inverse_distance: f32,
}

impl<'a> GradientSampler<'a> {
    fn new(gradient: &'a Gradient, stops: &'a [GradientStop], position: f32, x: i32) -> Self {
        let index =
            stops.partition_point(|stop| stop.position <= position).saturating_sub(1).min(stops.len() - 2);
        let mut sampler = Self {
            gradient,
            stops,
            position,
            x,
            index,
            first: PremultipliedRgbaColor::default(),
            second: PremultipliedRgbaColor::default(),
            inverse_distance: 0.0,
        };
        sampler.update_colors();
        sampler
    }

    fn sample(&mut self, x: i32) -> PremultipliedRgbaColor {
        self.position += self.gradient.x_step * (x - self.x) as f32;
        self.x = x;
        while self.index + 1 < self.stops.len() - 1 && self.position >= self.stops[self.index + 1].position {
            self.index += 1;
            self.update_colors();
        }
        while self.index != 0 && self.position < self.stops[self.index].position {
            self.index -= 1;
            self.update_colors();
        }
        let amount =
            ((self.position - self.stops[self.index].position) * self.inverse_distance).clamp(0.0, 1.0);
        interpolate((amount * 255.0).round() as u32, self.first, self.second)
    }

    fn update_colors(&mut self) {
        let first = self.stops[self.index];
        let second = self.stops[self.index + 1];
        self.first = prepare_border_color(first.color, self.gradient.opacity, self.gradient.inner_color);
        self.second = prepare_border_color(second.color, self.gradient.opacity, self.gradient.inner_color);
        self.inverse_distance = 1.0 / (second.position - first.position);
    }
}

fn prepare_border_color(
    color: blit::color::Color,
    opacity: u8,
    inner: PremultipliedRgbaColor,
) -> PremultipliedRgbaColor {
    let border = PremultipliedRgbaColor::new(color, opacity);
    let inverse = 255 - border.alpha as u16;
    PremultipliedRgbaColor {
        red: (inner.red as u16 * inverse / 255) as u8 + border.red,
        green: (inner.green as u16 * inverse / 255) as u8 + border.green,
        blue: (inner.blue as u16 * inverse / 255) as u8 + border.blue,
        alpha: (inner.alpha as u16 + border.alpha as u16 - inner.alpha as u16 * border.alpha as u16 / 255)
            as u8,
    }
}
