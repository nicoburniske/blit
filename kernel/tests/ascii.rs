use std::{cell::Cell, rc::Rc, time::Duration};

use blit::{
    Absolute, Anchor, Atom, Axis, Clip, Constraints, Content, Easing, Frame, FrameInfo, Input,
    Interaction, Layout, LayoutCx, LayoutResolution, Modifiers, NodeId, Platform, Point,
    PointerButton, Rect, Sense, Size, Sizing, Transition, WidgetId,
};

type Ui<'a, S = blit::state::Build> = blit::Ui<'a, AsciiPlatform, S>;

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
            FrameInfo::new(Size::uniform(1.0)),
            time,
            [],
            |mut ui: Ui<'_>| {
                value = ui.animate(animation, target, Duration::from_secs(1), Easing::Linear);
                fired = ui.timer(timer, Duration::from_millis(500));
                ui.insert(Fill::new('X', Size::uniform(1.0)));
            },
        );
    }

    assert_eq!(value, 0.5);
    assert!(fired);
    assert!(frame.has_pending_redraw());
}

#[test]
fn lays_out_and_paints_external_atoms() {
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
fn culls_only_atoms_with_disjoint_known_paint_bounds() {
    let culled = Rc::new(Cell::new(0));
    let clipped = Rc::new(Cell::new(0));
    let overflow = Rc::new(Cell::new(0));
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(3.0, 1.0)),
        |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            root.absolute(Absolute::at(4.0, 0.0)).insert(PaintCount {
                count: culled.clone(),
                bounds_offset: Point::ZERO,
            });
            root.absolute(Absolute::at(4.0, 0.0)).insert(PaintCount {
                count: overflow.clone(),
                bounds_offset: Point::new(-4.0, 0.0),
            });
            root.child(TestItem::fixed(1.0, 1.0)).build(|ui: Ui<'_>| {
                let mut panel = ui.layout(Overlay).clip(DiamondClip);
                panel.absolute(Absolute::at(1.0, 0.0)).insert(PaintCount {
                    count: clipped.clone(),
                    bounds_offset: Point::ZERO,
                });
            });
        },
    );

    assert_eq!(culled.get(), 0);
    assert_eq!(clipped.get(), 0);
    assert_eq!(overflow.get(), 1);
}

#[test]
fn leaf_atoms_measure_and_paint_in_order() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(5.0, 4.0)),
        |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            root.child(TestItem::default()).build(|mut ui: Ui<'_>| {
                ui.insert(());
                ui.insert(FillContent);
                ui.insert(Fill::new('B', Size::new(1.0, 2.0)));
            });
        },
    );

    assert_eq!(platform.contents(), "     \n BBB \n BBB \n     ");
}

#[test]
fn content_works_before_layout_on_current_and_fresh_nodes() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(2.0, 1.0)),
        |mut ui: Ui<'_>| {
            ui.insert(PreparedText("a"));
            let mut root = ui.layout(Overlay);
            root.insert(Pair('x', 'y'));
            root.child(TestItem::default()).insert(PreparedText("b"));
        },
    );

    assert_eq!(platform.prepared, 2);
    assert_eq!(platform.contents(), "BB");
}

#[test]
fn empty_and_absolute_children_are_valid() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();

    frame.render(
        &mut platform,
        FrameInfo::new(Size::uniform(1.0)),
        |ui: Ui<'_>| {
            let mut root = ui.layout(Column);
            root.child(
                TestItem::new(0.0)
                    .width(Sizing::grow())
                    .height(Sizing::grow()),
            );
            root.absolute(Absolute::at(0.0, 0.0));
        },
    );
}

#[test]
fn owned_frame_values_use_resolved_area_and_drop() {
    let area = Rc::new(Cell::new(Rect::default()));
    let drops = Rc::new(Cell::new(0));
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(3.0, 2.0)),
        |ui: Ui<'_>| {
            let value = || OwnedValue {
                area: area.clone(),
                drops: drops.clone(),
            };
            ui.layout(value()).insert(value());
        },
    );

    assert_eq!(area.get(), Rect::new(0.0, 0.0, 3.0, 2.0));
    assert_eq!(platform.contents(), "PPP\nPPP");
    assert_eq!(drops.get(), 2);
}

#[test]
fn resolves_absolute_targets_and_layer_order() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(8.0, 5.0)),
        |ui: Ui<'_>| {
            let mut overlay = ui.layout(Overlay);
            let target = overlay.child(TestItem::default()).build(|mut ui: Ui<'_>| {
                let id = ui.id();
                ui.insert(Fill::new('T', Size::uniform(2.0)));
                id
            });
            let mut absolute = overlay.absolute(
                Absolute::attach(Anchor::BottomRight, Anchor::TopLeft).relative_to(target),
            );
            absolute.insert(Fill::new('A', Size::uniform(1.0)));
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
        |ui: Ui<'_>| {
            let mut overlay = ui.layout(Overlay);
            let layer = overlay.new_layer();
            overlay
                .child(TestItem::default())
                .layer(layer)
                .insert(Fill::new('A', Size::new(3.0, 1.0)));
            overlay
                .child(TestItem::default())
                .z_index(100)
                .insert(Fill::new('B', Size::new(3.0, 1.0)));
        },
    );

    assert_eq!(platform.contents(), "AAA");

    frame.render(
        &mut platform,
        FrameInfo::new(Size::uniform(3.0)),
        |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            let layer = root.root_layer();
            root.child(TestItem::default()).build(|ui: Ui<'_>| {
                let mut panel = ui.layout(Fixed(Size::uniform(3.0))).clip(DiamondClip);
                panel.insert(Fill::new('p', Size::ZERO));
                panel
                    .child(TestItem::default())
                    .layer(layer)
                    .insert(Fill::new('L', Size::uniform(3.0)));
            });
        },
    );
    assert_eq!(platform.contents(), "LLL\nLLL\nLLL");

    frame.render(
        &mut platform,
        FrameInfo::new(Size::uniform(3.0)),
        |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            root.child(TestItem::default()).build(|ui: Ui<'_>| {
                let mut panel = ui.layout(Fixed(Size::uniform(3.0))).clip(DiamondClip);
                panel.insert(Fill::new('p', Size::ZERO));
                let layer = panel.new_layer();
                panel
                    .child(TestItem::default())
                    .layer(layer)
                    .insert(Fill::new('L', Size::uniform(3.0)));
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

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(3.0, 1.0)),
        |ui: Ui<'_>| {
            buttons(ui, bottom, top);
        },
    );
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
        |ui: Ui<'_>| responses.push(buttons(ui, bottom, top)),
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
    assert_eq!(platform.contents(), "X\nY\n \n ");

    transition_scene(&mut frame, &mut platform, id, 3.0, Duration::ZERO);
    assert_eq!(platform.contents(), "X\nY\n \n ");
    assert!(frame.has_pending_redraw());

    transition_scene(
        &mut frame,
        &mut platform,
        id,
        3.0,
        Duration::from_millis(500),
    );
    assert_eq!(platform.contents(), "X\nX\nY\n ");

    transition_scene(&mut frame, &mut platform, id, 3.0, Duration::from_secs(1));
    assert_eq!(platform.contents(), "X\nX\nX\nY");
    assert!(!frame.has_pending_redraw());
}

#[test]
fn unsupported_size_transitions_finish_immediately() {
    struct Unsupported;

    impl<R: Platform> Layout<R> for Unsupported {
        type Item = ();

        fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
            let child = cx.children().next().unwrap();
            let size = cx.layout_child(child, Constraints::loose(constraints.max));
            cx.set_child_position(child, Point::ZERO);
            constraints.constrain(size)
        }

        fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
            false
        }
    }

    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let id = WidgetId::new("unsupported transition");
    let mut render = |extent, time| {
        frame.render_inputs(
            &mut platform,
            FrameInfo::new(Size::uniform(4.0)),
            time,
            [Input::None],
            |ui: Ui<'_>| {
                ui.layout(Unsupported)
                    .child(())
                    .widget_id(id)
                    .transition(Transition::new(Duration::from_secs(1)).size())
                    .insert(Fill::new('X', Size::uniform(extent)));
            },
        );
    };

    render(1.0, Duration::ZERO);
    render(2.0, Duration::ZERO);

    assert_eq!(frame.geometry(id).unwrap().size(), Size::uniform(2.0));
    assert!(!frame.has_pending_redraw());
}

#[test]
fn absolute_size_transitions_use_layout_resolution() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let id = WidgetId::new("absolute transition");
    let info = FrameInfo::new(Size::new(5.0, 1.0)).layout_resolution(LayoutResolution::Discrete {
        step: Size::uniform(1.0),
    });
    let mut render = |width, time| {
        frame.render_inputs(&mut platform, info, time, [Input::None], |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            root.absolute(
                Absolute::at(0.0, 0.0)
                    .width(Sizing::fixed(width))
                    .height(Sizing::fixed(1.0)),
            )
            .widget_id(id)
            .transition(Transition::new(Duration::from_secs(1)).width())
            .insert(Fill::new('X', Size::uniform(1.0)));
        });
        frame.geometry(id).unwrap().width
    };

    render(1.0, Duration::ZERO);
    render(4.0, Duration::ZERO);
    assert_eq!(render(4.0, Duration::from_millis(500)), 3.0);
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
fn resolves_places_and_content_offsets() {
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
        |ui: Ui<'_>| {
            let mut overlay = ui.layout(Overlay).offset(Point::new(1.0, 0.0));
            overlay
                .child(TestItem::fixed(3.0, 1.0))
                .widget_id(fixed)
                .insert(Fill::new('F', Size::uniform(1.0)));
            overlay
                .child(
                    TestItem::default()
                        .width(Sizing::grow())
                        .height(Sizing::fixed(1.0)),
                )
                .widget_id(grow)
                .insert(Fill::new('G', Size::uniform(1.0)));
            overlay
                .child(
                    TestItem::default()
                        .width(Sizing::percent(0.25))
                        .height(Sizing::fixed(1.0)),
                )
                .widget_id(percent)
                .insert(Fill::new('P', Size::uniform(1.0)));
            overlay
                .child(
                    TestItem::default()
                        .width(Sizing::fit_range(0.0, 3.0))
                        .height(Sizing::fixed(1.0)),
                )
                .widget_id(fit)
                .insert(Fill::new('M', Size::new(6.0, 1.0)));
        },
    );

    assert_eq!(frame.geometry(fixed), Some(Rect::new(3.0, 1.5, 4.0, 1.0)));
    assert_eq!(frame.geometry(grow), Some(Rect::new(1.0, 1.5, 8.0, 1.0)));
    assert_eq!(frame.geometry(percent), Some(Rect::new(4.0, 1.5, 2.0, 1.0)));
    assert_eq!(frame.geometry(fit), Some(Rect::new(3.0, 1.5, 4.0, 1.0)));
}

#[test]
fn absolute_places_position_against_the_target_and_size_against_the_parent() {
    let mut frame = Frame::<AsciiPlatform>::default();
    let mut platform = AsciiPlatform::default();
    let id = WidgetId::new("absolute");

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(10.0, 4.0)),
        |ui: Ui<'_>| {
            let mut overlay = ui.layout(Overlay);
            let target = overlay.child(TestItem::default()).build(|mut ui: Ui<'_>| {
                let id = ui.id();
                ui.insert(Fill::new('T', Size::new(6.0, 2.0)));
                id
            });
            overlay
                .absolute(
                    Absolute::attach(Anchor::BottomRight, Anchor::TopLeft)
                        .relative_to(target)
                        .width(Sizing::percent(0.5))
                        .height(Sizing::grow()),
                )
                .build(|ui: Ui<'_>| {
                    let mut absolute = ui.layout(Overlay).widget_id(id);
                    absolute.insert(Fill::new('A', Size::ZERO));
                });
        },
    );

    assert_eq!(frame.geometry(id), Some(Rect::new(8.0, 3.0, 5.0, 4.0)));
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
        FrameInfo::new(Size::uniform(1.0)),
        |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            layer = Some(root.new_layer());
            node = Some(root.child(TestItem::default()).build(|mut ui: Ui<'_>| {
                let id = ui.id();
                ui.insert(Fill::new('X', Size::uniform(1.0)));
                id
            }));
        },
    );

    let node = node.unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            frame.render(
                &mut platform,
                FrameInfo::new(Size::uniform(1.0)),
                |ui: Ui<'_>| {
                    let mut root = ui.layout(Overlay);
                    root.child(TestItem::default());
                    root.absolute(Absolute::at(0.0, 0.0).relative_to(node));
                },
            );
        }))
        .is_err()
    );

    let layer = layer.unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            frame.render(
                &mut platform,
                FrameInfo::new(Size::uniform(1.0)),
                |ui: Ui<'_>| {
                    let mut root = ui.layout(Overlay);
                    root.child(TestItem::default())
                        .layer(layer)
                        .insert(Fill::new('X', Size::uniform(1.0)));
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

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(5.0, 1.0)),
        |ui: Ui<'_>| {
            clipped_button(ui, id);
        },
    );

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
        |ui: Ui<'_>| active.push(clipped_button(ui, id).active),
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
        .child(TestItem::default())
        .widget_id(bottom)
        .insert(Fill::new('B', Size::new(3.0, 1.0)));
    overlay
        .child(TestItem::default())
        .z_index(1)
        .widget_id(top)
        .insert(Fill::new('T', Size::new(3.0, 1.0)));
    responses
}

fn transition_scene(
    frame: &mut Frame<AsciiPlatform>,
    platform: &mut AsciiPlatform,
    id: WidgetId,
    height: f32,
    time: Duration,
) {
    frame.render_inputs(
        platform,
        FrameInfo::new(Size::new(1.0, 4.0)),
        time,
        [Input::None],
        |ui: Ui<'_>| {
            let mut column = ui.layout(Column);
            column.child(TestItem::new(0.0)).build(|ui: Ui<'_>| {
                let mut child = ui
                    .layout(Overlay)
                    .widget_id(id)
                    .transition(Transition::new(Duration::from_secs(1)).height());
                child
                    .child(TestItem::default())
                    .insert(Fill::new('X', Size::new(1.0, height)));
            });
            column
                .child(TestItem::new(0.0))
                .insert(Fill::new('Y', Size::new(1.0, 1.0)));
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
        |ui: Ui<'_>| {
            let mut overlay = ui.layout(Overlay);
            overlay.child(TestItem::default()).build(|ui: Ui<'_>| {
                let mut child = ui
                    .layout(Overlay)
                    .transition(Transition::new(Duration::from_secs(1)).width());
                child
                    .child(TestItem::default())
                    .insert(Fill::new('X', Size::new(width, 1.0)));
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
        |ui: Ui<'_>| {
            let mut column = ui.layout(Column).offset(Point::new(0.0, 1.0));
            column.child(TestItem::new(gap)).build(|ui: Ui<'_>| {
                let mut child = ui
                    .layout(Overlay)
                    .widget_id(id)
                    .transition(Transition::new(Duration::from_secs(1)).y());
                child
                    .child(TestItem::default())
                    .insert(Fill::new('X', Size::uniform(1.0)));
            });
        },
    );
}

fn clipped_button(mut ui: Ui<'_>, id: WidgetId) -> Interaction {
    let interaction = ui.interact(id, Sense::CLICK);
    let mut root = ui.layout(Overlay);
    root.child(TestItem::default()).build(|ui: Ui<'_>| {
        let mut panel = ui.layout(Fixed(Size::new(3.0, 1.0))).clip(DiamondClip);
        panel.insert(Fill::new('P', Size::ZERO));
        panel
            .child(TestItem::default())
            .widget_id(id)
            .insert(Fill::new('C', Size::new(5.0, 1.0)));
    });
    interaction
}

fn scene(ui: Ui<'_>) {
    let mut column = ui.layout(Column);
    column
        .child(TestItem::new(0.0))
        .insert(Fill::new('A', Size::new(3.0, 1.0)));
    column.child(TestItem::new(1.0)).build(|ui: Ui<'_>| {
        let mut panel = ui.layout(Overlay).clip(DiamondClip);
        panel
            .child(TestItem::default())
            .insert(Fill::new('b', Size::new(5.0, 3.0)));
        panel
            .child(TestItem::default())
            .insert(Fill::new('C', Size::uniform(1.0)));
    });
}

struct PaintCount {
    count: Rc<Cell<usize>>,
    bounds_offset: Point,
}

impl Atom<AsciiPlatform> for PaintCount {
    fn measure(&self, _: &mut AsciiPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::uniform(1.0))
    }

    fn paint(&self, _: &mut AsciiPlatform, _: Rect) {
        self.count.set(self.count.get() + 1);
    }

    fn paint_bounds(&self, area: Rect) -> Rect {
        Rect {
            x: area.x + self.bounds_offset.x,
            y: area.y + self.bounds_offset.y,
            ..area
        }
    }
}

struct OwnedValue {
    area: Rc<Cell<Rect>>,
    drops: Rc<Cell<usize>>,
}

impl<R: Platform> Layout<R> for OwnedValue {
    type Item = ();

    fn layout(&self, _: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        constraints.min
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}

impl Atom<AsciiPlatform> for OwnedValue {
    fn measure(&self, _: &mut AsciiPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, platform: &mut AsciiPlatform, area: Rect) {
        self.area.set(area);
        platform.cells.fill('P');
    }

    fn paint_bounds(&self, area: Rect) -> Rect {
        area
    }
}

impl Drop for OwnedValue {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
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

impl Atom<AsciiPlatform> for Fill {
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

    fn paint_bounds(&self, area: Rect) -> Rect {
        area
    }
}

struct FillContent;

impl Content<AsciiPlatform> for FillContent {
    type Response = ();

    fn append(self, mut ui: Ui<'_, blit::state::Node>) {
        ui.insert(Fill::new('A', Size::new(3.0, 1.0)));
    }
}

struct PreparedText<'a>(&'a str);

impl Content<AsciiPlatform> for PreparedText<'_> {
    type Response = ();

    fn append(self, mut ui: Ui<'_, blit::state::Node>) {
        let glyph = ui.platform().prepare(self.0);
        ui.insert(Fill::new(glyph, Size::new(2.0, 1.0)));
    }
}

struct Pair(char, char);

impl Content<AsciiPlatform> for Pair {
    type Response = ();

    fn append(self, mut ui: Ui<'_, blit::state::Node>) {
        ui.insert(Fill::new(self.0, Size::new(2.0, 1.0)));
        ui.insert(Fill::new(self.1, Size::new(2.0, 1.0)));
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

struct TestItem {
    gap_before: f32,
    width: Sizing,
    height: Sizing,
}

impl TestItem {
    fn new(gap_before: f32) -> Self {
        Self {
            gap_before,
            width: Sizing::fit(),
            height: Sizing::fit(),
        }
    }

    fn fixed(width: f32, height: f32) -> Self {
        Self::new(0.0)
            .width(Sizing::fixed(width))
            .height(Sizing::fixed(height))
    }

    fn width(mut self, width: Sizing) -> Self {
        self.width = width;
        self
    }

    fn height(mut self, height: Sizing) -> Self {
        self.height = height;
        self
    }
}

impl Default for TestItem {
    fn default() -> Self {
        Self::new(0.0)
    }
}

fn override_test_item(item: &mut TestItem, width: Option<f32>, height: Option<f32>) -> bool {
    if let Some(extent) = width {
        item.width = Sizing::fixed(extent);
    }
    if let Some(extent) = height {
        item.height = Sizing::fixed(extent);
    }
    true
}

#[derive(Clone, Copy)]
struct Column;

impl<R: Platform> Layout<R> for Column {
    type Item = TestItem;

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut children = Size::ZERO;
        for child in cx.children() {
            let item = cx.item(child);
            let size = resolve_child(cx, child, constraints.max, true, false);
            children.width = children.width.max(size.width);
            children.height += item.gap_before + size.height;
        }

        let size = constraints.constrain(children);
        let mut y = 0.0;
        for child in cx.children() {
            y += cx.item(child).gap_before;
            cx.set_child_position(child, Point::new(0.0, y));
            y += cx.child_size(child).height;
        }
        size
    }

    fn override_size(
        &self,
        item: &mut Self::Item,
        width: Option<f32>,
        height: Option<f32>,
    ) -> bool {
        override_test_item(item, width, height)
    }
}

#[derive(Clone, Copy)]
struct Overlay;

impl<R: Platform> Layout<R> for Overlay {
    type Item = TestItem;

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut size = Size::ZERO;
        for child in cx.children() {
            size = size.max(resolve_child(cx, child, constraints.max, true, true));
        }
        let size = constraints.constrain(size);
        for child in cx.children() {
            let child_size = cx.child_size(child);
            cx.set_child_position(
                child,
                Point::new(
                    (size.width - child_size.width) / 2.0,
                    (size.height - child_size.height) / 2.0,
                ),
            );
        }
        size
    }

    fn override_size(
        &self,
        item: &mut Self::Item,
        width: Option<f32>,
        height: Option<f32>,
    ) -> bool {
        override_test_item(item, width, height)
    }
}

#[derive(Clone, Copy)]
struct Fixed(Size);

impl<R: Platform> Layout<R> for Fixed {
    type Item = TestItem;

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let size = constraints.constrain(self.0);
        for child in cx.children() {
            resolve_child(cx, child, constraints.max, true, true);
            cx.set_child_position(child, Point::ZERO);
        }
        size
    }

    fn override_size(
        &self,
        item: &mut Self::Item,
        width: Option<f32>,
        height: Option<f32>,
    ) -> bool {
        override_test_item(item, width, height)
    }
}

fn resolve_child<R: Platform>(
    cx: &mut LayoutCx<'_, R, TestItem>,
    child: NodeId,
    available: Size,
    width_cross: bool,
    height_cross: bool,
) -> Size {
    let resolve = |sizing: Sizing, intrinsic: f32, available: f32, stretch: bool| match sizing {
        Sizing::Fit { .. } => sizing.clamp(intrinsic.min(available)),
        Sizing::Grow { .. } if stretch => sizing.clamp(available),
        Sizing::Grow { .. } => sizing.clamp(intrinsic.min(available)),
        Sizing::Fixed(size) => size.max(0.0),
        Sizing::Percent(fraction) if available.is_finite() => {
            assert!((0.0..=1.0).contains(&fraction));
            available * fraction
        }
        Sizing::Percent(_) => 0.0,
    };
    let res = cx.resolution();
    let intrinsic = cx.layout_child(child, Constraints::loose(available));
    let item = cx.item(child);
    let size = Size::new(
        resolve(
            res.sizing(Axis::Horizontal, item.width),
            intrinsic.width,
            available.width,
            width_cross,
        ),
        resolve(
            res.sizing(Axis::Vertical, item.height),
            intrinsic.height,
            available.height,
            height_cross,
        ),
    );
    cx.layout_child(child, Constraints::tight(size))
}

#[derive(Default)]
struct AsciiPlatform {
    width: usize,
    height: usize,
    cells: Vec<char>,
    diamond_clips: Vec<Rect>,
    prepared: usize,
}

impl AsciiPlatform {
    fn prepare(&mut self, text: &str) -> char {
        self.prepared += 1;
        text.chars().next().unwrap().to_ascii_uppercase()
    }

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
