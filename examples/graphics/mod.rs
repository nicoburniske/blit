use super::*;

#[cfg_attr(feature = "gpu", path = "gpu.rs")]
#[cfg_attr(not(feature = "gpu"), path = "cpu.rs")]
mod backend;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Detail {
    Low,
    #[default]
    Medium,
    High,
}

pub struct View {
    detail: Detail,
    phase: f32,
    running: bool,
    effect: bool,
    showcase: backend::View,
    terrain: Rc<RefCell<TerrainBuffers>>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            detail: Detail::default(),
            phase: 0.0,
            running: true,
            effect: true,
            showcase: backend::View::default(),
            terrain: Rc::default(),
        }
    }
}

impl Widget<DesktopContext> for &mut View {
    type Response = ();

    fn build(self, mut ui: Ui<'_>) {
        if self.running {
            self.phase = ui.animate_loop(
                Id::new("graphics laboratory"),
                Tween::new(Duration::from_secs(24), Easing::LINEAR),
            );
        }
        self.showcase.phase = self.phase;
        let mut page = ui.layout(flex::column().gap(sz::SM));
        {
            let mut toolbar = page.child().layout(flex::row().gap(sz::XS).align(Align::Center));
            toolbar.child().build(select(
                &mut self.effect,
                &[(backend::View::TITLE, true), ("terrain", false)],
                flex::row().gap(sz::XXS),
                draw_button,
            ));
            toolbar.child().item(flex::item().grow()).insert(());
            if toolbar
                .child()
                .build(Button::new(if self.running { "[ pause ]" } else { "[ play ]" }, false))
            {
                self.running = !self.running;
            }
        }
        let mut stage = page.child().item(flex::item().grow()).layout(()).clip(BoundsClip);
        stage.insert(Rectangle::new().background(colors::TRACK));
        if self.effect {
            if stage.child().build(&mut self.showcase) {
                self.running = false;
            }
        } else {
            let (columns, rows) = match self.detail {
                Detail::Low => (64, 36),
                Detail::Medium => (128, 72),
                Detail::High => (256, 144),
            };
            stage.insert(Terrain {
                columns,
                rows,
                phase: self.phase,
                buffers: Rc::clone(&self.terrain),
            });
            stage.child().build(select(
                &mut self.detail,
                &[("4K", Detail::Low), ("18K", Detail::Medium), ("74K", Detail::High)],
                absolute::place(flex::row().gap(sz::XXS).pad(sz::SM)),
                draw_button,
            ));
        }
        stage.insert(Rectangle::new().border(Border::solid(1.0, colors::BORDER)));
    }
}

struct Terrain {
    columns: usize,
    rows: usize,
    phase: f32,
    buffers: Rc<RefCell<TerrainBuffers>>,
}

#[derive(Default)]
struct TerrainBuffers {
    points: Vec<(f32, f32, f32, f32)>,
    vertices: Vec<MeshVertex>,
    indices: Vec<u32>,
}

impl Atom<DesktopContext> for Terrain {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        let travel = self.phase * std::f32::consts::TAU;
        let mut buffers = self.buffers.borrow_mut();
        let TerrainBuffers {
            points,
            vertices,
            indices,
        } = &mut *buffers;
        points.clear();
        vertices.clear();
        indices.clear();
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
        for row in 0..self.rows {
            for column in 0..self.columns {
                let top = row * stride + column;
                let a = points[top];
                let b = points[top + 1];
                let c = points[top + stride];
                let d = points[top + stride + 1];
                for (triangle, slope) in [
                    ([a, b, c], (a.2 - b.2) * 0.7 + (a.2 - c.2) * 0.3),
                    ([b, d, c], (c.2 - d.2) * 0.7 + (b.2 - d.2) * 0.3),
                ] {
                    let elevation = (triangle[0].2 + triangle[1].2 + triangle[2].2) / 3.0;
                    let depth = (triangle[0].3 + triangle[1].3 + triangle[2].3) / 3.0;
                    let color = shade(elevation, depth, slope);
                    let first = u32::try_from(vertices.len()).expect("terrain mesh is too large");
                    vertices.extend(triangle.map(|point| MeshVertex::new(point.0, point.1, color)));
                    indices.extend(first..first + 3);
                }
            }
        }
        let bounds = vertices.iter().fold(area, |bounds, vertex| {
            bounds.union(LogicalRect::new(vertex.position.x, vertex.position.y, 0.0, 0.0))
        });
        Mesh { vertices, indices }.paint(bounds, cx.context_mut());
    }
}
