use blit::{PhysicalRect, widgets::Rectangle};

use crate::{Pixel, PixelSpan, PremultipliedRgbaColor};

use super::rounded::{Radii, RoundedRectangle, draw_line};

#[derive(Clone, Copy)]
pub struct Prepared {
    pub geometry: PhysicalRect,
    inner: PhysicalRect,
    radii: Radii,
    border_width: i32,
    border_color: PremultipliedRgbaColor,
    inner_color: PremultipliedRgbaColor,
}

impl Prepared {
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
        if inner_color.alpha == 0 && border_width == 0 {
            return None;
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
