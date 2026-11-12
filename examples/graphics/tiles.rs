use std::f32::consts::TAU;

use blit::{HasFrameStore, Paint};
use blit_gui::mesh;

use super::*;

#[derive(Default)]
pub struct View;

impl View {
    pub const TITLE: &str = "tiles";

    pub fn build(&mut self, mut ui: Ui<'_>, phase: f32) -> bool {
        ui.insert(Tiles { phase });
        false
    }
}

struct Tiles {
    phase: f32,
}

impl Atom<DesktopContext> for Tiles {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        let turn = self.phase * TAU;
        let tile = area.width.min(area.height) * 0.047;
        let mut order = cx.scratch().slice(81);
        order.extend(0..81);
        order.sort_unstable_by_key(|index| index / 9 + index % 9);
        let context = cx.context_mut();
        let mut vertices = context.store_mut().slice(81 * 12);
        vertices.extend((0..81 * 12).map(|index| {
            let block = index / 12;
            let (row, column) = ((block / 9) as f32, (block % 9) as f32);
            let distance = ((row - 4.0).powi(2) + (column - 4.0).powi(2)).sqrt();
            let height = tile * (0.5 + (0.5 + (distance * 0.85 - turn * 2.0).cos() * 0.5) * 2.4);
            let x = area.x + area.width * 0.5 + (column - row) * tile;
            let y = area.y + area.height * 0.53 + (column + row - 8.0) * tile * 0.5;
            let width = tile * 0.91;
            let face = index % 12 / 4;
            let corners = match face {
                0 => [
                    [x, y - width * 0.5 - height],
                    [x + width, y - height],
                    [x, y + width * 0.5 - height],
                    [x - width, y - height],
                ],
                1 => [
                    [x - width, y - height],
                    [x, y + width * 0.5 - height],
                    [x, y + width * 0.5],
                    [x - width, y],
                ],
                _ => [
                    [x, y + width * 0.5 - height],
                    [x + width, y - height],
                    [x + width, y],
                    [x, y + width * 0.5],
                ],
            };
            let [x, y] = corners[index as usize % 4];
            let base = if (block / 27 + block % 9 / 3) % 2 == 0 {
                [226.0, 184.0, 107.0]
            } else {
                [79.0, 153.0, 158.0]
            };
            let rgb = base.map(|value| (value * [1.0, 0.64, 0.38][face as usize]) as u8);
            MeshVertex::new(x, y, Color::rgb(rgb[0], rgb[1], rgb[2]))
        }));
        let vertices = vertices.finish();
        let mut indices = context.store_mut().slice(81 * 18);
        indices.extend((0..81 * 18).map(|index| {
            let block = order[index as usize / 18];
            let face = index % 18 / 6;
            block * 12 + face * 4 + [0, 1, 2, 0, 2, 3][index as usize % 6]
        }));
        let indices = indices.finish();
        context.paint(area, mesh::Stored { vertices, indices });
    }
}
