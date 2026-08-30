use std::time::Duration;

use blit_kernel::{
    Absolute, Anchor, Clip, Constraints, Frame, FrameInfo, Input, Interaction, Layout, LayoutCx,
    Leaf, Modifiers, Paint, Point, PointerButton, Rect, Renderer, Sense, Size, Transition, Ui,
    WidgetId,
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

#[test]
fn resolves_absolute_targets_and_layer_order() {
    let mut frame = Frame::<AsciiRenderer>::default();
    let mut renderer = AsciiRenderer::default();

    frame.render(&mut renderer, Size::new(8.0, 5.0), |mut ui| {
        let mut overlay = ui.layout(Overlay);
        let target = overlay.add((), |mut ui| ui.add(Fill::new('T', Size::new(2.0, 2.0))));
        overlay.add((), |mut ui| {
            let _absolute = ui
                .layout_with(Fill::new('A', Size::new(1.0, 1.0)), Overlay)
                .absolute(
                    Absolute::attach(Anchor::BottomRight, Anchor::TopLeft).relative_to(target),
                );
        });
    });

    assert_eq!(
        renderer.contents(),
        concat!(
            "        \n",
            "   TT   \n",
            "   TT   \n",
            "     A  \n",
            "        ",
        )
    );

    frame.render(&mut renderer, Size::new(3.0, 1.0), |mut ui| {
        let mut overlay = ui.layout(Overlay);
        let layer = overlay.new_layer();
        overlay.add((), |mut ui| {
            let node = ui.add(Fill::new('A', Size::new(3.0, 1.0)));
            ui.set_layer(node, layer);
        });
        overlay.add((), |mut ui| {
            let node = ui.add(Fill::new('B', Size::new(3.0, 1.0)));
            ui.set_z_index(node, 100);
        });
    });

    assert_eq!(renderer.contents(), "AAA");

    frame.render(&mut renderer, Size::new(3.0, 3.0), |mut ui| {
        let mut root = ui.layout(Overlay);
        let layer = root.new_layer();
        root.add((), |mut ui| {
            let mut panel = ui
                .layout_with(Fill::new('p', Size::new(3.0, 3.0)), BaseOnly)
                .clip(DiamondClip);
            panel.add((), |mut ui| {
                let node = ui.add(Fill::new('L', Size::new(3.0, 3.0)));
                ui.set_layer(node, layer);
            });
        });
    });
    assert_eq!(renderer.contents(), "LLL\nLLL\nLLL");

    frame.render(&mut renderer, Size::new(3.0, 3.0), |mut ui| {
        let mut root = ui.layout(Overlay);
        root.add((), |mut ui| {
            let mut panel = ui
                .layout_with(Fill::new('p', Size::new(3.0, 3.0)), BaseOnly)
                .clip(DiamondClip);
            let layer = panel.new_layer();
            panel.add((), |mut ui| {
                let node = ui.add(Fill::new('L', Size::new(3.0, 3.0)));
                ui.set_layer(node, layer);
            });
        });
    });
    assert_eq!(renderer.contents(), " L \nLLL\n L ");
}

#[test]
fn interaction_uses_resolved_paint_order() {
    let mut frame = Frame::<AsciiRenderer>::default();
    let mut renderer = AsciiRenderer::default();
    let bottom = WidgetId::new("bottom");
    let top = WidgetId::new("top");

    frame.render(&mut renderer, Size::new(3.0, 1.0), |ui| {
        buttons(ui, bottom, top);
    });
    assert_eq!(frame.geometry(top), Some(Rect::new(0.0, 0.0, 3.0, 1.0)));

    let mut responses = Vec::new();
    frame.render_inputs(
        &mut renderer,
        Size::new(3.0, 1.0),
        Duration::ZERO,
        [
            Input::PointerDown {
                position: Point::new(1.0, 0.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
            Input::PointerUp {
                position: Point::new(1.0, 0.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
        ],
        |ui| responses.push(buttons(ui, bottom, top)),
    );

    assert!(!responses[0][0].active);
    assert!(responses[0][1].active);
    assert!(responses[0][1].activated);
    assert!(!responses[1][0].clicked);
    assert!(responses[1][1].clicked);
    assert!(responses[1][1].deactivated);
}

#[test]
fn transitions_relayout_animated_sizes() {
    let mut frame = Frame::<AsciiRenderer>::default();
    let mut renderer = AsciiRenderer::default();
    let id = WidgetId::new("transition");

    transition_scene(&mut frame, &mut renderer, id, 1.0, Duration::ZERO);
    assert_eq!(renderer.contents(), "X   ");

    transition_scene(&mut frame, &mut renderer, id, 3.0, Duration::ZERO);
    assert_eq!(renderer.contents(), "X   ");
    assert!(frame.has_pending_redraw());

    transition_scene(
        &mut frame,
        &mut renderer,
        id,
        3.0,
        Duration::from_millis(500),
    );
    assert_eq!(renderer.contents(), "XX  ");

    transition_scene(&mut frame, &mut renderer, id, 3.0, Duration::from_secs(1));
    assert_eq!(renderer.contents(), "XXX ");
    assert!(!frame.has_pending_redraw());
}

#[test]
fn transitions_resolved_positions() {
    let mut frame = Frame::<AsciiRenderer>::default();
    let mut renderer = AsciiRenderer::default();
    let id = WidgetId::new("position transition");

    position_transition_scene(&mut frame, &mut renderer, id, 0.0, Duration::ZERO);
    position_transition_scene(&mut frame, &mut renderer, id, 2.0, Duration::ZERO);
    assert_eq!(renderer.contents(), "X\n \n \n ");

    position_transition_scene(
        &mut frame,
        &mut renderer,
        id,
        2.0,
        Duration::from_millis(500),
    );
    assert_eq!(renderer.contents(), " \nX\n \n ");

    position_transition_scene(&mut frame, &mut renderer, id, 2.0, Duration::from_secs(1));
    assert_eq!(renderer.contents(), " \n \nX\n ");
}

#[test]
fn interaction_is_bounded_by_clip_rectangles() {
    let mut frame = Frame::<AsciiRenderer>::default();
    let mut renderer = AsciiRenderer::default();
    let id = WidgetId::new("clipped");

    frame.render(&mut renderer, Size::new(5.0, 1.0), |ui| {
        clipped_button(ui, id);
    });

    let mut active = Vec::new();
    frame.render_inputs(
        &mut renderer,
        Size::new(5.0, 1.0),
        Duration::ZERO,
        [
            Input::PointerDown {
                position: Point::new(4.5, 0.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
            Input::PointerUp {
                position: Point::new(4.5, 0.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
            Input::PointerDown {
                position: Point::new(2.5, 0.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
        ],
        |ui| active.push(clipped_button(ui, id).active),
    );

    assert_eq!(active, [false, false, true]);
}

fn buttons(mut ui: Ui<'_, AsciiRenderer>, bottom: WidgetId, top: WidgetId) -> [Interaction; 2] {
    let responses = [
        ui.interact(bottom, Sense::CLICK),
        ui.interact(top, Sense::CLICK),
    ];
    let mut overlay = ui.layout(Overlay);
    overlay.add((), |mut ui| {
        let node = ui.add(Fill::new('B', Size::new(3.0, 1.0)));
        ui.set_id(node, bottom);
    });
    overlay.add((), |mut ui| {
        let node = ui.add(Fill::new('T', Size::new(3.0, 1.0)));
        ui.set_id(node, top);
        ui.set_z_index(node, 1);
    });
    responses
}

fn transition_scene(
    frame: &mut Frame<AsciiRenderer>,
    renderer: &mut AsciiRenderer,
    id: WidgetId,
    width: f32,
    time: Duration,
) {
    frame.render_inputs(
        renderer,
        Size::new(4.0, 1.0),
        time,
        [Input::None],
        |mut ui| {
            let mut column = ui.layout(Column);
            column.add(ColumnItem { gap_before: 0.0 }, |mut ui| {
                let _child = ui
                    .layout_with(Fill::new('X', Size::new(width, 1.0)), Overlay)
                    .id(id)
                    .transition(Transition::new(Duration::from_secs(1)).width());
            });
        },
    );
}

fn position_transition_scene(
    frame: &mut Frame<AsciiRenderer>,
    renderer: &mut AsciiRenderer,
    id: WidgetId,
    gap: f32,
    time: Duration,
) {
    frame.render_inputs(
        renderer,
        Size::new(1.0, 4.0),
        time,
        [Input::None],
        |mut ui| {
            let mut column = ui.layout(Column);
            column.add(ColumnItem { gap_before: gap }, |mut ui| {
                let _child = ui
                    .layout_with(Fill::new('X', Size::new(1.0, 1.0)), Overlay)
                    .id(id)
                    .transition(Transition::new(Duration::from_secs(1)).y());
            });
        },
    );
}

fn clipped_button(mut ui: Ui<'_, AsciiRenderer>, id: WidgetId) -> Interaction {
    let interaction = ui.interact(id, Sense::CLICK);
    let mut root = ui.layout(Overlay);
    root.add((), |mut ui| {
        let mut panel = ui
            .layout_with(Fill::new('P', Size::new(3.0, 1.0)), BaseOnly)
            .clip(DiamondClip);
        panel.add((), |mut ui| {
            let node = ui.add(Fill::new('C', Size::new(5.0, 1.0)));
            ui.set_id(node, id);
        });
    });
    interaction
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

#[derive(Clone, Copy)]
struct BaseOnly;

impl<R: Renderer> Layout<R> for BaseOnly {
    type Item = ();

    fn layout(&self, mut cx: LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let size = constraints.constrain(cx.measure_base(Constraints::loose(constraints.max)));
        for child in cx.children() {
            cx.layout_child(child, Constraints::loose(constraints.max));
            cx.set_position(child, Point::ZERO);
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
