use blit::PhysicalRect;
use blit_gui::style::BorderRadius;

use crate::math;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct Radii {
    pub top_left: i32,
    pub top_right: i32,
    pub bottom_right: i32,
    pub bottom_left: i32,
}

impl Radii {
    #[inline]
    pub fn new(radius: BorderRadius, scale_factor: f32, width: i32, height: i32) -> Self {
        let mut radii = Self {
            top_left: math::roundf(radius.top_left * scale_factor).max(0.0) as i32,
            top_right: math::roundf(radius.top_right * scale_factor).max(0.0) as i32,
            bottom_right: math::roundf(radius.bottom_right * scale_factor).max(0.0) as i32,
            bottom_left: math::roundf(radius.bottom_left * scale_factor).max(0.0) as i32,
        };
        let scale = 1.0f32
            .min(width as f32 / (radii.top_left + radii.top_right).max(1) as f32)
            .min(width as f32 / (radii.bottom_left + radii.bottom_right).max(1) as f32)
            .min(height as f32 / (radii.top_left + radii.bottom_left).max(1) as f32)
            .min(height as f32 / (radii.top_right + radii.bottom_right).max(1) as f32);
        radii.top_left = math::roundf(radii.top_left as f32 * scale) as i32;
        radii.top_right = math::roundf(radii.top_right as f32 * scale) as i32;
        radii.bottom_right = math::roundf(radii.bottom_right as f32 * scale) as i32;
        radii.bottom_left = math::roundf(radii.bottom_left as f32 * scale) as i32;
        radii
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        self.top_left == 0 && self.top_right == 0 && self.bottom_right == 0 && self.bottom_left == 0
    }
}

#[derive(Clone, Copy)]
pub struct RoundedLine {
    x: i32,
    width: i32,
    left: Corner,
    right: Corner,
}

impl RoundedLine {
    /// first row where a corner starts or ends so each side only narrows or widens before it
    pub fn zone_until(area: PhysicalRect, radii: Radii, line: i32) -> i32 {
        let end = area.y.saturating_add(area.height);
        let mut next = i32::MAX;
        for row in [
            area.y,
            area.y.saturating_add(radii.top_left),
            area.y.saturating_add(radii.top_right),
            end.saturating_sub(radii.bottom_left),
            end.saturating_sub(radii.bottom_right),
            end,
        ] {
            if row > line {
                next = next.min(row);
            }
        }
        next
    }

    #[inline]
    pub fn new(area: PhysicalRect, radii: Radii, line: i32) -> Option<Self> {
        if area.width <= 0 || area.height <= 0 {
            return None;
        }
        let top = line - area.y;
        if top < 0 || top >= area.height {
            return None;
        }
        let bottom = area.height - top - 1;
        let left = if top < radii.top_left {
            Corner::new(radii.top_left, top - radii.top_left)
        } else if bottom < radii.bottom_left {
            Corner::new(radii.bottom_left, radii.bottom_left - bottom - 1)
        } else {
            Corner::default()
        };
        let right = if radii.top_right == radii.top_left && radii.bottom_right == radii.bottom_left {
            left
        } else if top < radii.top_right {
            Corner::new(radii.top_right, top - radii.top_right)
        } else if bottom < radii.bottom_right {
            Corner::new(radii.bottom_right, radii.bottom_right - bottom - 1)
        } else {
            Corner::default()
        };
        Some(Self {
            x: area.x,
            width: area.width,
            left,
            right,
        })
    }

    #[inline]
    pub fn visible_start(&self) -> i32 {
        self.x.saturating_add(math::floorf(self.left.start) as i32)
    }

    #[inline]
    pub fn visible_end(&self) -> i32 {
        self.x
            .saturating_add(self.width.saturating_sub(math::floorf(self.right.start) as i32))
    }

    #[inline]
    pub fn full_start(&self) -> i32 {
        self.x.saturating_add(math::ceilf(self.left.end) as i32)
    }

    #[inline]
    pub fn full_end(&self) -> i32 {
        self.x
            .saturating_add(self.width.saturating_sub(math::ceilf(self.right.end) as i32))
    }

    #[inline]
    pub fn coverage(&self, x: i32) -> u8 {
        let x = x - self.x;
        if x < 0 || x >= self.width {
            return 0;
        }
        let right_x = self.width - x - 1;
        if (x + 1) as f32 <= self.left.start || (right_x + 1) as f32 <= self.right.start {
            return 0;
        }
        let coverage = match ((x as f32) < self.left.end, (right_x as f32) < self.right.end) {
            (false, false) => return 255,
            (true, false) => self.left.coverage(x),
            (false, true) => self.right.coverage(right_x),
            (true, true) => {
                let left = (x + 1) as f32;
                let right = (right_x + 1) as f32;
                self.left
                    .edges
                    .iter()
                    .zip(&self.right.edges)
                    .map(|(l, r)| ((left - l).clamp(0.0, 1.0) + (right - r).clamp(0.0, 1.0) - 1.0).max(0.0))
                    .sum()
            }
        };
        (coverage * (255.0 / 8.0) + 0.5) as u8
    }
}

#[derive(Clone, Copy, Default)]
struct Corner {
    radius: f32,
    row: f32,
    start: f32,
    end: f32,
    edges: [f32; 8],
}

impl Corner {
    #[inline]
    fn new(radius: i32, y: i32) -> Self {
        let radius = radius as f32;
        let y = y as f32;
        let [first, second] = [y, y + 1.0].map(|y| radius - math::sqrtf((radius * radius - y * y).max(0.0)));
        Self {
            radius,
            row: radius - (y + 0.5).abs() - 0.5,
            start: first.min(second),
            end: first.max(second),
            edges: outer_edges(radius, y),
        }
    }

    #[inline]
    fn coverage(&self, x: i32) -> f32 {
        let x = x as f32;
        let (x, edges) = if x > self.row {
            (self.row, outer_edges(self.radius, x - self.radius))
        } else {
            (x, self.edges)
        };
        edges.iter().map(|edge| (x + 1.0 - edge).clamp(0.0, 1.0)).sum()
    }
}

#[inline]
fn outer_edges(radius: f32, y: f32) -> [f32; 8] {
    let mut edges = [0.0625, 0.1875, 0.3125, 0.4375, 0.5625, 0.6875, 0.8125, 0.9375];
    for edge in &mut edges {
        let y = y + *edge;
        *edge = radius - math::sqrtf((radius * radius - y * y).max(0.0));
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_tangents_preserve_border_weight() {
        let area = PhysicalRect {
            x: 0,
            y: 0,
            width: 64,
            height: 48,
        };
        for radius in [8, 12, 20] {
            let radii = Radii::new(BorderRadius::all(radius as f32), 1.0, area.width, area.height);
            let top = RoundedLine::new(area, radii, 0).unwrap();
            let side = RoundedLine::new(area, radii, radius - 1).unwrap();
            assert!(top.coverage(radius - 1) >= 249);
            assert!(side.coverage(0) >= 249);
            assert_eq!(top.coverage(radius - 1), side.coverage(0));
            assert_eq!(top.coverage(radius), 255);
            assert_eq!(RoundedLine::new(area, radii, radius).unwrap().coverage(0), 255);
        }
    }
}
