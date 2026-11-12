use std::f32::consts::TAU;

use blit::{HasFrameStore, Paint};
use blit_gui::mesh;

use super::*;

#[derive(Default)]
pub struct View;

impl View {
    pub const TITLE: &str = "knot";

    pub fn build(&mut self, mut ui: Ui<'_>, phase: f32) -> bool {
        ui.insert(Knot { phase });
        false
    }
}

struct Knot {
    phase: f32,
}

impl Atom<DesktopContext> for Knot {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        let turn = self.phase * TAU;
        let (yaw_sine, yaw_cosine) = (turn + 0.35).sin_cos();
        let (tilt_sine, tilt_cosine) = (0.55 + turn.sin() * 0.2).sin_cos();
        let rotate = |[x, y, z]: [f32; 3]| {
            let depth = z * yaw_cosine - x * yaw_sine;
            [
                x * yaw_cosine + z * yaw_sine,
                y * tilt_cosine - depth * tilt_sine,
                y * tilt_sine + depth * tilt_cosine,
            ]
        };
        let normalize = |vector: [f32; 3]| {
            let length = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
            vector.map(|value| value / length)
        };
        let circle = std::array::from_fn::<_, 10, _>(|side| (side as f32 / 10.0 * TAU).sin_cos());
        let mut points = cx.scratch().slice(80 * 10);
        for segment in 0..80 {
            let t = segment as f32 / 80.0 * TAU;
            let (sine, cosine) = (t * 2.0).sin_cos();
            let (twist_sine, twist_cosine) = (t * 3.0).sin_cos();
            let radius = 0.82 + twist_cosine * 0.24;
            let center = [cosine * radius, sine * radius, twist_sine * 0.34];
            let slope = -twist_sine * 0.72;
            let tangent = normalize([
                cosine * slope - sine * radius * 2.0,
                sine * slope + cosine * radius * 2.0,
                twist_cosine * 1.02,
            ]);
            let dot = tangent[0] * cosine + tangent[1] * sine;
            let normal = normalize([cosine - tangent[0] * dot, sine - tangent[1] * dot, -tangent[2] * dot]);
            let binormal = [
                tangent[1] * normal[2] - tangent[2] * normal[1],
                tangent[2] * normal[0] - tangent[0] * normal[2],
                tangent[0] * normal[1] - tangent[1] * normal[0],
            ];
            for (sine, cosine) in circle {
                let normal = std::array::from_fn(|i| normal[i] * cosine + binormal[i] * sine);
                let position = std::array::from_fn(|i| center[i] + normal[i] * 0.145);
                points.push((rotate(position), rotate(normal)));
            }
        }
        let triangle = |index: usize| {
            let segment = index / 20;
            let side = index / 2 % 10;
            let a = segment * 10 + side;
            let b = (segment + 1) % 80 * 10 + side;
            let c = segment * 10 + (side + 1) % 10;
            let d = (segment + 1) % 80 * 10 + (side + 1) % 10;
            if index.is_multiple_of(2) { [a, c, b] } else { [b, c, d] }
        };
        let mut order = cx.scratch().slice(80 * 10 * 2);
        for index in 0..80 * 10 * 2 {
            let [a, b, c] = triangle(index).map(|index| points[index].0);
            let facing = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if facing > 0.0 {
                order.push((index, a[2] + b[2] + c[2]));
            }
        }
        order.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
        let size = area.width.min(area.height) * 0.33;
        let context = cx.context_mut();
        let mut vertices = context.store_mut().slice(points.len());
        vertices.extend(points.iter().map(|&(position, normal)| {
            let diffuse = (-normal[0] * 0.45 + normal[1] * 0.6 + normal[2] * 0.7).max(0.0);
            let specular = (-normal[0] * 0.24 + normal[1] * 0.32 + normal[2] * 0.9165)
                .max(0.0)
                .powi(32);
            let rim = (1.0 - normal[2].max(0.0)).powi(3);
            let rgb: [u8; 3] = std::array::from_fn(|i| {
                ([212.0, 146.0, 62.0][i] * (0.2 + diffuse * 0.7)
                    + [245.0, 225.0, 180.0][i] * specular
                    + [24.0, 40.0, 48.0][i] * rim) as u8
            });
            MeshVertex::new(
                area.x + area.width * 0.5 + position[0] * size,
                area.y + area.height * 0.46 - position[1] * size,
                Color::rgb(rgb[0], rgb[1], rgb[2]),
            )
        }));
        let vertices = vertices.finish();
        let mut indices = context.store_mut().slice(order.len() * 3);
        indices.extend((0..order.len() * 3).map(|index| triangle(order[index / 3].0)[index % 3] as u32));
        let indices = indices.finish();
        context.paint(area, mesh::Stored { vertices, indices });
    }
}
