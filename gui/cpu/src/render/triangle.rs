use blit::{PhysicalRect, Scale2};
use blit_gui::display_list::MeshVertex;

use crate::{Pixel, PixelSpan, PremultipliedRgbaColor};

#[derive(Clone, Copy)]
pub struct Prepared {
    points: [[f32; 2]; 3],
    color: PremultipliedRgbaColor,
    color_step_x: [f32; 4],
    color_step_y: [f32; 4],
    color_kind: ColorKind,
    inclusive: [bool; 3],
    pub bounds: PhysicalRect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorKind {
    Solid,
    Opaque,
    Translucent,
}

impl Prepared {
    pub fn new(vertices: [MeshVertex; 3], scale: Scale2) -> Option<Self> {
        let mut points =
            vertices.map(|vertex| [vertex.position.x * scale.x, vertex.position.y * scale.y]);
        if !points.iter().flatten().all(|value| value.is_finite()) {
            return None;
        }
        let mut colors = vertices.map(|vertex| PremultipliedRgbaColor::new(vertex.color, 255));
        let mut area = (points[1][0] - points[0][0]) * (points[2][1] - points[0][1])
            - (points[1][1] - points[0][1]) * (points[2][0] - points[0][0]);
        if area == 0.0 || !area.is_finite() {
            return None;
        }
        if area < 0.0 {
            points.swap(1, 2);
            colors.swap(1, 2);
            area = -area;
        }
        let inverse_area = area.recip();
        if !inverse_area.is_finite() {
            return None;
        }
        let color_kind = if colors[0] == colors[1] && colors[0] == colors[2] {
            ColorKind::Solid
        } else if colors.iter().all(|color| color.alpha == 255) {
            ColorKind::Opaque
        } else {
            ColorKind::Translucent
        };
        let components = colors.map(|color| {
            [
                color.red as f32,
                color.green as f32,
                color.blue as f32,
                color.alpha as f32,
            ]
        });
        let second_color = std::array::from_fn::<_, 4, _>(|channel| {
            components[1][channel] - components[0][channel]
        });
        let third_color = std::array::from_fn::<_, 4, _>(|channel| {
            components[2][channel] - components[0][channel]
        });
        let second = [points[1][0] - points[0][0], points[1][1] - points[0][1]];
        let third = [points[2][0] - points[0][0], points[2][1] - points[0][1]];
        let color_step_x = std::array::from_fn(|channel| {
            (second_color[channel] * third[1] - third_color[channel] * second[1]) * inverse_area
        });
        let color_step_y = std::array::from_fn(|channel| {
            (third_color[channel] * second[0] - second_color[channel] * third[0]) * inverse_area
        });
        let inclusive = |start: [f32; 2], end: [f32; 2]| {
            start[1] < end[1] || (start[1] == end[1] && start[0] < end[0])
        };
        let inclusive = [
            inclusive(points[1], points[2]),
            inclusive(points[2], points[0]),
            inclusive(points[0], points[1]),
        ];
        let [first, second, third] = points;
        let left = first[0].min(second[0]).min(third[0]).floor() as i32;
        let top = first[1].min(second[1]).min(third[1]).floor() as i32;
        let right = first[0].max(second[0]).max(third[0]).ceil() as i32;
        let bottom = first[1].max(second[1]).max(third[1]).ceil() as i32;
        let bounds = PhysicalRect {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        };
        (bounds.width > 0 && bounds.height > 0).then_some(Self {
            points,
            color: colors[0],
            color_step_x,
            color_step_y,
            color_kind,
            inclusive,
            bounds,
        })
    }

    pub fn draw_line<P: Pixel>(
        &self,
        line: i32,
        clip: PhysicalRect,
        coverage: u8,
        row: PixelSpan<'_, P>,
    ) {
        let [first, second, third] = self.points;
        let y = line as f32 + 0.5;
        let x = clip.x as f32 + 0.5;
        let edge = |start: [f32; 2], end: [f32; 2]| {
            (end[0] - start[0]) * (y - start[1]) - (end[1] - start[1]) * (x - start[0])
        };
        let edges = [edge(second, third), edge(third, first), edge(first, second)];
        let steps = [
            second[1] - third[1],
            third[1] - first[1],
            first[1] - second[1],
        ];
        let covered = |offset: i32| {
            let offset = offset as f32;
            (0..3).all(|index| {
                let edge = edges[index] + steps[index] * offset;
                edge > 0.0 || (edge == 0.0 && self.inclusive[index])
            })
        };
        let mut left = 0;
        let mut right = clip.width;
        for index in 0..3 {
            if steps[index] > 0.0 {
                left = left.max(((-edges[index] / steps[index]) as i32).saturating_sub(1));
            } else if steps[index] < 0.0 {
                right = right.min(((-edges[index] / steps[index]) as i32).saturating_add(2));
            } else if !(edges[index] > 0.0 || (edges[index] == 0.0 && self.inclusive[index])) {
                return;
            }
        }
        left = left.clamp(0, clip.width);
        right = right.clamp(left, clip.width);
        while left < right && !covered(left) {
            left += 1;
        }
        while left < right && !covered(right - 1) {
            right -= 1;
        }
        if left == right {
            return;
        }
        let start = clip.x.saturating_sub(row.x) as usize + left as usize;
        let pixels = &mut row.pixels[start..start + (right - left) as usize];
        if self.color_kind == ColorKind::Solid {
            P::blend_slice(
                pixels,
                if coverage == 255 {
                    self.color
                } else {
                    self.color.coverage(coverage as u32)
                },
            );
            return;
        }
        let base = [
            self.color.red as f32,
            self.color.green as f32,
            self.color.blue as f32,
            self.color.alpha as f32,
        ];
        let offset_x = x + left as f32 - first[0];
        let offset_y = y - first[1];
        let mut color = std::array::from_fn::<_, 4, _>(|channel| {
            base[channel]
                + self.color_step_x[channel] * offset_x
                + self.color_step_y[channel] * offset_y
        });
        if self.color_kind == ColorKind::Opaque && coverage == 255 {
            P::write_opaque_gradient(
                pixels,
                [color[0], color[1], color[2]],
                [
                    self.color_step_x[0],
                    self.color_step_x[1],
                    self.color_step_x[2],
                ],
            );
            return;
        }
        for pixel in pixels {
            let sample = PremultipliedRgbaColor {
                red: (color[0] + 0.5) as u8,
                green: (color[1] + 0.5) as u8,
                blue: (color[2] + 0.5) as u8,
                alpha: (color[3] + 0.5) as u8,
            };
            pixel.blend(if coverage == 255 {
                sample
            } else {
                sample.coverage(coverage as u32)
            });
            for channel in 0..4 {
                color[channel] += self.color_step_x[channel];
            }
        }
    }
}
