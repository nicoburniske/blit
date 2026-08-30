use std::time::Duration;

use blit::{
    Absolute, Anchor, Axis, Clip, Constraints, Easing, Frame, FrameInfo, Input, Interaction,
    Layout, LayoutCx, LayoutResolution, Leaf, Modifiers, NodeId, Platform, Point, PointerButton,
    Rect, Sense, Size, Sizing, Slot, Transition, Widget, WidgetId,
};

type Ui<'a> = blit::Ui<'a, AsciiPlatform>;

#[test]
fn animations_and_timers_schedule_frames() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let animation = WidgetId::new("animation");
    let timer = WidgetId::new("timer");
    let mut value = 0.0;
    let mut fired = false;

    for (time, target) in [
        (Duration::ZERO, 0.0),
        (Duration::ZERO, 1.0),
        (Duration::from_millis(500), 1.0),
    ] {
        frame.render_inputs(
            &mut platform,
            FrameInfo::new(Size::new(1.0, 1.0)),
            time,
            [],
            |mut ui| {
                value = ui.animate(animation, target, Duration::from_secs(1), Easing::Linear);
                fired = ui.timer(timer, Duration::from_millis(500));
                ui.add(Fill::new('X', Size::new(1.0, 1.0)));
            },
        );
    }

    assert_eq!(value, 0.5);
    assert!(fired);
    assert!(frame.has_pending_redraw());
}

#[test]
fn lays_out_and_paints_external_leaves() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();

    frame.render(&mut platform, FrameInfo::new(Size::new(8.0, 6.0)), scene);

    assert_eq!(
        platform.contents(),
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
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(8.0, 5.0)),
        |mut ui| {
            let mut overlay = ui.layout(Overlay);
            let target = overlay.add(Fill::new('T', Size::new(2.0, 2.0)));
            overlay.add(|ui: &mut Ui<'_>| {
                let _absolute = ui
                    .layout_with(Fill::new('A', Size::new(1.0, 1.0)), Overlay)
                    .absolute(
                        Absolute::attach(Anchor::BottomRight, Anchor::TopLeft).relative_to(target),
                    );
            });
        },
    );

    assert_eq!(
        platform.contents(),
        concat!(
            "        \n",
            "   TT   \n",
            "   TT   \n",
            "     A  \n",
            "        ",
        )
    );

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(3.0, 1.0)),
        |mut ui| {
            let mut overlay = ui.layout(Overlay);
            let layer = overlay.new_layer();
            overlay
                .child()
                .slot(Slot::new().layer(layer))
                .add(Fill::new('A', Size::new(3.0, 1.0)));
            overlay
                .child()
                .slot(Slot::new().z_index(100))
                .add(Fill::new('B', Size::new(3.0, 1.0)));
        },
    );

    assert_eq!(platform.contents(), "AAA");

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(3.0, 3.0)),
        |mut ui| {
            let mut root = ui.layout(Overlay);
            let layer = root.new_layer();
            root.add(|ui: &mut Ui<'_>| {
                let mut panel = ui
                    .layout_with(Fill::new('p', Size::new(3.0, 3.0)), BaseOnly)
                    .clip(DiamondClip);
                panel
                    .child()
                    .slot(Slot::new().layer(layer))
                    .add(Fill::new('L', Size::new(3.0, 3.0)));
            });
        },
    );
    assert_eq!(platform.contents(), "LLL\nLLL\nLLL");

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(3.0, 3.0)),
        |mut ui| {
            let mut root = ui.layout(Overlay);
            root.add(|ui: &mut Ui<'_>| {
                let mut panel = ui
                    .layout_with(Fill::new('p', Size::new(3.0, 3.0)), BaseOnly)
                    .clip(DiamondClip);
                let layer = panel.new_layer();
                panel
                    .child()
                    .slot(Slot::new().layer(layer))
                    .add(Fill::new('L', Size::new(3.0, 3.0)));
            });
        },
    );
    assert_eq!(platform.contents(), " L \nLLL\n L ");
}

#[test]
fn interaction_uses_resolved_paint_order() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let bottom = WidgetId::new("bottom");
    let top = WidgetId::new("top");

    frame.render(&mut platform, FrameInfo::new(Size::new(3.0, 1.0)), |ui| {
        buttons(ui, bottom, top);
    });
    assert_eq!(frame.geometry(top), Some(Rect::new(0.0, 0.0, 3.0, 1.0)));

    let mut responses = Vec::new();
    frame.render_inputs(
        &mut platform,
        FrameInfo::new(Size::new(3.0, 1.0)),
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
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let id = WidgetId::new("transition");

    transition_scene(&mut frame, &mut platform, id, 1.0, Duration::ZERO);
    assert_eq!(platform.contents(), "X   ");

    transition_scene(&mut frame, &mut platform, id, 3.0, Duration::ZERO);
    assert_eq!(platform.contents(), "X   ");
    assert!(frame.has_pending_redraw());

    transition_scene(
        &mut frame,
        &mut platform,
        id,
        3.0,
        Duration::from_millis(500),
    );
    assert_eq!(platform.contents(), "XX  ");

    transition_scene(&mut frame, &mut platform, id, 3.0, Duration::from_secs(1));
    assert_eq!(platform.contents(), "XXX ");
    assert!(!frame.has_pending_redraw());
}

#[test]
fn transitions_resolved_positions() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let id = WidgetId::new("position transition");

    position_transition_scene(&mut frame, &mut platform, id, 0.0, Duration::ZERO);
    position_transition_scene(&mut frame, &mut platform, id, 2.0, Duration::ZERO);
    assert_eq!(platform.contents(), " \nX\n \n ");

    position_transition_scene(
        &mut frame,
        &mut platform,
        id,
        2.0,
        Duration::from_millis(500),
    );
    assert_eq!(platform.contents(), " \n \nX\n ");

    position_transition_scene(&mut frame, &mut platform, id, 2.0, Duration::from_secs(1));
    assert_eq!(platform.contents(), " \n \n \nX");
}

#[test]
fn resolves_slots_and_content_offsets() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let fixed = WidgetId::new("fixed");
    let grow = WidgetId::new("grow");
    let percent = WidgetId::new("percent");
    let fit = WidgetId::new("fit");

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(8.0, 4.0)).layout_resolution(LayoutResolution::Discrete {
            step: Size::new(2.0, 1.0),
        }),
        |mut ui| {
            let mut overlay = ui.layout(Overlay).offset(Point::new(1.0, 0.0));
            overlay
                .child()
                .slot(Slot::new().fixed(3.0, 1.0))
                .id(fixed)
                .add(Fill::new('F', Size::new(1.0, 1.0)));
            overlay
                .child()
                .slot(Slot::new().width(Sizing::grow()).height(Sizing::fixed(1.0)))
                .id(grow)
                .add(Fill::new('G', Size::new(1.0, 1.0)));
            overlay
                .child()
                .slot(
                    Slot::new()
                        .width(Sizing::percent(0.25))
                        .height(Sizing::fixed(1.0)),
                )
                .id(percent)
                .add(Fill::new('P', Size::new(1.0, 1.0)));
            overlay
                .child()
                .slot(
                    Slot::new()
                        .width(Sizing::fit().max(3.0))
                        .height(Sizing::fixed(1.0)),
                )
                .id(fit)
                .add(Fill::new('M', Size::new(6.0, 1.0)));
        },
    );

    assert_eq!(frame.geometry(fixed), Some(Rect::new(3.0, 1.5, 4.0, 1.0)));
    assert_eq!(frame.geometry(grow), Some(Rect::new(1.0, 1.5, 8.0, 1.0)));
    assert_eq!(frame.geometry(percent), Some(Rect::new(4.0, 1.5, 2.0, 1.0)));
    assert_eq!(frame.geometry(fit), Some(Rect::new(3.0, 1.5, 4.0, 1.0)));
}

#[test]
fn absolute_slots_resolve_against_the_target() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let id = WidgetId::new("absolute");

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(10.0, 4.0)),
        |mut ui| {
            let mut overlay = ui.layout(Overlay);
            let target = overlay.add(Fill::new('T', Size::new(6.0, 2.0)));
            overlay
                .child()
                .slot(
                    Slot::new()
                        .width(Sizing::percent(0.5))
                        .height(Sizing::grow()),
                )
                .add(|ui: &mut Ui<'_>| {
                    let _absolute = ui
                        .layout_with(Fill::new('A', Size::new(1.0, 1.0)), Overlay)
                        .absolute(
                            Absolute::attach(Anchor::BottomRight, Anchor::TopLeft)
                                .relative_to(target),
                        )
                        .id(id);
                });
        },
    );

    assert_eq!(frame.geometry(id), Some(Rect::new(8.0, 3.0, 3.0, 2.0)));
}

#[test]
fn transitions_without_ids_are_ignored() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();

    unidentified_transition_scene(&mut frame, &mut platform, 1.0);
    unidentified_transition_scene(&mut frame, &mut platform, 3.0);

    assert_eq!(platform.contents(), "XXX ");
    assert!(!frame.has_pending_redraw());
}

#[cfg(debug_assertions)]
#[test]
fn frame_ids_reject_cross_frame_use() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let mut node = None;
    let mut layer = None;
    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(1.0, 1.0)),
        |mut ui| {
            let mut root = ui.layout(Overlay);
            layer = Some(root.new_layer());
            node = Some(root.add(Fill::new('X', Size::new(1.0, 1.0))));
        },
    );

    let node = node.unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            frame.render(
                &mut platform,
                FrameInfo::new(Size::new(1.0, 1.0)),
                |mut ui| ui.set_id(node, WidgetId::new("stale")),
            );
        }))
        .is_err()
    );

    let layer = layer.unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            frame.render(
                &mut platform,
                FrameInfo::new(Size::new(1.0, 1.0)),
                |mut ui| {
                    let mut root = ui.layout(Overlay);
                    root.child()
                        .slot(Slot::new().layer(layer))
                        .add(Fill::new('X', Size::new(1.0, 1.0)));
                },
            );
        }))
        .is_err()
    );
}

#[test]
fn interaction_is_bounded_by_clip_rectangles() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let id = WidgetId::new("clipped");

    frame.render(&mut platform, FrameInfo::new(Size::new(5.0, 1.0)), |ui| {
        clipped_button(ui, id);
    });

    let mut active = Vec::new();
    frame.render_inputs(
        &mut platform,
        FrameInfo::new(Size::new(5.0, 1.0)),
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

fn buttons(mut ui: Ui<'_>, bottom: WidgetId, top: WidgetId) -> [Interaction; 2] {
    let responses = [
        ui.interact(bottom, Sense::CLICK),
        ui.interact(top, Sense::CLICK),
    ];
    let mut overlay = ui.layout(Overlay);
    overlay
        .child()
        .id(bottom)
        .add(Fill::new('B', Size::new(3.0, 1.0)));
    overlay
        .child()
        .slot(Slot::new().z_index(1))
        .id(top)
        .add(Fill::new('T', Size::new(3.0, 1.0)));
    responses
}

fn transition_scene(
    frame: &mut Frame<AsciiPlatform>,
    platform: &mut AsciiPlatform,
    id: WidgetId,
    width: f32,
    time: Duration,
) {
    frame.render_inputs(
        platform,
        FrameInfo::new(Size::new(4.0, 1.0)),
        time,
        [Input::None],
        |mut ui| {
            let mut column = ui.layout(Column);
            column
                .item(ColumnItem { gap_before: 0.0 })
                .add(|ui: &mut Ui<'_>| {
                    let _child = ui
                        .layout_with(Fill::new('X', Size::new(width, 1.0)), Overlay)
                        .id(id)
                        .transition(Transition::new(Duration::from_secs(1)).width());
                });
        },
    );
}

fn unidentified_transition_scene(
    frame: &mut Frame<AsciiPlatform>,
    platform: &mut AsciiPlatform,
    width: f32,
) {
    frame.render_inputs(
        platform,
        FrameInfo::new(Size::new(4.0, 1.0)),
        Duration::ZERO,
        [Input::None],
        |mut ui| {
            let mut overlay = ui.layout(Overlay);
            overlay.add(|ui: &mut Ui<'_>| {
                let _child = ui
                    .layout_with(Fill::new('X', Size::new(width, 1.0)), Overlay)
                    .transition(Transition::new(Duration::from_secs(1)).width());
            });
        },
    );
}

fn position_transition_scene(
    frame: &mut Frame<AsciiPlatform>,
    platform: &mut AsciiPlatform,
    id: WidgetId,
    gap: f32,
    time: Duration,
) {
    frame.render_inputs(
        platform,
        FrameInfo::new(Size::new(1.0, 4.0)),
        time,
        [Input::None],
        |mut ui| {
            let mut column = ui.layout(Column).offset(Point::new(0.0, 1.0));
            column
                .item(ColumnItem { gap_before: gap })
                .add(|ui: &mut Ui<'_>| {
                    let _child = ui
                        .layout_with(Fill::new('X', Size::new(1.0, 1.0)), Overlay)
                        .id(id)
                        .transition(Transition::new(Duration::from_secs(1)).y());
                });
        },
    );
}

fn clipped_button(mut ui: Ui<'_>, id: WidgetId) -> Interaction {
    let interaction = ui.interact(id, Sense::CLICK);
    let mut root = ui.layout(Overlay);
    root.add(|ui: &mut Ui<'_>| {
        let mut panel = ui
            .layout_with(Fill::new('P', Size::new(3.0, 1.0)), BaseOnly)
            .clip(DiamondClip);
        panel
            .child()
            .id(id)
            .add(Fill::new('C', Size::new(5.0, 1.0)));
    });
    interaction
}

fn scene(mut ui: Ui<'_>) {
    let mut column = ui.layout(Column);
    column
        .item(ColumnItem { gap_before: 0.0 })
        .add(Fill::new('A', Size::new(3.0, 1.0)));
    column
        .item(ColumnItem { gap_before: 1.0 })
        .add(|ui: &mut Ui<'_>| {
            let mut panel = ui
                .layout_with(Fill::new('b', Size::new(5.0, 3.0)), Overlay)
                .clip(DiamondClip);
            panel.add(Fill::new('C', Size::new(1.0, 1.0)));
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

impl Widget<AsciiPlatform> for Fill {
    type Response = NodeId;

    fn build(self, ui: &mut Ui<'_>) -> Self::Response {
        ui.add_leaf(self)
    }
}

impl Leaf<AsciiPlatform> for Fill {
    fn measure(&self, _: &mut AsciiPlatform, constraints: Constraints) -> Size {
        constraints.constrain(self.size)
    }

    fn paint(&self, platform: &mut AsciiPlatform, area: Rect) {
        let left = area.x.max(0.0) as usize;
        let top = area.y.max(0.0) as usize;
        let right = (area.x + area.width).min(platform.width as f32) as usize;
        let bottom = (area.y + area.height).min(platform.height as f32) as usize;
        for y in top..bottom {
            for x in left..right {
                let point = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                if !platform
                    .diamond_clips
                    .iter()
                    .all(|area| DiamondClip::contains_point(*area, point))
                {
                    continue;
                }
                platform.cells[y * platform.width + x] = self.glyph;
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

impl Clip<AsciiPlatform> for DiamondClip {
    fn push(&self, platform: &mut AsciiPlatform, area: Rect) {
        platform.diamond_clips.push(area);
    }

    fn pop(&self, platform: &mut AsciiPlatform) {
        platform.diamond_clips.pop().expect("clip stack is empty");
    }
}

#[derive(Clone, Copy)]
struct ColumnItem {
    gap_before: f32,
}

#[derive(Clone, Copy)]
struct Column;

impl<R: Platform> Layout<R> for Column {
    type Item = ColumnItem;

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let base = cx.measure_base(Constraints::loose(constraints.max));
        let mut children = Size::ZERO;
        for child in cx.children() {
            let item = cx.item(child);
            let size = resolve_child(cx, child, constraints.max, true, false);
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

impl<R: Platform> Layout<R> for Overlay {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut size = cx.measure_base(Constraints::loose(constraints.max));
        for child in cx.children() {
            size = size.max(resolve_child(cx, child, constraints.max, true, true));
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

impl<R: Platform> Layout<R> for BaseOnly {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let size = constraints.constrain(cx.measure_base(Constraints::loose(constraints.max)));
        for child in cx.children() {
            resolve_child(cx, child, constraints.max, true, true);
            cx.set_position(child, Point::ZERO);
        }
        size
    }
}

fn resolve_child<R: Platform, I: Copy + 'static>(
    cx: &mut LayoutCx<'_, R, I>,
    child: NodeId,
    available: Size,
    width_cross: bool,
    height_cross: bool,
) -> Size {
    let intrinsic = cx.layout_child(child, Constraints::loose(available));
    let size = Size::new(
        cx.sizing(child, Axis::Horizontal)
            .resolve(intrinsic.width, available.width, width_cross),
        cx.sizing(child, Axis::Vertical)
            .resolve(intrinsic.height, available.height, height_cross),
    );
    cx.constrain_child(child, Constraints::tight(size))
}

#[derive(Default)]
struct AsciiPlatform {
    width: usize,
    height: usize,
    cells: Vec<char>,
    diamond_clips: Vec<Rect>,
}

impl AsciiPlatform {
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

impl Platform for AsciiPlatform {
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
