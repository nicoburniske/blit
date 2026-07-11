use std::ops::{Add, Mul, Sub};

use bullseye::{PhysicalRect, widgets::Rectangle};

use crate::{Pixel, PixelBuffer, PixelSpan, PremultipliedRgbaColor};

#[derive(Clone, Copy)]
pub struct PreparedRectangle {
    geometry: PhysicalRect,
    inner: PhysicalRect,
    radii: Radii,
    border_width: i32,
    border_color: PremultipliedRgbaColor,
    inner_color: PremultipliedRgbaColor,
}

impl PreparedRectangle {
    pub fn new(rectangle: &Rectangle, scale_factor: f32) -> Option<Self> {
        let geometry = rectangle.area.to_physical(scale_factor);
        if geometry.width <= 0 || geometry.height <= 0 || rectangle.opacity <= 0.0 {
            return None;
        }
        let mut border_width = (rectangle.border_width * scale_factor).round().max(0.0) as i32;
        let inner_color =
            PremultipliedRgbaColor::with_opacity(rectangle.background, rectangle.opacity);
        let mut border_color =
            PremultipliedRgbaColor::with_opacity(rectangle.border_color, rectangle.opacity);
        if border_color.alpha == 0 {
            border_width = 0;
        } else if border_color.alpha < 255 {
            let border_alpha = border_color.alpha as u16;
            border_color = PremultipliedRgbaColor {
                red: ((inner_color.red as u16 * (255 - border_alpha)) / 255) as u8
                    + border_color.red,
                green: ((inner_color.green as u16 * (255 - border_alpha)) / 255) as u8
                    + border_color.green,
                blue: ((inner_color.blue as u16 * (255 - border_alpha)) / 255) as u8
                    + border_color.blue,
                alpha: (inner_color.alpha as u16 + border_alpha
                    - (inner_color.alpha as u16 * border_alpha) / 255) as u8,
            };
        }
        let radii = Radii {
            top_left: (rectangle.radius.top_left * scale_factor).round().max(0.0) as i32,
            top_right: (rectangle.radius.top_right * scale_factor).round().max(0.0) as i32,
            bottom_right: (rectangle.radius.bottom_right * scale_factor)
                .round()
                .max(0.0) as i32,
            bottom_left: (rectangle.radius.bottom_left * scale_factor)
                .round()
                .max(0.0) as i32,
        }
        .fit(geometry.width, geometry.height);
        let inner = PhysicalRect {
            x: geometry.x + border_width,
            y: geometry.y + border_width,
            width: (geometry.width - border_width * 2).max(0),
            height: (geometry.height - border_width * 2).max(0),
        };
        Some(Self {
            geometry,
            inner,
            radii,
            border_width,
            border_color,
            inner_color,
        })
    }

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
        let pixels = &mut row.pixels[(clipped.x - row.x) as usize..][..clipped.width as usize];
        if self.radii.is_zero() {
            if self.border_width == 0
                || line < self.inner.y
                || line >= self.inner.y + self.inner.height
            {
                P::blend_slice(
                    pixels,
                    if self.border_width == 0 {
                        self.inner_color
                    } else {
                        self.border_color
                    },
                );
                return;
            }
            let left = (self.inner.x - clipped.x).clamp(0, clipped.width) as usize;
            let right =
                (self.inner.x + self.inner.width - clipped.x).clamp(0, clipped.width) as usize;
            P::blend_slice(&mut pixels[..left], self.border_color);
            P::blend_slice(&mut pixels[left..right], self.inner_color);
            P::blend_slice(&mut pixels[right..], self.border_color);
            return;
        }
        draw_rounded_line(
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

pub fn draw<B: PixelBuffer>(
    buffer: &mut B,
    rectangle: &Rectangle,
    clips: &[PhysicalRect],
    scale_factor: f32,
) {
    let Some(rectangle) = PreparedRectangle::new(rectangle, scale_factor) else {
        return;
    };
    let screen = PhysicalRect {
        x: 0,
        y: 0,
        width: buffer.width() as i32,
        height: buffer.height() as i32,
    };
    for clip in clips {
        let Some(clipped) = rectangle
            .geometry
            .intersection(*clip)
            .and_then(|area| area.intersection(screen))
        else {
            continue;
        };
        for y in clipped.y..clipped.y + clipped.height {
            let row = buffer.line_mut(y as usize);
            rectangle.draw_line(y, *clip, PixelSpan { x: 0, pixels: row });
        }
    }
}

#[derive(Clone, Copy)]
struct Radii {
    top_left: i32,
    top_right: i32,
    bottom_right: i32,
    bottom_left: i32,
}

impl Radii {
    fn is_zero(self) -> bool {
        self.top_left == 0 && self.top_right == 0 && self.bottom_right == 0 && self.bottom_left == 0
    }

    fn fit(mut self, width: i32, height: i32) -> Self {
        let scale = 1.0f32
            .min(width as f32 / (self.top_left + self.top_right).max(1) as f32)
            .min(width as f32 / (self.bottom_left + self.bottom_right).max(1) as f32)
            .min(height as f32 / (self.top_left + self.bottom_left).max(1) as f32)
            .min(height as f32 / (self.top_right + self.bottom_right).max(1) as f32);
        self.top_left = (self.top_left as f32 * scale).round() as i32;
        self.top_right = (self.top_right as f32 * scale).round() as i32;
        self.bottom_right = (self.bottom_right as f32 * scale).round() as i32;
        self.bottom_left = (self.bottom_left as f32 * scale).round() as i32;
        self
    }
}

struct RoundedRectangle {
    radii: Radii,
    border_width: i32,
    border_color: PremultipliedRgbaColor,
    inner_color: PremultipliedRgbaColor,
    left_clip: i32,
    right_clip: i32,
    top_clip: i32,
    bottom_clip: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Shifted(u32);

impl Add for Shifted {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Shifted {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl Mul for Shifted {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self(self.0 * other.0)
    }
}

impl Shifted {
    const ONE: Self = Self(1 << 4);
    const ZERO: Self = Self(0);

    fn new(value: i32) -> Self {
        Self((value.max(0) as u32) << 4)
    }

    fn floor(self) -> u32 {
        self.0 >> 4
    }

    fn ceil(self) -> u32 {
        (self.0 + Self::ONE.0 - 1) >> 4
    }

    fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    fn sqrt(self) -> Self {
        Self(self.0.isqrt())
    }
}

fn draw_rounded_line<P: Pixel>(
    span: PhysicalRect,
    line: i32,
    rounded: &RoundedRectangle,
    row: &mut [P],
) {
    let width = row.len();
    let y1 = line - span.y + rounded.top_clip;
    let y2 = span.y + span.height - line + rounded.bottom_clip - 1;
    let y = y1.min(y2);
    let border = Shifted::new(rounded.border_width);
    let anti_alias = |x1: Shifted, x2: Shifted, process_pixel: &mut dyn FnMut(usize, u32)| {
        for x in x1.floor()..x2.ceil() {
            let coverage =
                ((Shifted::ONE + Shifted::new(x as i32) - x1).0 << 8) / (Shifted::ONE + x2 - x1).0;
            process_pixel(x as usize, coverage.min(255));
        }
    };
    let reverse = |x: Shifted| (Shifted::new(width as i32 + rounded.right_clip)).saturating_sub(x);
    let calculate_edges = |radius: i32, y: i32| {
        let radius = Shifted::new(radius);
        let y = radius - Shifted::new(y);
        let x2 = radius - (radius * radius).saturating_sub(y * y).sqrt();
        let x1 = radius
            - (radius * radius)
                .saturating_sub((y - Shifted::ONE) * (y - Shifted::ONE))
                .sqrt();
        let inner_radius = radius.saturating_sub(border);
        let x4 = radius - (inner_radius * inner_radius).saturating_sub(y * y).sqrt();
        let x3 = radius
            - (inner_radius * inner_radius)
                .saturating_sub((y - Shifted::ONE) * (y - Shifted::ONE))
                .sqrt();
        (x1, x2, x3, x4)
    };

    let (x1, x2, x3, x4) = if y1 < rounded.radii.top_left {
        calculate_edges(rounded.radii.top_left, y)
    } else if y2 < rounded.radii.bottom_left {
        calculate_edges(rounded.radii.bottom_left, y)
    } else {
        (Shifted::ZERO, Shifted::ZERO, border, border)
    };
    let (x5, x6, x7, x8) = if y1 < rounded.radii.top_right {
        let x = calculate_edges(rounded.radii.top_right, y);
        (x.3, x.2, x.1, x.0)
    } else if y2 < rounded.radii.bottom_right {
        let x = calculate_edges(rounded.radii.bottom_right, y);
        (x.3, x.2, x.1, x.0)
    } else {
        (border, border, Shifted::ZERO, Shifted::ZERO)
    };
    let (x5, x6, x7, x8) = (reverse(x5), reverse(x6), reverse(x7), reverse(x8));
    let left_clip = Shifted::new(rounded.left_clip);

    anti_alias(
        x1.saturating_sub(left_clip),
        x2.saturating_sub(left_clip),
        &mut |x, coverage| {
            if x < width {
                let color = if border == Shifted::ZERO {
                    rounded.inner_color
                } else {
                    rounded.border_color
                };
                row[x].blend(color.coverage(coverage));
            }
        },
    );
    if y < rounded.border_width {
        let left = x2
            .ceil()
            .saturating_sub(rounded.left_clip as u32)
            .min(width as u32) as usize;
        let right = x7.floor().min(width as u32) as usize;
        if left < right {
            P::blend_slice(&mut row[left..right], rounded.border_color);
        }
    } else {
        if border > Shifted::ZERO {
            if Shifted::ONE + x2 <= x3 {
                let left = x2
                    .ceil()
                    .saturating_sub(rounded.left_clip as u32)
                    .min(width as u32) as usize;
                let right = x3
                    .floor()
                    .saturating_sub(rounded.left_clip as u32)
                    .min(width as u32) as usize;
                if left < right {
                    P::blend_slice(&mut row[left..right], rounded.border_color);
                }
            }
            anti_alias(
                x3.saturating_sub(left_clip),
                x4.saturating_sub(left_clip),
                &mut |x, coverage| {
                    if x < width {
                        row[x].blend(interpolate(
                            coverage,
                            rounded.border_color,
                            rounded.inner_color,
                        ));
                    }
                },
            );
        }
        let left = x4
            .ceil()
            .saturating_sub(rounded.left_clip as u32)
            .min(width as u32) as usize;
        let right = x5.floor().min(width as u32) as usize;
        if left < right {
            P::blend_slice(&mut row[left..right], rounded.inner_color);
        }
        if border > Shifted::ZERO {
            anti_alias(x5, x6, &mut |x, coverage| {
                if x < width {
                    row[x].blend(interpolate(
                        coverage,
                        rounded.inner_color,
                        rounded.border_color,
                    ));
                }
            });
            if Shifted::ONE + x6 <= x7 {
                let left = x6.ceil().min(width as u32) as usize;
                let right = x7.floor().min(width as u32) as usize;
                if left < right {
                    P::blend_slice(&mut row[left..right], rounded.border_color);
                }
            }
        }
    }
    anti_alias(x7, x8, &mut |x, coverage| {
        if x < width {
            let color = if border == Shifted::ZERO {
                rounded.inner_color
            } else {
                rounded.border_color
            };
            row[x].blend(color.coverage(255 - coverage));
        }
    });
}

fn interpolate(
    amount: u32,
    first: PremultipliedRgbaColor,
    second: PremultipliedRgbaColor,
) -> PremultipliedRgbaColor {
    let inverse = 255 - amount;
    PremultipliedRgbaColor {
        alpha: ((inverse * first.alpha as u32 + amount * second.alpha as u32) / 255) as u8,
        red: ((inverse * first.red as u32 + amount * second.red as u32) / 255) as u8,
        green: ((inverse * first.green as u32 + amount * second.green as u32) / 255) as u8,
        blue: ((inverse * first.blue as u32 + amount * second.blue as u32) / 255) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VecBuffer;
    use bullseye::{Color, LogicalRect, widgets::BorderRadius};

    #[test]
    fn rounded_edges_have_partial_coverage() {
        let mut buffer = VecBuffer::<u32>::new(16, 16);
        draw(
            &mut buffer,
            &Rectangle {
                area: LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: 16.0,
                    height: 16.0,
                },
                background: Color::WHITE,
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
                radius: BorderRadius {
                    top_left: 8.0,
                    top_right: 8.0,
                    bottom_right: 8.0,
                    bottom_left: 8.0,
                },
                opacity: 1.0,
            },
            &[PhysicalRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            }],
            1.0,
        );

        assert_eq!(buffer.pixels()[0], 0);
        assert_ne!(buffer.pixels()[7], 0);
        assert_ne!(buffer.pixels()[7], 0x00ff_ffff);
        assert_eq!(buffer.pixels()[8 * 16 + 8], 0x00ff_ffff);
    }

    #[test]
    fn clipping_does_not_touch_other_pixels() {
        let mut buffer = VecBuffer::<u32>::new(8, 8);
        draw(
            &mut buffer,
            &Rectangle::new(LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 8.0,
                height: 8.0,
            })
            .background(Color::WHITE),
            &[PhysicalRect {
                x: 2,
                y: 3,
                width: 2,
                height: 1,
            }],
            1.0,
        );

        assert_eq!(
            buffer.pixels().iter().filter(|pixel| **pixel != 0).count(),
            2
        );
    }

    #[test]
    fn corner_radii_are_independent() {
        let mut buffer = VecBuffer::<u32>::new(12, 12);
        draw(
            &mut buffer,
            &Rectangle {
                area: LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: 12.0,
                    height: 12.0,
                },
                background: Color::WHITE,
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
                radius: BorderRadius {
                    top_left: 6.0,
                    top_right: 0.0,
                    bottom_right: 0.0,
                    bottom_left: 0.0,
                },
                opacity: 1.0,
            },
            &[PhysicalRect {
                x: 0,
                y: 0,
                width: 12,
                height: 12,
            }],
            1.0,
        );

        assert_eq!(buffer.pixels()[0], 0);
        assert_eq!(buffer.pixels()[11], 0x00ff_ffff);
        assert_eq!(buffer.pixels()[11 * 12], 0x00ff_ffff);
        assert_eq!(buffer.pixels()[12 * 12 - 1], 0x00ff_ffff);
    }

    #[test]
    fn rounded_border_keeps_separate_border_and_inner_spans() {
        let mut buffer = VecBuffer::<u32>::new(16, 16);
        draw(
            &mut buffer,
            &Rectangle::new(LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 16.0,
                height: 16.0,
            })
            .background(Color::from_rgba8(0, 255, 0, 255))
            .border(2.0, Color::from_rgba8(255, 0, 0, 255))
            .uniform_radius(6.0),
            &[PhysicalRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            }],
            1.0,
        );

        assert_eq!(buffer.pixels()[8], 0x00ff_0000);
        assert_eq!(buffer.pixels()[8 * 16 + 8], 0x0000_ff00);
    }
}
