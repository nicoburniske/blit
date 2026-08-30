use blit_kernel::{
    Clip, Constraints, Frame, FrameInfo, Layout, LayoutCx, Leaf, Paint, Point, Rect, Renderer,
    Size, Ui,
};

#[test]
fn lays_out_and_paints_external_leaves() {
    let mut frame = Frame::<AsciiRenderer>::default();
    let mut renderer = AsciiRenderer::default();

    frame.render(&mut renderer, Size::new(8.0, 6.0), scene);

    assert_eq!(
        renderer.contents(),
        concat!(
            "AAA     \n",
            "        \n",
            "  b     \n",
            "bbCbb   \n",
            "  b     \n",
            "        ",
        )
    );
}

fn scene(mut ui: Ui<'_, AsciiRenderer>) {
    let mut column = ui.layout(Column);
    column.add(ColumnItem { gap_before: 0.0 }, |mut ui| {
        ui.add(Fill::new('A', Size::new(3.0, 1.0)));
    });
    column.add(ColumnItem { gap_before: 1.0 }, |mut ui| {
        let mut panel = ui
            .layout_with(Fill::new('b', Size::new(5.0, 3.0)), Overlay)
            .clip(DiamondClip);
        panel.add((), |mut ui| {
            ui.add(Fill::new('C', Size::new(1.0, 1.0)));
        });
    });
}

#[derive(Clone, Copy)]
struct Fill {
    glyph: char,
    size: Size,
}

impl Fill {
    const fn new(glyph: char, size: Size) -> Self {
        Self { glyph, size }
    }
}

impl Leaf<AsciiRenderer> for Fill {
    fn measure(&self, _: &mut AsciiRenderer, constraints: Constraints) -> Size {
        constraints.constrain(self.size)
    }

    fn paint(&self, renderer: &mut AsciiRenderer, area: Rect) {
        FillCommand {
            area,
            glyph: self.glyph,
        }
        .paint(renderer);
    }
}

#[derive(Clone, Copy)]
struct FillCommand {
    area: Rect,
    glyph: char,
}

impl Paint<AsciiRenderer> for FillCommand {
    fn paint(self, renderer: &mut AsciiRenderer) {
        let left = self.area.x.max(0.0) as usize;
        let top = self.area.y.max(0.0) as usize;
        let right = (self.area.x + self.area.width).min(renderer.width as f32) as usize;
        let bottom = (self.area.y + self.area.height).min(renderer.height as f32) as usize;
        for y in top..bottom {
            for x in left..right {
                let point = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                if !renderer
                    .diamond_clips
                    .iter()
                    .all(|area| DiamondClip::contains_point(*area, point))
                {
                    continue;
                }
                renderer.cells[y * renderer.width + x] = self.glyph;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct DiamondClip;

impl DiamondClip {
    fn contains_point(area: Rect, point: Point) -> bool {
        let radius_x = area.width / 2.0;
        let radius_y = area.height / 2.0;
        if radius_x <= 0.0 || radius_y <= 0.0 {
            return false;
        }
        let center_x = area.x + radius_x;
        let center_y = area.y + radius_y;
        (point.x - center_x).abs() / radius_x + (point.y - center_y).abs() / radius_y <= 1.0
    }
}

impl Clip<AsciiRenderer> for DiamondClip {
    fn push(&self, renderer: &mut AsciiRenderer, area: Rect) {
        renderer.diamond_clips.push(area);
    }

    fn pop(&self, renderer: &mut AsciiRenderer) {
        renderer.diamond_clips.pop().expect("clip stack is empty");
    }
}

#[derive(Clone, Copy)]
struct ColumnItem {
    gap_before: f32,
}

#[derive(Clone, Copy)]
struct Column;

impl<R: Renderer> Layout<R> for Column {
    type Item = ColumnItem;

    fn layout(&self, mut cx: LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let base = cx.measure_base(Constraints::loose(constraints.max));
        let mut children = Size::ZERO;
        for child in cx.children() {
            let item = cx.item(child);
            let size = cx.layout_child(child, Constraints::loose(constraints.max));
            children.width = children.width.max(size.width);
            children.height += item.gap_before + size.height;
        }

        let size = constraints.constrain(base.max(children));
        let mut y = 0.0;
        for child in cx.children() {
            y += cx.item(child).gap_before;
            cx.set_position(child, Point::new(0.0, y));
            y += cx.size(child).height;
        }
        size
    }
}

#[derive(Clone, Copy)]
struct Overlay;

impl<R: Renderer> Layout<R> for Overlay {
    type Item = ();

    fn layout(&self, mut cx: LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut size = cx.measure_base(Constraints::loose(constraints.max));
        for child in cx.children() {
            size = size.max(cx.layout_child(child, Constraints::loose(constraints.max)));
        }
        let size = constraints.constrain(size);
        for child in cx.children() {
            let child_size = cx.size(child);
            cx.set_position(
                child,
                Point::new(
                    (size.width - child_size.width) / 2.0,
                    (size.height - child_size.height) / 2.0,
                ),
            );
        }
        size
    }
}

#[derive(Default)]
struct AsciiRenderer {
    width: usize,
    height: usize,
    cells: Vec<char>,
    diamond_clips: Vec<Rect>,
}

impl AsciiRenderer {
    fn contents(&self) -> String {
        let mut contents = String::with_capacity(self.cells.len() + self.height.saturating_sub(1));
        for (row, cells) in self.cells.chunks(self.width).enumerate() {
            if row != 0 {
                contents.push('\n');
            }
            contents.extend(cells);
        }
        contents
    }
}

impl Renderer for AsciiRenderer {
    fn begin(&mut self, frame: FrameInfo) {
        self.width = frame.size.width as usize;
        self.height = frame.size.height as usize;
        self.cells.clear();
        self.cells.resize(self.width * self.height, ' ');
        assert!(self.diamond_clips.is_empty());
    }

    fn end(&mut self) {
        assert!(self.diamond_clips.is_empty());
    }
}
