use blit::{FrameStore, Paint};
use blit_gui::mesh;

use super::*;

#[derive(Default)]
pub struct View {
    detail: Detail,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Detail {
    Low,
    #[default]
    Medium,
    High,
}

impl View {
    pub const TITLE: &str = "terrain";

    pub fn controls(&mut self, ui: Ui<'_>) {
        select(
            &mut self.detail,
            &[("4K", Detail::Low), ("18K", Detail::Medium), ("74K", Detail::High)],
            flex::row().gap(sz::XXS),
            draw_button,
        )
        .build(ui);
    }

    pub fn build(&mut self, ui: Ui<'_>, phase: f32) -> bool {
        let (columns, rows) = match self.detail {
            Detail::Low => (64, 36),
            Detail::Medium => (128, 72),
            Detail::High => (256, 144),
        };
        let mut stage = ui.layout(());
        stage.insert(Terrain { columns, rows, phase });
        false
    }
}

struct Terrain {
    columns: usize,
    rows: usize,
    phase: f32,
}

impl Atom<DesktopContext> for Terrain {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        let travel = self.phase * std::f32::consts::TAU;
        let mut points = cx.scratch((self.rows + 1) * (self.columns + 1));
        for row in 0..=self.rows {
            let depth = row as f32 / self.rows as f32;
            let perspective = depth * depth;
            let z = depth * 9.0 + travel;
            let span = area.width * (0.3 + perspective);
            for column in 0..=self.columns {
                let x = column as f32 / self.columns as f32;
                let world_x = (x - 0.5) * 8.0;
                let elevation = (world_x * 0.8 + z).sin() * 0.55
                    + (world_x * 1.9 - z * 0.7).cos() * 0.28
                    + (world_x * 4.7 + z * 1.3).sin() * 0.12
                    + (world_x * 11.3 - z * 2.7).cos() * 0.04;
                points.push((
                    area.x + area.width * 0.5 + (x - 0.5) * span,
                    area.y + area.height * (0.28 + perspective * 0.78) - elevation * (6.0 + perspective * 72.0),
                    elevation,
                    depth,
                ));
            }
        }
        let shade = |elevation: f32, depth: f32, slope: f32| {
            let strata = (elevation * 17.0 + depth * 8.0).sin().abs().powi(10);
            let light = (0.48 + slope * 1.8 + strata * 0.12).clamp(0.08, 1.0);
            let fog = (1.0 - depth).powi(2);
            Color::rgb(
                ((38.0 + light * 174.0) * (1.0 - fog) + 37.0 * fog) as u8,
                ((43.0 + light * 156.0) * (1.0 - fog) + 43.0 * fog) as u8,
                ((46.0 + light * 128.0) * (1.0 - fog) + 48.0 * fog) as u8,
            )
        };
        let stride = self.columns + 1;
        let count = u32::try_from(self.rows * self.columns * 6).expect("terrain mesh is too large");
        let mut triangle = [MeshVertex::new(0.0, 0.0, Color::TRANSPARENT); 3];
        let mut bounds = area;
        let context = cx.context_mut();
        let vertices = context.store_slice((0..count).map(|index| {
            if index % 3 == 0 {
                let cell = index as usize / 6;
                let top = cell / self.columns * stride + cell % self.columns;
                let a = points[top];
                let b = points[top + 1];
                let c = points[top + stride];
                let d = points[top + stride + 1];
                let (points, slope) = if index % 6 == 0 {
                    ([a, b, c], (a.2 - b.2) * 0.7 + (a.2 - c.2) * 0.3)
                } else {
                    ([b, d, c], (c.2 - d.2) * 0.7 + (b.2 - d.2) * 0.3)
                };
                let elevation = (points[0].2 + points[1].2 + points[2].2) / 3.0;
                let depth = (points[0].3 + points[1].3 + points[2].3) / 3.0;
                let color = shade(elevation, depth, slope);
                triangle = points.map(|point| MeshVertex::new(point.0, point.1, color));
            }
            let vertex = triangle[index as usize % 3];
            bounds = bounds.union(LogicalRect::new(vertex.position.x, vertex.position.y, 0.0, 0.0));
            vertex
        }));
        let indices = context.store_slice(0..count);
        context.paint(bounds, mesh::Stored { vertices, indices });
    }
}
