use std::{cell::RefCell, rc::Rc};

use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, Size};
use blit_gui::{
    color::Color,
    mesh::{Mesh, MeshVertex},
};

use super::super::DesktopContext;

pub struct Sculpture {
    pub phase: f32,
    pub buffers: Rc<RefCell<SculptureBuffers>>,
}

#[derive(Default)]
pub struct SculptureBuffers {
    faces: Vec<([MeshVertex; 4], f32)>,
    vertices: Vec<MeshVertex>,
    indices: Vec<u32>,
}

impl Atom<DesktopContext> for Sculpture {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        let mut buffers = self.buffers.borrow_mut();
        let SculptureBuffers {
            faces,
            vertices,
            indices,
        } = &mut *buffers;
        faces.clear();
        vertices.clear();
        indices.clear();
        let time = self.phase * std::f32::consts::TAU;
        let (turn_sin, turn_cos) = time.sin_cos();
        let (tilt_sin, tilt_cos) = (0.6 + time.sin() * 0.3).sin_cos();
        let radius = area.width.min(area.height) * 0.38;
        for band in 0..24 {
            let latitude = (band as f32 / 23.0 - 0.5) * 2.8;
            for segment in 0..96 {
                let mut depth = 0.0;
                let quad = [(0.0, -1.0), (1.0, -1.0), (0.0, 1.0), (1.0, 1.0)].map(|(step, edge)| {
                    let angle = (segment as f32 + step) / 96.0 * std::f32::consts::TAU;
                    let latitude = latitude + edge * 0.033;
                    let (vertical, horizontal) = latitude.sin_cos();
                    let (sine, cosine) = angle.sin_cos();
                    let x = horizontal * cosine;
                    let y = vertical;
                    let z = horizontal * sine;
                    let twisted = z * turn_cos - x * turn_sin;
                    let x = x * turn_cos + z * turn_sin;
                    let y_rotated = y * tilt_cos - twisted * tilt_sin;
                    let z = y * tilt_sin + twisted * tilt_cos;
                    let y = y_rotated;
                    let perspective = 4.0 / (4.0 - z);
                    depth += z;
                    let diffuse = (x * -0.4 - y * 0.5 + z * 0.75).max(0.0);
                    let highlight = diffuse.powi(18) * 150.0;
                    let light = 0.14 + diffuse * 0.68;
                    let base = if band % 6 < 2 {
                        [220.0, 159.0, 83.0]
                    } else {
                        [175.0, 191.0, 193.0]
                    };
                    let [red, green, blue] = base.map(|channel| (channel * light + highlight).min(255.0) as u8);
                    MeshVertex::new(
                        area.x + area.width * 0.5 + x * radius * perspective,
                        area.y + area.height * 0.5 + y * radius * perspective,
                        Color::rgb(red, green, blue),
                    )
                });
                faces.push((quad, depth));
            }
        }
        faces.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
        for (quad, _) in faces.iter() {
            let first = vertices.len() as u32;
            vertices.extend_from_slice(quad);
            indices.extend([first, first + 1, first + 2, first + 1, first + 3, first + 2]);
        }
        cx.context_mut().paint(area, Mesh { vertices, indices });
    }
}
