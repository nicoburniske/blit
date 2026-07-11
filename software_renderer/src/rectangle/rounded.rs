use std::ops::{Add, Mul, Sub};

use bullseye::PhysicalRect;

use crate::{Pixel, PremultipliedRgbaColor};

#[derive(Clone, Copy)]
pub struct Radii {
    pub top_left: i32,
    pub top_right: i32,
    pub bottom_right: i32,
    pub bottom_left: i32,
}

impl Radii {
    pub fn is_zero(self) -> bool {
        self.top_left == 0 && self.top_right == 0 && self.bottom_right == 0 && self.bottom_left == 0
    }

    pub fn fit(mut self, width: i32, height: i32) -> Self {
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

pub struct RoundedRectangle {
    pub radii: Radii,
    pub border_width: i32,
    pub border_color: PremultipliedRgbaColor,
    pub inner_color: PremultipliedRgbaColor,
    pub left_clip: i32,
    pub right_clip: i32,
    pub top_clip: i32,
    pub bottom_clip: i32,
}

pub fn draw_line<P: Pixel>(
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
