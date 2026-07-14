use std::ops::{Add, Mul, Sub};

use blit::{PhysicalRect, widgets::BorderRadius};

use crate::{Pixel, PremultipliedRgbaColor};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Radii {
    pub top_left: i32,
    pub top_right: i32,
    pub bottom_right: i32,
    pub bottom_left: i32,
}

impl Radii {
    pub fn new(radius: BorderRadius, scale_factor: f32, width: i32, height: i32) -> Self {
        let mut radii = Self {
            top_left: (radius.top_left * scale_factor).round().max(0.0) as i32,
            top_right: (radius.top_right * scale_factor).round().max(0.0) as i32,
            bottom_right: (radius.bottom_right * scale_factor).round().max(0.0) as i32,
            bottom_left: (radius.bottom_left * scale_factor).round().max(0.0) as i32,
        };
        let scale = 1.0f32
            .min(width as f32 / (radii.top_left + radii.top_right).max(1) as f32)
            .min(width as f32 / (radii.bottom_left + radii.bottom_right).max(1) as f32)
            .min(height as f32 / (radii.top_left + radii.bottom_left).max(1) as f32)
            .min(height as f32 / (radii.top_right + radii.bottom_right).max(1) as f32);
        radii.top_left = (radii.top_left as f32 * scale).round() as i32;
        radii.top_right = (radii.top_right as f32 * scale).round() as i32;
        radii.bottom_right = (radii.bottom_right as f32 * scale).round() as i32;
        radii.bottom_left = (radii.bottom_left as f32 * scale).round() as i32;
        radii
    }

    pub fn is_zero(self) -> bool {
        self.top_left == 0 && self.top_right == 0 && self.bottom_right == 0 && self.bottom_left == 0
    }
}

#[derive(Clone, Copy)]
pub struct RoundedLine {
    x: i32,
    width: i32,
    left_start: Shifted,
    left_end: Shifted,
    right_start: Shifted,
    right_end: Shifted,
}

impl RoundedLine {
    pub fn new(area: PhysicalRect, radii: Radii, line: i32) -> Option<Self> {
        if area.width <= 0 || area.height <= 0 {
            return None;
        }
        let top = line - area.y;
        if top < 0 || top >= area.height {
            return None;
        }
        let bottom = area.height - top - 1;
        let (left_start, left_end) = if top < radii.top_left {
            outer_edges(radii.top_left, top)
        } else if bottom < radii.bottom_left {
            outer_edges(radii.bottom_left, bottom)
        } else {
            (Shifted::ZERO, Shifted::ZERO)
        };
        let (right_start, right_end) = if top < radii.top_right {
            outer_edges(radii.top_right, top)
        } else if bottom < radii.bottom_right {
            outer_edges(radii.bottom_right, bottom)
        } else {
            (Shifted::ZERO, Shifted::ZERO)
        };
        let width = Shifted::new(area.width);
        Some(Self {
            x: area.x,
            width: area.width,
            left_start,
            left_end,
            right_start: width.saturating_sub(right_end),
            right_end: width.saturating_sub(right_start),
        })
    }

    pub fn visible_start(self) -> i32 {
        self.x.saturating_add(self.left_start.floor() as i32)
    }

    pub fn visible_end(self) -> i32 {
        self.x
            .saturating_add(self.right_end.ceil().min(self.width as u32) as i32)
    }

    pub fn full_start(self) -> i32 {
        self.x.saturating_add(self.left_end.ceil() as i32)
    }

    pub fn full_end(self) -> i32 {
        self.x
            .saturating_add(self.right_start.floor().min(self.width as u32) as i32)
    }

    pub fn coverage(self, x: i32) -> u8 {
        let x = x - self.x;
        if x < 0 || x >= self.width {
            return 0;
        }
        let shifted = Shifted::new(x);
        let mut coverage = 255;
        if shifted < self.left_end {
            coverage = coverage.min(edge_coverage(self.left_start, self.left_end, x));
        }
        if shifted >= self.right_start {
            coverage = coverage.min(255 - edge_coverage(self.right_start, self.right_end, x));
        }
        coverage as u8
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

pub struct RoundedGradient {
    pub radii: Radii,
    pub border_width: i32,
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
    let (y, border, [x1, x2, x3, x4, x5, x6, x7, x8]) = line_edges(
        span,
        line,
        rounded.radii,
        rounded.border_width,
        [
            rounded.left_clip,
            rounded.right_clip,
            rounded.top_clip,
            rounded.bottom_clip,
        ],
        width,
    );
    let anti_alias = |x1: Shifted, x2: Shifted, process_pixel: &mut dyn FnMut(usize, u32)| {
        for x in x1.floor()..x2.ceil() {
            process_pixel(x as usize, edge_coverage(x1, x2, x as i32));
        }
    };

    anti_alias(x1, x2, &mut |x, coverage| {
        if x < width {
            let color = if border == Shifted::ZERO {
                rounded.inner_color
            } else {
                rounded.border_color
            };
            row[x].blend(color.coverage(coverage));
        }
    });
    if y < rounded.border_width {
        let left = x2.ceil().min(width as u32) as usize;
        let right = x7.floor().min(width as u32) as usize;
        if left < right {
            P::blend_slice(&mut row[left..right], rounded.border_color);
        }
    } else {
        if border > Shifted::ZERO {
            if Shifted::ONE + x2 <= x3 {
                let left = x2.ceil().min(width as u32) as usize;
                let right = x3.floor().min(width as u32) as usize;
                if left < right {
                    P::blend_slice(&mut row[left..right], rounded.border_color);
                }
            }
            anti_alias(x3, x4, &mut |x, coverage| {
                if x < width {
                    row[x].blend(interpolate(
                        coverage,
                        rounded.border_color,
                        rounded.inner_color,
                    ));
                }
            });
        }
        let left = x4.ceil().min(width as u32) as usize;
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

pub fn draw_gradient_line<P: Pixel>(
    span: PhysicalRect,
    line: i32,
    rounded: &RoundedGradient,
    row: &mut [P],
    mut border_color: impl FnMut(i32) -> PremultipliedRgbaColor,
) {
    let width = row.len();
    let (y, border, [x1, x2, x3, x4, x5, x6, x7, x8]) = line_edges(
        span,
        line,
        rounded.radii,
        rounded.border_width,
        [
            rounded.left_clip,
            rounded.right_clip,
            rounded.top_clip,
            rounded.bottom_clip,
        ],
        width,
    );
    for x in x1.floor()..x2.ceil() {
        let x = x as usize;
        if x < width {
            row[x].blend(border_color(span.x + x as i32).coverage(edge_coverage(x1, x2, x as i32)));
        }
    }
    if y < rounded.border_width {
        blend_gradient(row, span.x, x2, x7, &mut border_color);
    } else {
        if border > Shifted::ZERO {
            if Shifted::ONE + x2 <= x3 {
                blend_gradient(row, span.x, x2, x3, &mut border_color);
            }
            for x in x3.floor()..x4.ceil() {
                let x = x as usize;
                if x < width {
                    row[x].blend(interpolate(
                        edge_coverage(x3, x4, x as i32),
                        border_color(span.x + x as i32),
                        rounded.inner_color,
                    ));
                }
            }
        }
        let left = x4.ceil().min(width as u32) as usize;
        let right = x5.floor().min(width as u32) as usize;
        if left < right {
            P::blend_slice(&mut row[left..right], rounded.inner_color);
        }
        if border > Shifted::ZERO {
            for x in x5.floor()..x6.ceil() {
                let x = x as usize;
                if x < width {
                    row[x].blend(interpolate(
                        edge_coverage(x5, x6, x as i32),
                        rounded.inner_color,
                        border_color(span.x + x as i32),
                    ));
                }
            }
            if Shifted::ONE + x6 <= x7 {
                blend_gradient(row, span.x, x6, x7, &mut border_color);
            }
        }
    }
    for x in x7.floor()..x8.ceil() {
        let x = x as usize;
        if x < width {
            row[x].blend(
                border_color(span.x + x as i32).coverage(255 - edge_coverage(x7, x8, x as i32)),
            );
        }
    }
}

fn blend_gradient<P: Pixel>(
    row: &mut [P],
    span_x: i32,
    start: Shifted,
    end: Shifted,
    border_color: &mut impl FnMut(i32) -> PremultipliedRgbaColor,
) {
    let start = start.ceil().min(row.len() as u32) as usize;
    let end = end.floor().min(row.len() as u32) as usize;
    for (x, pixel) in row.iter_mut().enumerate().take(end).skip(start) {
        let color = border_color(span_x + x as i32);
        if color.alpha == 255 {
            *pixel = P::from_rgb(color.red, color.green, color.blue);
        } else {
            pixel.blend(color);
        }
    }
}

#[inline]
fn line_edges(
    span: PhysicalRect,
    line: i32,
    radii: Radii,
    border_width: i32,
    clips: [i32; 4],
    width: usize,
) -> (i32, Shifted, [Shifted; 8]) {
    let [left_clip, right_clip, top_clip, bottom_clip] = clips;
    let y1 = line - span.y + top_clip;
    let y2 = span.y + span.height - line + bottom_clip - 1;
    let y = y1.min(y2);
    let border = Shifted::new(border_width);
    let (x1, x2, x3, x4) = if y1 < radii.top_left {
        calculate_edges(radii.top_left, y1, border)
    } else if y2 < radii.bottom_left {
        calculate_edges(radii.bottom_left, y2, border)
    } else {
        (Shifted::ZERO, Shifted::ZERO, border, border)
    };
    let (x5, x6, x7, x8) = if y1 < radii.top_right {
        let x = calculate_edges(radii.top_right, y1, border);
        (x.3, x.2, x.1, x.0)
    } else if y2 < radii.bottom_right {
        let x = calculate_edges(radii.bottom_right, y2, border);
        (x.3, x.2, x.1, x.0)
    } else {
        (border, border, Shifted::ZERO, Shifted::ZERO)
    };
    let reverse = |x: Shifted| (Shifted::new(width as i32 + right_clip)).saturating_sub(x);
    let left_clip = Shifted::new(left_clip);
    (
        y,
        border,
        [
            x1.saturating_sub(left_clip),
            x2.saturating_sub(left_clip),
            x3.saturating_sub(left_clip),
            x4.saturating_sub(left_clip),
            reverse(x5),
            reverse(x6),
            reverse(x7),
            reverse(x8),
        ],
    )
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

fn calculate_edges(radius: i32, y: i32, border: Shifted) -> (Shifted, Shifted, Shifted, Shifted) {
    let (x1, x2) = outer_edges(radius, y);
    let radius = Shifted::new(radius);
    let y = radius - Shifted::new(y);
    let inner_radius = radius.saturating_sub(border);
    let x4 = radius - (inner_radius * inner_radius).saturating_sub(y * y).sqrt();
    let x3 = radius
        - (inner_radius * inner_radius)
            .saturating_sub((y - Shifted::ONE) * (y - Shifted::ONE))
            .sqrt();
    (x1, x2, x3, x4)
}

fn outer_edges(radius: i32, y: i32) -> (Shifted, Shifted) {
    let radius = Shifted::new(radius);
    let y = radius - Shifted::new(y);
    let x2 = radius - (radius * radius).saturating_sub(y * y).sqrt();
    let x1 = radius
        - (radius * radius)
            .saturating_sub((y - Shifted::ONE) * (y - Shifted::ONE))
            .sqrt();
    (x1, x2)
}

fn edge_coverage(start: Shifted, end: Shifted, x: i32) -> u32 {
    if start == end {
        return 255;
    }
    (((Shifted::ONE + Shifted::new(x) - start).0 << 8) / (Shifted::ONE + end - start).0).min(255)
}

pub fn interpolate(
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
