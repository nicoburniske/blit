use std::{cell::Cell, rc::Rc, time::Duration};

use blit::{
    Atom, Axis, Clip, Constraints, Content, Easing, Frame, FrameInfo, Input, Interaction, Layout, LayoutCx, Modifiers,
    NodeId, NodeTarget, Point, PointerButton, Rect, Sense, Size, Sizing, Transition, Widget, WidgetId,
};

type Ui<'a, S = blit::state::Build> = blit::Ui<'a, AsciiContext, S>;

#[test]
fn animations_and_timers_schedule_frames() {
    let (mut frame, mut context) = frame(Size::uniform(1.0));
    let animation = WidgetId::new("animation");
    let timer = WidgetId::new("timer");
    let mut value = 0.0;
    let mut fired = false;

    for (time, target) in [
        (Duration::ZERO, 0.0),
        (Duration::ZERO, 1.0),
        (Duration::from_millis(500), 1.0),
    ] {
        render_inputs(&mut frame, &mut context, time, [], |mut ui: Ui<'_>| {
            value = ui.animate(animation, target, Duration::from_secs(1), Easing::Linear);
            fired = ui.timer(timer, Duration::from_millis(500));
            ui.insert(Fill::new('X', Size::uniform(1.0)));
        });
    }

    assert_eq!(value, 0.5);
    assert!(fired);
    assert!(frame.has_pending_redraw());
}

#[test]
fn lays_out_and_paints_external_atoms() {
    let (mut frame, mut context) = frame(Size::new(8.0, 6.0));

    render(&mut frame, &mut context, scene);

    assert_eq!(
        context.contents(),
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
    let (mut frame, mut context) = frame(Size::new(3.0, 1.0));

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        root.target(NodeTarget::Parent)
            .layout(At::new(4.0, 0.0))
            .child()
            .insert(PaintCount {
                count: culled.clone(),
                bounds_offset: Point::ZERO,
            });
        root.target(NodeTarget::Parent)
            .layout(At::new(4.0, 0.0))
            .child()
            .insert(PaintCount {
                count: overflow.clone(),
                bounds_offset: Point::new(-4.0, 0.0),
            });
        root.child().item(TestItem::fixed(1.0, 1.0)).build(|ui: Ui<'_>| {
            let mut panel = ui.layout(Overlay).clip(DiamondClip);
            panel
                .target(NodeTarget::Parent)
                .layout(At::new(1.0, 0.0))
                .child()
                .insert(PaintCount {
                    count: clipped.clone(),
                    bounds_offset: Point::ZERO,
                });
        });
    });

    assert_eq!(culled.get(), 0);
    assert_eq!(clipped.get(), 0);
    assert_eq!(overflow.get(), 1);
}

#[test]
fn leaf_atoms_measure_and_paint_in_order() {
    let (mut frame, mut context) = frame(Size::new(5.0, 4.0));

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        root.child().build(|mut ui: Ui<'_>| {
            ui.insert(());
            ui.insert(FillContent);
            ui.insert(Fill::new('B', Size::new(1.0, 2.0)));
        });
    });

    assert_eq!(context.contents(), "     \n BBB \n BBB \n     ");
}

#[test]
fn content_works_before_layout_on_current_and_fresh_nodes() {
    let (mut frame, mut context) = frame(Size::new(2.0, 1.0));

    render(&mut frame, &mut context, |mut ui: Ui<'_>| {
        ui.insert(PreparedText("a"));
        let mut root = ui.layout(Overlay);
        root.insert(Pair('x', 'y'));
        root.child().insert(PreparedText("b"));
    });

    assert_eq!(context.prepared, 2);
    assert_eq!(context.contents(), "BB");
}

#[test]
fn empty_and_out_of_flow_children_are_valid() {
    let (mut frame, mut context) = frame(Size::uniform(1.0));

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Column);
        root.child()
            .item(TestItem::new(0.0).width(Sizing::grow()).height(Sizing::grow()));
        root.target(NodeTarget::Parent)
            .layout(At::new(0.0, 0.0))
            .child()
            .insert(());
    });
}

#[test]
fn owned_frame_values_use_resolved_area_and_drop() {
    let area = Rc::new(Cell::new(Rect::default()));
    let drops = Rc::new(Cell::new(0));
    let (mut frame, mut context) = frame(Size::new(3.0, 2.0));

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let value = || OwnedValue {
            area: area.clone(),
            drops: drops.clone(),
        };
        ui.layout(value()).insert(value());
    });

    assert_eq!(area.get(), Rect::new(0.0, 0.0, 3.0, 2.0));
    assert_eq!(context.contents(), "PPP\nPPP");
    assert_eq!(drops.get(), 2);
}

#[test]
fn default_children_share_one_item() {
    struct SharedDefault {
        children: Rc<Cell<usize>>,
        default: Rc<Cell<Option<*const ()>>>,
    }

    impl Layout<AsciiContext> for SharedDefault {
        type Item = Rc<()>;

        fn layout(&self, cx: &mut LayoutCx<'_, AsciiContext, Rc<()>>, constraints: Constraints) -> Size {
            let mut count = 0;
            for child in cx.children() {
                let item = Rc::as_ptr(cx.item(child));
                if let Some(default) = self.default.get() {
                    assert_eq!(item, default);
                } else {
                    self.default.set(Some(item));
                }
                cx.layout_child(child, Constraints::loose(constraints.max));
                cx.set_position(child, Point::ZERO);
                count += 1;
            }
            self.children.set(count);
            constraints.min
        }
    }

    let children = Rc::new(Cell::new(0));
    let (mut frame, mut context) = frame(Size::uniform(4.0));
    let id = WidgetId::new("shared default transition");

    for (extent, time) in [
        (1.0, Duration::ZERO),
        (2.0, Duration::ZERO),
        (2.0, Duration::from_millis(500)),
    ] {
        render_inputs(&mut frame, &mut context, time, [], |ui: Ui<'_>| {
            let default = Rc::new(Cell::new(None));
            let layout = || SharedDefault {
                children: children.clone(),
                default: default.clone(),
            };
            let mut root = ui.layout(layout());
            root.child()
                .widget_id(id)
                .transition(Transition::new(Duration::from_secs(1)).size())
                .insert(Fill::new('X', Size::uniform(extent)));
            root.child().layout(layout()).child();
            root.child();
        });
    }

    assert_eq!(children.get(), 3);
}

#[test]
fn resolves_named_anchors_and_clipping() {
    let (mut frame, mut context) = frame(Size::new(8.0, 5.0));

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut overlay = ui.layout(Overlay);
        let target = WidgetId::new("anchor");
        overlay
            .child()
            .widget_id(target)
            .insert(Fill::new('T', Size::uniform(2.0)));
        overlay
            .target(target)
            .layout(At::new(0.0, 0.0).anchors(Point::new(1.0, 1.0), Point::ZERO))
            .child()
            .insert(Fill::new('A', Size::uniform(1.0)));
    });

    assert_eq!(
        context.contents(),
        concat!("        \n", "   TT   \n", "   TT   \n", "     A  \n", "        ",)
    );

    let info = FrameInfo::new(Size::uniform(3.0));
    context = AsciiContext::new(info);
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        root.child().build(|ui: Ui<'_>| {
            let mut panel = ui.layout(Fixed(Size::uniform(3.0))).clip(DiamondClip);
            panel.insert(Fill::new('p', Size::ZERO));
            panel
                .child()
                .visual_parent(NodeTarget::Root)
                .insert(Fill::new('L', Size::uniform(3.0)));
        });
    });
    assert_eq!(context.contents(), "LLL\nLLL\nLLL");

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        root.child().build(|ui: Ui<'_>| {
            let panel_id = WidgetId::new("panel");
            let mut panel = ui
                .layout(Fixed(Size::uniform(3.0)))
                .widget_id(panel_id)
                .clip(DiamondClip);
            panel.insert(Fill::new('p', Size::ZERO));
            panel
                .child()
                .visual_parent(panel_id)
                .insert(Fill::new('L', Size::uniform(3.0)));
        });
    });
    assert_eq!(context.contents(), " L \nLLL\n L ");
}

#[test]
fn visual_parent_preserves_outer_clip_and_supplies_out_of_flow_size() {
    let (mut frame, mut context) = frame(Size::new(7.0, 5.0));
    let popup_id = WidgetId::new("popup");
    let build = |mut ui: Ui<'_>| {
        let response = ui.interact_widget(popup_id, Sense::CLICK);
        let mut root = ui.layout(Overlay);
        root.target(NodeTarget::Parent)
            .layout(At::new(1.0, 0.0).width(Sizing::fixed(5.0)).height(Sizing::fixed(5.0)))
            .child()
            .clip(DiamondClip)
            .build(|ui: Ui<'_>| {
                let outer_id = ui.id();
                let mut outer = ui.layout(Overlay);
                outer
                    .target(NodeTarget::Parent)
                    .layout(At::new(2.0, 2.0).width(Sizing::fixed(1.0)).height(Sizing::fixed(1.0)))
                    .child()
                    .clip(DiamondClip)
                    .build(|ui: Ui<'_>| {
                        ui.layout(Overlay)
                            .target(NodeTarget::Parent)
                            .visual_parent(outer_id)
                            .z_index(1)
                            .layout(At::new(-2.0, -2.0).width(Sizing::grow()).height(Sizing::grow()))
                            .child()
                            .widget_id(popup_id)
                            .insert(Fill::new('P', Size::ZERO));
                    });
            });
        response
    };
    render(&mut frame, &mut context, &build);
    assert_eq!(frame.geometry(popup_id), Some(Rect::new(1.0, 0.0, 5.0, 5.0)));
    assert_eq!(context.contents(), "   P   \n  PPP  \n PPPPP \n  PPP  \n   P   ");

    let mut active = Vec::new();
    render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [
            Input::PointerDown {
                position: Point::new(0.5, 2.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
            Input::PointerUp {
                position: Point::new(0.5, 2.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
            Input::PointerDown {
                position: Point::new(1.5, 2.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
        ],
        |ui| active.push(build(ui).active),
    );
    assert_eq!(active, [false, false, true]);
}

#[test]
fn paint_and_interaction_follow_visual_groups() {
    let (mut frame, mut context) = frame(Size::new(3.0, 1.0));
    let size = context.info.size;
    let ids = ["background", "badge", "popup"].map(WidgetId::new);
    let canvas_id = WidgetId::new("canvas");

    for open in [false, true, false] {
        let build = |mut ui: Ui<'_>| {
            let responses = ids.map(|id| ui.interact_widget(id, Sense::CLICK));
            let mut root = ui.widget_id(ids[0]).layout(Overlay);
            if open {
                let mut modal_target = root
                    .target(NodeTarget::Parent)
                    .z_index(1)
                    .layout(At::new(0.0, 0.0).width(Sizing::grow()).height(Sizing::grow()));
                let mut modal = modal_target.child().layout(Overlay);
                modal.insert(Fill::new('D', Size::ZERO));
                modal
                    .target(NodeTarget::Parent)
                    .layout(At::new(0.0, 0.0))
                    .child()
                    .widget_id(ids[2])
                    .insert(Fill::new('M', Size::uniform(1.0)));
            }
            root.child()
                .item(TestItem::fixed(3.0, 1.0))
                .widget_id(canvas_id)
                .build(|ui: Ui<'_>| {
                    ui.layout(Overlay)
                        .child()
                        .item(TestItem::fixed(3.0, 1.0))
                        .build(|ui: Ui<'_>| {
                            let mut rect = ui.layout(Overlay);
                            let mut badge = rect.target(NodeTarget::Parent);
                            if open {
                                badge = badge.visual_parent(canvas_id).z_index(i16::MAX);
                            }
                            badge
                                .layout(At::new(0.0, 0.0))
                                .child()
                                .widget_id(ids[1])
                                .insert(Fill::new('A', size));
                        });
                });
            root.insert(Fill::new('C', size));
            responses
        };
        render(&mut frame, &mut context, build);
        assert_eq!(
            context.contents(),
            if open { "MDD" } else { "AAA" },
            "the badge stays below the modal backdrop, and content paints above it",
        );
        render_inputs(
            &mut frame,
            &mut context,
            Duration::ZERO,
            [
                Input::PointerDown {
                    position: Point::new(0.5, 0.5),
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                },
                Input::PointerUp {
                    position: Point::new(0.5, 0.5),
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                    leave: false,
                },
            ],
            |ui: Ui<'_>| {
                let down = matches!(ui.input(), Input::PointerDown { .. });
                assert_eq!(
                    build(ui).map(|response| if down { response.activated } else { response.clicked }),
                    [false, !open, open],
                    "clicks follow paint order even when the background is named last",
                );
            },
        );
    }
}

#[test]
fn transitions_relayout_animated_sizes() {
    let (mut frame, mut context) = frame(Size::new(1.0, 4.0));
    let id = WidgetId::new("transition");

    transition_scene(&mut frame, &mut context, id, 1.0, Duration::ZERO);
    assert_eq!(context.contents(), "X\nY\n \n ");

    transition_scene(&mut frame, &mut context, id, 3.0, Duration::ZERO);
    assert_eq!(context.contents(), "X\nY\n \n ");
    assert!(frame.has_pending_redraw());

    transition_scene(&mut frame, &mut context, id, 3.0, Duration::from_millis(500));
    assert_eq!(context.contents(), "X\nX\nY\n ");

    transition_scene(&mut frame, &mut context, id, 3.0, Duration::from_secs(1));
    assert_eq!(context.contents(), "X\nX\nX\nY");
    assert!(!frame.has_pending_redraw());
}

#[test]
fn size_transitions_override_child_constraints() {
    struct Loose;

    impl<C> Layout<C> for Loose {
        type Item = ();

        fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> Size {
            let child = cx.children().next().unwrap();
            let size = cx.layout_child(child, Constraints::loose(constraints.max));
            cx.set_position(child, Point::ZERO);
            constraints.constrain(size)
        }
    }

    let (mut frame, mut context) = frame(Size::uniform(4.0));
    let id = WidgetId::new("constraint transition");
    let mut render = |extent, time| {
        render_inputs(&mut frame, &mut context, time, [Input::None], |ui: Ui<'_>| {
            ui.layout(Loose)
                .child()
                .widget_id(id)
                .transition(Transition::new(Duration::from_secs(1)).size())
                .insert(Fill::new('X', Size::uniform(extent)));
        });
        (frame.geometry(id).unwrap().size(), frame.has_pending_redraw())
    };

    render(1.0, Duration::ZERO);
    assert_eq!(render(2.0, Duration::ZERO), (Size::uniform(1.0), true));
    assert_eq!(render(2.0, Duration::from_millis(500)), (Size::uniform(1.5), true));
    assert_eq!(render(2.0, Duration::from_secs(1)), (Size::uniform(2.0), false));
}

#[test]
fn out_of_flow_size_transitions_remain_fractional() {
    let (mut frame, mut context) = frame(Size::new(5.0, 1.0));
    let id = WidgetId::new("out of flow transition");
    let child_id = WidgetId::new("out of flow transition child");
    let fixed_id = WidgetId::new("out of flow fixed transition");
    let mut render = |width, time| {
        render_inputs(&mut frame, &mut context, time, [Input::None], |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            root.target(NodeTarget::Parent)
                .widget_id(id)
                .transition(Transition::new(Duration::from_secs(1)).width())
                .layout(At::new(0.0, 0.0).width(Sizing::fixed(width)).height(Sizing::fixed(1.0)))
                .child()
                .widget_id(child_id)
                .insert(Fill::new('X', Size::uniform(1.0)));
            root.target(NodeTarget::Parent)
                .widget_id(fixed_id)
                .transition(Transition::new(Duration::from_secs(1)).width())
                .layout(Fixed(Size::new(width, 1.0)));
        });
        [id, child_id, fixed_id].map(|id| frame.geometry(id).unwrap().width)
    };

    render(1.0, Duration::ZERO);
    render(4.0, Duration::ZERO);
    assert_eq!(render(4.0, Duration::from_millis(500)), [2.5; 3]);
}

#[test]
fn out_of_flow_leaf_size_transitions() {
    let (mut frame, mut context) = frame(Size::new(5.0, 1.0));
    let id = WidgetId::new("out of flow leaf");
    let mut render = |width, time| {
        render_inputs(&mut frame, &mut context, time, [Input::None], |ui: Ui<'_>| {
            ui.layout(Overlay)
                .target(NodeTarget::Root)
                .widget_id(id)
                .transition(Transition::new(Duration::from_secs(1)).width())
                .insert(Fill::new('X', Size::new(width, 1.0)));
        });
        frame.geometry(id).unwrap().width
    };

    assert_eq!(render(1.0, Duration::ZERO), 1.0);
    assert_eq!(render(3.0, Duration::ZERO), 1.0);
    assert_eq!(render(3.0, Duration::from_millis(500)), 2.0);
    assert_eq!(render(3.0, Duration::from_secs(1)), 3.0);
}

#[test]
fn transitions_resolved_positions() {
    let (mut frame, mut context) = frame(Size::new(1.0, 4.0));
    let id = WidgetId::new("position transition");

    position_transition_scene(&mut frame, &mut context, id, 0.0, Duration::ZERO);
    position_transition_scene(&mut frame, &mut context, id, 2.0, Duration::ZERO);
    assert_eq!(context.contents(), " \nX\n \n ");

    position_transition_scene(&mut frame, &mut context, id, 2.0, Duration::from_millis(500));
    assert_eq!(context.contents(), " \n \nX\n ");

    position_transition_scene(&mut frame, &mut context, id, 2.0, Duration::from_secs(1));
    assert_eq!(context.contents(), " \n \n \nX");
}

#[test]
fn resolves_places_and_content_offsets() {
    let (mut frame, mut context) = frame(Size::new(8.0, 4.0));
    let fixed = WidgetId::new("fixed");
    let grow = WidgetId::new("grow");
    let percent = WidgetId::new("percent");
    let fit = WidgetId::new("fit");

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut overlay = ui.layout(Overlay).offset(Point::new(1.0, 0.0));
        overlay
            .child()
            .item(TestItem::fixed(3.0, 1.0))
            .widget_id(fixed)
            .insert(Fill::new('F', Size::uniform(1.0)));
        overlay
            .child()
            .item(TestItem::default().width(Sizing::grow()).height(Sizing::fixed(1.0)))
            .widget_id(grow)
            .insert(Fill::new('G', Size::uniform(1.0)));
        overlay
            .child()
            .item(
                TestItem::default()
                    .width(Sizing::percent(0.25))
                    .height(Sizing::fixed(1.0)),
            )
            .widget_id(percent)
            .insert(Fill::new('P', Size::uniform(1.0)));
        overlay
            .child()
            .item(
                TestItem::default()
                    .width(Sizing::fit_range(0.0, 3.0))
                    .height(Sizing::fixed(1.0)),
            )
            .widget_id(fit)
            .insert(Fill::new('M', Size::new(6.0, 1.0)));
    });

    assert_eq!(frame.geometry(fixed), Some(Rect::new(3.5, 1.5, 3.0, 1.0)));
    assert_eq!(frame.geometry(grow), Some(Rect::new(1.0, 1.5, 8.0, 1.0)));
    assert_eq!(frame.geometry(percent), Some(Rect::new(4.0, 1.5, 2.0, 1.0)));
    assert_eq!(frame.geometry(fit), Some(Rect::new(3.5, 1.5, 3.0, 1.0)));
}

#[test]
fn out_of_flow_positions_against_target_and_sizes_against_visual_parent() {
    let (mut frame, mut context) = frame(Size::new(10.0, 4.0));
    let id = WidgetId::new("out of flow");

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut overlay = ui.layout(Overlay);
        let target = overlay.child().build(|mut ui: Ui<'_>| {
            ui.insert(Fill::new('T', Size::new(6.0, 2.0)));
            ui.id()
        });
        overlay
            .target(target)
            .widget_id(id)
            .layout(
                At::new(0.0, 0.0)
                    .anchors(Point::new(1.0, 1.0), Point::ZERO)
                    .width(Sizing::percent(0.5))
                    .height(Sizing::grow()),
            )
            .child()
            .build(|ui: Ui<'_>| {
                let mut content = ui.layout(Overlay);
                content.insert(Fill::new('A', Size::ZERO));
            });
    });

    assert_eq!(frame.geometry(id), Some(Rect::new(8.0, 3.0, 5.0, 4.0)));
}

#[test]
fn out_of_flow_target_paints_after_its_declaring_subtree() {
    let (mut frame, mut context) = frame(Size::uniform(1.0));
    let id = WidgetId::new("popup");

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut panel = root.child().layout(Overlay);
        panel
            .target(NodeTarget::Root)
            .visual_parent(NodeTarget::Root)
            .layout(At::new(0.0, 0.0))
            .child()
            .widget_id(id)
            .insert(Fill::new('P', Size::uniform(1.0)));
        panel.child().insert(Fill::new('Q', Size::uniform(1.0)));
    });

    assert_eq!(context.contents(), "P");
    assert_eq!(frame.geometry(id), Some(Rect::new(0.0, 0.0, 1.0, 1.0)));
}

#[test]
fn transitions_use_automatic_widget_ids() {
    let (mut frame, mut context) = frame(Size::new(4.0, 1.0));

    unidentified_transition_scene(&mut frame, &mut context, 1.0);
    unidentified_transition_scene(&mut frame, &mut context, 3.0);

    assert_eq!(context.contents(), " X  ");
    assert!(frame.has_pending_redraw());
}

#[test]
fn targets_reject_invalid_references() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let cases: &[fn(Ui<'_>, WidgetId)] = &[
        |ui, _| {
            let id = ui.id();
            ui.visual_parent(id).insert(());
        },
        |ui, _| {
            let mut root = ui.layout(Overlay);
            let child = root.child().build(|ui: Ui<'_>| ui.id());
            root.visual_parent(child).insert(());
        },
        |ui, id| {
            let mut root = ui.layout(Overlay);
            root.child().visual_parent(id).insert(());
            root.widget_id(id).insert(());
        },
        |ui, id| {
            let mut root = ui.layout(Overlay);
            root.target(id).insert(());
            root.widget_id(id).insert(());
        },
        |ui, id| ui.widget_id(id).visual_parent(id).insert(()),
        |ui, id| {
            let mut root = ui.layout(Overlay);
            root.child().widget_id(id).insert(());
            root.visual_parent(id).insert(());
        },
        #[cfg(debug_assertions)]
        |ui, id| {
            ui.widget_id(id).layout(Overlay).child().widget_id(id).insert(());
        },
        #[cfg(debug_assertions)]
        |ui, id| {
            let mut root = ui.widget_id(id).layout(Overlay);
            root.child().widget_id(id.child(1_u32)).insert(());
            root.child()
                .transition(Transition::new(Duration::from_secs(1)))
                .insert(());
        },
        |ui, id| {
            ui.widget_id(id)
                .widget_id(id.child("renamed"))
                .layout(Overlay)
                .child()
                .visual_parent(id)
                .insert(());
        },
        #[cfg(debug_assertions)]
        |ui, id| {
            let mut root = ui.widget_id(id).layout(Overlay);
            root.child().insert(());
            root.child().widget_id(id.child(0_u32)).insert(());
        },
    ];
    for (case, build) in cases.iter().enumerate() {
        let (mut frame, mut context) = frame(Size::uniform(1.0));
        // a previous build must not satisfy a current reference
        render(&mut frame, &mut context, |ui: Ui<'_>| {
            ui.widget_id(WidgetId::new("target")).insert(())
        });
        let result = catch_unwind(AssertUnwindSafe(|| {
            render(&mut frame, &mut context, |ui: Ui<'_>| {
                build(ui, WidgetId::new("target"))
            });
        }));
        assert!(result.is_err(), "case {case}");
    }
}

#[cfg(debug_assertions)]
#[test]
fn node_targets_reject_previous_renders() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    for anchor in [false, true] {
        let (mut frame, mut context) = frame(Size::uniform(1.0));
        let previous = render(&mut frame, &mut context, |ui: Ui<'_>| ui.id());
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                render(&mut frame, &mut context, |ui: Ui<'_>| {
                    let mut root = ui.layout(Overlay);
                    if anchor {
                        root.target(previous).insert(());
                    } else {
                        root.child().visual_parent(previous).insert(());
                    }
                });
            }))
            .is_err()
        );
    }
}

#[test]
fn named_bindings_follow_each_build() {
    let (mut frame, mut context) = frame(Size::uniform(10.0));
    let a = WidgetId::new("a");
    let b = WidgetId::new("b");
    // change node indices and remove names before bringing them back
    for count in [0, 3, 1, 0, 2] {
        render_inputs(
            &mut frame,
            &mut context,
            Duration::ZERO,
            [Input::None; 2],
            |ui: Ui<'_>| {
                let mut root = ui.layout(Overlay);
                for _ in 0..count {
                    root.child().insert(());
                }
                if count == 0 {
                    return;
                }
                root.child().widget_id(b).insert(());
                root.child().widget_id(a).visual_parent(b).insert(());
                root.target(a).insert(());
            },
        );
        assert_eq!(frame.geometry(a).is_some(), count != 0);
        assert_eq!(frame.geometry(b).is_some(), count != 0);
    }
}

#[test]
fn geometry_retains_only_requested_nodes() {
    let (mut frame, mut context) = frame(Size::uniform(10.0));
    let queried = WidgetId::new("queried");
    let interactive = WidgetId::new("interactive");

    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        root.geometry(interactive);
        root.interact_widget(interactive, Sense::CLICK);
        root.child().widget_id(queried).insert(());
        root.child().widget_id(interactive).insert(());
    });
    assert!(frame.geometry(queried).is_some());

    render(&mut frame, &mut context, |mut ui: Ui<'_>| {
        assert_eq!(ui.geometry(queried), None);
        assert!(ui.geometry(interactive).is_some());
        let mut root = ui.layout(Overlay);
        root.child().widget_id(queried).insert(());
    });

    render(&mut frame, &mut context, |mut ui: Ui<'_>| {
        assert!(ui.geometry(queried).is_some());
        ui.insert(());
    });
}

#[test]
fn interaction_is_bounded_by_clip_rectangles() {
    let (mut frame, mut context) = frame(Size::new(5.0, 1.0));

    render(&mut frame, &mut context, clipped_button);

    let mut active = Vec::new();
    render_inputs(
        &mut frame,
        &mut context,
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
            Input::Scroll {
                position: Point::new(2.5, 0.5),
                delta_x: 0.0,
                delta_y: 1.0,
                modifiers: Modifiers::NONE,
                continuous: false,
                phase: blit::ScrollPhase::Moved,
            },
        ],
        |ui: Ui<'_>| active.push(clipped_button(ui)),
    );

    assert!(!active[0].active && !active[1].active && active[2].active && active[3].active);
    assert!(active[3].scroll.is_some());
}

fn transition_scene(
    frame: &mut Frame<AsciiContext>,
    context: &mut AsciiContext,
    id: WidgetId,
    height: f32,
    time: Duration,
) {
    render_inputs(frame, context, time, [Input::None], |ui: Ui<'_>| {
        let mut column = ui.layout(Column);
        column.child().item(TestItem::new(0.0)).build(|ui: Ui<'_>| {
            let mut child = ui
                .layout(Overlay)
                .transition(Transition::new(Duration::from_secs(1)).height())
                .widget_id(id);
            child.child().insert(Fill::new('X', Size::new(1.0, height)));
        });
        column
            .child()
            .item(TestItem::new(0.0))
            .insert(Fill::new('Y', Size::new(1.0, 1.0)));
    });
}

fn unidentified_transition_scene(frame: &mut Frame<AsciiContext>, context: &mut AsciiContext, width: f32) {
    render_inputs(frame, context, Duration::ZERO, [Input::None], |ui: Ui<'_>| {
        let mut overlay = ui.layout(Overlay);
        overlay.child().build(|ui: Ui<'_>| {
            let mut child = ui
                .layout(Overlay)
                .transition(Transition::new(Duration::from_secs(1)).width());
            child.child().insert(Fill::new('X', Size::new(width, 1.0)));
        });
    });
}

fn position_transition_scene(
    frame: &mut Frame<AsciiContext>,
    context: &mut AsciiContext,
    id: WidgetId,
    gap: f32,
    time: Duration,
) {
    render_inputs(frame, context, time, [Input::None], |ui: Ui<'_>| {
        let mut column = ui.layout(Column).offset(Point::new(0.0, 1.0));
        column.child().item(TestItem::new(gap)).build(|ui: Ui<'_>| {
            let mut child = ui
                .layout(Overlay)
                .transition(Transition::new(Duration::from_secs(1)).y())
                .widget_id(id);
            child.child().insert(Fill::new('X', Size::uniform(1.0)));
        });
    });
}

fn clipped_button(ui: Ui<'_>) -> Interaction {
    let mut root = ui.layout(Overlay);
    root.child().build(|ui: Ui<'_>| {
        let mut panel = ui.layout(Fixed(Size::new(3.0, 1.0))).clip(DiamondClip);
        panel.insert(Fill::new('P', Size::ZERO));
        panel.child().build(|mut ui: Ui<'_>| {
            let interaction = ui.interact(Sense::CLICK);
            ui.interact(Sense::SCROLL);
            ui.insert(Fill::new('C', Size::new(5.0, 1.0)));
            interaction
        })
    })
}

fn scene(ui: Ui<'_>) {
    let mut column = ui.layout(Column);
    column
        .child()
        .item(TestItem::new(0.0))
        .insert(Fill::new('A', Size::new(3.0, 1.0)));
    column.child().item(TestItem::new(1.0)).build(|ui: Ui<'_>| {
        let mut panel = ui.layout(Overlay).clip(DiamondClip);
        panel.child().insert(Fill::new('b', Size::new(5.0, 3.0)));
        panel.child().insert(Fill::new('C', Size::uniform(1.0)));
    });
}

struct PaintCount {
    count: Rc<Cell<usize>>,
    bounds_offset: Point,
}

impl Atom<AsciiContext> for PaintCount {
    fn measure(&self, _: &mut AsciiContext, constraints: Constraints) -> Size {
        constraints.constrain(Size::uniform(1.0))
    }

    fn paint(&self, _: &mut AsciiContext, _: Rect) {
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

impl<C> Layout<C> for OwnedValue {
    type Item = ();

    fn layout(&self, _: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> Size {
        constraints.min
    }
}

impl Atom<AsciiContext> for OwnedValue {
    fn measure(&self, _: &mut AsciiContext, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, context: &mut AsciiContext, area: Rect) {
        self.area.set(area);
        context.cells.fill('P');
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

impl Atom<AsciiContext> for Fill {
    fn measure(&self, _: &mut AsciiContext, constraints: Constraints) -> Size {
        constraints.constrain(self.size)
    }

    fn paint(&self, context: &mut AsciiContext, area: Rect) {
        let left = area.x.max(0.0) as usize;
        let top = area.y.max(0.0) as usize;
        let right = (area.x + area.width).min(context.width as f32) as usize;
        let bottom = (area.y + area.height).min(context.height as f32) as usize;
        for y in top..bottom {
            for x in left..right {
                let point = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                if !context
                    .diamond_clips
                    .iter()
                    .all(|area| DiamondClip::contains_point(*area, point))
                {
                    continue;
                }
                context.cells[y * context.width + x] = self.glyph;
            }
        }
    }

    fn paint_bounds(&self, area: Rect) -> Rect {
        area
    }
}

struct FillContent;

impl Content<AsciiContext> for FillContent {
    type Response = ();

    fn append(self, mut ui: Ui<'_, blit::state::Node>) {
        ui.insert(Fill::new('A', Size::new(3.0, 1.0)));
    }
}

struct PreparedText<'a>(&'a str);

impl Content<AsciiContext> for PreparedText<'_> {
    type Response = ();

    fn append(self, mut ui: Ui<'_, blit::state::Node>) {
        let glyph = ui.context().prepare(self.0);
        ui.insert(Fill::new(glyph, Size::new(2.0, 1.0)));
    }
}

struct Pair(char, char);

impl Content<AsciiContext> for Pair {
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

impl Clip<AsciiContext> for DiamondClip {
    fn push(&self, context: &mut AsciiContext, area: Rect) {
        context.diamond_clips.push(area);
    }

    fn pop(&self, context: &mut AsciiContext) {
        context.diamond_clips.pop().expect("clip stack is empty");
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
        Self::new(0.0).width(Sizing::fixed(width)).height(Sizing::fixed(height))
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

#[derive(Clone, Copy)]
struct Column;

impl<C> Layout<C> for Column {
    type Item = TestItem;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> Size {
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
            cx.set_position(child, Point::new(0.0, y));
            y += cx.size(child).height;
        }
        size
    }
}

#[derive(Clone, Copy)]
struct Overlay;

impl<C> Layout<C> for Overlay {
    type Item = TestItem;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> Size {
        let mut size = Size::ZERO;
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
struct At {
    offset: Point,
    target_anchor: Point,
    child_anchor: Point,
    width: Sizing,
    height: Sizing,
}

impl At {
    fn new(x: f32, y: f32) -> Self {
        Self {
            offset: Point::new(x, y),
            target_anchor: Point::ZERO,
            child_anchor: Point::ZERO,
            width: Sizing::fit(),
            height: Sizing::fit(),
        }
    }

    fn anchors(mut self, target: Point, child: Point) -> Self {
        self.target_anchor = target;
        self.child_anchor = child;
        self
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

impl<C> Layout<C> for At {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> Size {
        let child = cx.children().next().expect("positioned content is missing");
        let node = cx.node();
        let containing = cx.size(cx.visual_parent());
        let range = |sizing: Sizing, available: f32| match sizing {
            Sizing::Fit { min, max } => {
                let min = min.max(0.0);
                (min, max.max(min).min(available).max(min))
            }
            Sizing::Grow { .. } => {
                let size = sizing.clamp(available);
                (size, size)
            }
            Sizing::Fixed(size) => (size.max(0.0), size.max(0.0)),
            Sizing::Percent(fraction) if available.is_finite() => {
                let size = available * fraction;
                (size, size)
            }
            Sizing::Percent(_) => (0.0, 0.0),
        };
        let width = range(self.width, containing.width);
        let height = range(self.height, containing.height);
        let size = cx.layout_child(
            child,
            Constraints {
                min: bounds.constrain(Size::new(width.0, height.0)),
                max: bounds.constrain(Size::new(width.1, height.1)),
            },
        );
        let target = cx.size(cx.parent());
        cx.set_position(child, Point::ZERO);
        cx.set_position(
            node,
            Point::new(
                target.width * self.target_anchor.x - size.width * self.child_anchor.x + self.offset.x,
                target.height * self.target_anchor.y - size.height * self.child_anchor.y + self.offset.y,
            ),
        );
        bounds.constrain(size)
    }
}

#[derive(Clone, Copy)]
struct Fixed(Size);

impl<C> Layout<C> for Fixed {
    type Item = TestItem;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> Size {
        let size = constraints.constrain(self.0);
        for child in cx.children() {
            resolve_child(cx, child, constraints.max, true, true);
            cx.set_position(child, Point::ZERO);
        }
        size
    }
}

fn resolve_child<C>(
    cx: &mut LayoutCx<'_, C, TestItem>,
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
    let intrinsic = cx.layout_child(child, Constraints::loose(available));
    let item = cx.item(child);
    let size = Size::new(
        resolve(
            cx.resolve_sizing(child, Axis::Horizontal, item.width),
            intrinsic.width,
            available.width,
            width_cross,
        ),
        resolve(
            cx.resolve_sizing(child, Axis::Vertical, item.height),
            intrinsic.height,
            available.height,
            height_cross,
        ),
    );
    cx.layout_child(child, Constraints::tight(size))
}

struct AsciiContext {
    info: FrameInfo,
    width: usize,
    height: usize,
    cells: Vec<char>,
    diamond_clips: Vec<Rect>,
    prepared: usize,
}

impl AsciiContext {
    fn new(info: FrameInfo) -> Self {
        Self {
            info,
            width: info.size.width as usize,
            height: info.size.height as usize,
            cells: Vec::new(),
            diamond_clips: Vec::new(),
            prepared: 0,
        }
    }

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

fn frame(size: Size) -> (Frame<AsciiContext>, AsciiContext) {
    frame_info(FrameInfo::new(size))
}

fn frame_info(info: FrameInfo) -> (Frame<AsciiContext>, AsciiContext) {
    (Frame::default(), AsciiContext::new(info))
}

fn render<W: Widget<AsciiContext>>(
    frame: &mut Frame<AsciiContext>,
    context: &mut AsciiContext,
    widget: W,
) -> W::Response {
    let info = context.info;
    context.cells.clear();
    context.cells.resize(context.width * context.height, ' ');
    assert!(context.diamond_clips.is_empty());
    let output = frame.build(context, info, Duration::ZERO, Input::None, widget);
    frame.layout(context);
    frame.paint(context);
    assert!(context.diamond_clips.is_empty());
    output
}

fn render_inputs<O>(
    frame: &mut Frame<AsciiContext>,
    context: &mut AsciiContext,
    time: Duration,
    inputs: impl IntoIterator<Item = Input>,
    mut build: impl FnMut(Ui<'_>) -> O,
) {
    let info = context.info;
    context.cells.clear();
    context.cells.resize(context.width * context.height, ' ');
    assert!(context.diamond_clips.is_empty());
    let mut inputs = inputs.into_iter();
    let first = inputs.next().unwrap_or(Input::None);
    for input in std::iter::once(first).chain(inputs) {
        frame.build(context, info, time, input, &mut build);
        frame.layout(context);
    }
    frame.paint(context);
    assert!(context.diamond_clips.is_empty());
}
