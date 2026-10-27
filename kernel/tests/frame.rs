use super::*;

#[test]
fn layout_and_paint() {
    let (mut frame, mut context) = frame(Size::new(8.0, 6.0));
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut column = ui.layout(Column);
        {
            let mut transparent = column.child().layout(());
            transparent.insert(Fill('A', Size::uniform(1.0)));
            transparent.child().insert(Fill('A', Size::new(3.0, 1.0)));
            transparent.child().insert(Fill('A', Size::new(2.0, 1.0)));
        }
        column.child().item(1.0).build(|ui: Ui<'_>| {
            let mut panel = ui.layout(Overlay).clip(TestClip);
            panel.child().insert(Fill('b', Size::new(5.0, 3.0)));
            panel.child().insert(Fill('C', Size::uniform(1.0)));
        });
        column.child();
        column.child().layout(At((), |_| Point::ZERO));
    });
    assert_eq!(
        context.commands,
        [
            Paint('A', Rect::new(0.0, 0.0, 3.0, 1.0)),
            Paint('A', Rect::new(0.0, 0.0, 3.0, 1.0)),
            Paint('A', Rect::new(0.0, 0.0, 2.0, 1.0)),
            PushClip(Rect::new(0.0, 2.0, 5.0, 3.0)),
            Paint('b', Rect::new(0.0, 2.0, 5.0, 3.0)),
            Paint('C', Rect::new(2.0, 3.0, 1.0, 1.0)),
            PopClip,
        ]
    );
}

#[test]
fn content_and_atom_order() {
    let (mut frame, mut context) = frame(Size::new(5.0, 4.0));
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        ui.layout(Overlay).child().build(|mut ui: Ui<'_>| {
            ui.insert(Content("AC", Size::new(3.0, 1.0)));
            ui.insert(Fill('B', Size::new(1.0, 2.0)));
            ui.layout(()).child();
        });
    });
    assert_eq!(context.prepared, 2);
    let area = Rect::new(1.0, 1.0, 3.0, 2.0);
    assert_eq!(context.commands, [Paint('A', area), Paint('C', area), Paint('B', area)]);
}

#[test]
fn scratch_and_default_items() {
    let (mut frame, mut context) = frame(Size::uniform(4.0));
    let id = Id::new("scratch");
    for (extent, millis) in [(1.0, 0), (2.0, 0), (2.0, 500)] {
        render_inputs(
            &mut frame,
            &mut context,
            Duration::from_millis(millis),
            [],
            |ui: Ui<'_>| {
                let mut root = ui.layout(SharedItems(3));
                root.child()
                    .id(id)
                    .transition(Transition::new(Duration::from_secs(1)).size())
                    .layout(Temporary)
                    .child()
                    .layout(Temporary)
                    .child()
                    .insert(Fill('X', Size::uniform(extent)));
                root.child().layout(SharedItems(1)).child();
                root.child();
            },
        );
        assert_eq!(
            frame.area(id).unwrap().size(),
            Size::uniform(if millis == 500 { 1.5 } else { 1.0 })
        );
    }
    assert!(context.prepared >= 6);
}

#[test]
#[should_panic(expected = "too much scratch data")]
fn scratch_rejects_size_overflow() {
    let (mut frame, mut context) = frame(Size::uniform(4.0));
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        ui.scratch(usize::MAX, 0_u64);
    });
}

#[test]
fn owned_values_and_paint_bounds() {
    let result = Rc::new(Cell::new(ProbeResult::default()));
    let (mut frame, mut context) = frame(Size::new(3.0, 2.0));
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let probe = || Probe {
            result: result.clone(),
            offset: Point::ZERO,
        };
        ui.layout(probe()).insert(probe());
    });
    assert_eq!(
        result.get(),
        ProbeResult {
            paints: 1,
            drops: 2,
            area: Rect::new(0.0, 0.0, 3.0, 2.0)
        }
    );
    assert_eq!(context.commands, [Paint('P', Rect::new(0.0, 0.0, 3.0, 2.0))]);

    let results = [0; 3].map(|_| Rc::new(Cell::new(ProbeResult::default())));
    context.info = FrameInfo::new(Size::new(3.0, 1.0));
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        for (index, offset) in [Point::ZERO, Point::new(-4.0, 0.0)].into_iter().enumerate() {
            root.child().layout(At((), |_| Point::new(4.0, 0.0))).insert(Probe {
                result: results[index].clone(),
                offset,
            });
        }
        root.child()
            .layout(Fixed(Size::uniform(1.0)))
            .clip(TestClip)
            .child()
            .layout(At((), |_| Point::new(1.0, 0.0)))
            .insert(Probe {
                result: results[2].clone(),
                offset: Point::ZERO,
            });
    });
    assert_eq!(
        results
            .each_ref()
            .map(|result| (result.get().paints, result.get().drops)),
        [(0, 1), (1, 1), (0, 1)]
    );
    assert_eq!(context.commands, [Paint('P', Rect::new(4.0, 0.0, 1.0, 1.0))]);
}

#[test]
fn anchors_and_clip_parents() {
    let (mut frame, mut context) = frame(Size::new(8.0, 5.0));
    let anchor = Id::new("anchor");
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        root.child().id(anchor).insert(Fill('T', Size::uniform(2.0)));
        root.child()
            .layout(At((), |size| Point::new(size.width, size.height)))
            .relative(anchor)
            .insert(Fill('A', Size::uniform(1.0)));
    });
    assert_eq!(
        context.commands,
        [
            Paint('T', Rect::new(3.0, 1.5, 2.0, 2.0)),
            Paint('A', Rect::new(5.0, 3.5, 1.0, 1.0)),
        ]
    );
    context.info = FrameInfo::new(Size::uniform(3.0));
    for escape in [true, false] {
        render(&mut frame, &mut context, |ui: Ui<'_>| {
            let id = Id::new("panel");
            let mut root = ui.layout(Overlay);
            let mut panel = root.child().layout(Fixed(Size::uniform(3.0))).id(id).clip(TestClip);
            panel.insert(Fill('p', Size::ZERO));
            panel
                .child()
                .parent(if escape {
                    NodeTarget::Root
                } else {
                    NodeTarget::Widget(id)
                })
                .insert(Fill('L', Size::uniform(3.0)));
        });
        let area = Rect::new(0.0, 0.0, 3.0, 3.0);
        assert_eq!(
            context.commands,
            if escape {
                [PushClip(area), Paint('p', area), PopClip, Paint('L', area)]
            } else {
                [PushClip(area), Paint('p', area), Paint('L', area), PopClip]
            }
        );
    }
}

#[test]
fn visual_parent_and_hit_order() {
    let (mut frame, mut context) = frame(Size::new(7.0, 5.0));
    let popup = Id::new("popup");
    let modal = Id::new("modal");
    for open in [false, true, false] {
        let build = |mut ui: Ui<'_>| {
            let responses = [popup, modal].map(|id| ui.interact_widget(id, Press::new()));
            let mut root = ui.layout(Overlay);
            if open {
                root.child()
                    .layout(At(Full, |_| Point::ZERO))
                    .z_index(1)
                    .id(modal)
                    .insert(Fill('M', Size::ZERO));
            }
            root.child().clip(TestClip).build(|ui: Ui<'_>| {
                let outer = ui.node_id();
                ui.layout(At(Fixed(Size::uniform(5.0)), |_| Point::new(1.0, 0.0)))
                    .child()
                    .clip(TestClip)
                    .build(|ui: Ui<'_>| {
                        ui.layout(At(Fixed(Size::uniform(1.0)), |_| Point::new(2.0, 2.0)))
                            .child()
                            .layout(At(Full, |_| Point::new(-2.0, -2.0)))
                            .parent(outer)
                            .z_index(i16::MAX)
                            .id(popup)
                            .insert(Fill('P', Size::ZERO));
                    });
            });
            responses
        };
        render(&mut frame, &mut context, &build);
        assert_eq!(frame.area(popup), Some(Rect::new(1.0, 0.0, 5.0, 5.0)));
        let area = Rect::new(1.0, 0.0, 5.0, 5.0);
        if open {
            assert_eq!(
                context.commands,
                [
                    PushClip(area),
                    Paint('P', area),
                    PopClip,
                    Paint('M', Rect::new(0.0, 0.0, 7.0, 5.0))
                ]
            );
        } else {
            assert_eq!(context.commands, [PushClip(area), Paint('P', area), PopClip]);
        }
        for (x, expected) in [(0.5, [false, open]), (1.5, [!open, open])] {
            let down = render_inputs(
                &mut frame,
                &mut context,
                Duration::ZERO,
                [pointer(None, Down, [x, 2.5])],
                &build,
            );
            assert_eq!(down.map(|response| response.activated), expected);
            let up = render_inputs(
                &mut frame,
                &mut context,
                Duration::ZERO,
                [pointer(None, Up, [x, 2.5])],
                &build,
            );
            assert_eq!(up.map(|response| response.clicked), expected);
        }
    }
}
#[test]
fn targets_reject_invalid_references() {
    let cases: &[fn(Ui<'_>, Id)] = &[
        |ui, _| {
            let id = ui.node_id();
            ui.parent(id).insert(());
        },
        |ui, _| {
            let mut root = ui.layout(Overlay);
            let child = root.child().build(|ui: Ui<'_>| ui.node_id());
            root.parent(child).insert(());
        },
        |ui, id| {
            let mut root = ui.layout(Overlay);
            root.child().parent(id).insert(());
            root.id(id).insert(());
        },
        |ui, id| {
            let mut root = ui.layout(Overlay);
            root.child().layout(At((), |_| Point::ZERO)).relative(id).insert(());
            root.id(id).insert(());
        },
        |ui, id| ui.id(id).parent(id).insert(()),
        |ui, id| {
            let mut root = ui.layout(Overlay);
            root.child().id(id).insert(());
            root.parent(id).insert(());
        },
        #[cfg(debug_assertions)]
        |ui, id| {
            ui.id(id).layout(Overlay).child().id(id).insert(());
        },
        #[cfg(debug_assertions)]
        |ui, id| {
            let mut root = ui.id(id).layout(Overlay);
            root.child().id(id.child(1_u32)).insert(());
            root.child()
                .transition(Transition::new(Duration::from_secs(1)))
                .insert(());
        },
        |ui, id| {
            ui.id(id)
                .id(id.child("renamed"))
                .layout(Overlay)
                .child()
                .parent(id)
                .insert(());
        },
        #[cfg(debug_assertions)]
        |ui, id| {
            let mut root = ui.id(id).layout(Overlay);
            root.child().insert(());
            root.child().id(id.child(0_u32)).insert(());
        },
    ];
    for (case, build) in cases.into_iter().enumerate() {
        let (mut frame, mut context) = frame(Size::uniform(1.0));
        // a previous build must not satisfy a current reference
        render(&mut frame, &mut context, |ui: Ui<'_>| {
            ui.id(Id::new("target")).insert(())
        });
        let result = catch_unwind(AssertUnwindSafe(|| {
            render(&mut frame, &mut context, |ui: Ui<'_>| build(ui, Id::new("target")));
        }));
        assert!(result.is_err(), "case {case}");
    }
}

#[cfg(debug_assertions)]
#[test]
fn node_targets_reject_previous_renders() {
    for anchor in [false, true] {
        let (mut frame, mut context) = frame(Size::uniform(1.0));
        let previous = render(&mut frame, &mut context, |ui: Ui<'_>| ui.node_id());
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                render(&mut frame, &mut context, |ui: Ui<'_>| {
                    let mut root = ui.layout(Overlay);
                    if anchor {
                        root.child()
                            .layout(At((), |_| Point::ZERO))
                            .relative(previous)
                            .insert(());
                    } else {
                        root.child().parent(previous).insert(());
                    }
                });
            }))
            .is_err()
        );
    }
}

#[test]
fn bindings_and_geometry() {
    let (mut frame, mut context) = frame(Size::uniform(10.0));
    let a = Id::new("a");
    let b = Id::new("b");
    let mut retained_a = false;
    let mut retained_b = false;
    for (count, query) in [
        (0, false),
        (3, false),
        (1, true),
        (0, true),
        (2, true),
        (2, false),
        (3, true),
    ] {
        render(&mut frame, &mut context, |mut ui: Ui<'_>| {
            if query {
                assert_eq!(ui.area(a).is_some(), retained_a);
                assert_eq!(ui.area(b).is_some(), retained_b);
            }
            ui.interact_widget(b, Press::new());
            let mut root = ui.layout(Overlay);
            for _ in 0..count {
                root.child().insert(());
            }
            if count != 0 {
                root.child().id(b).insert(());
                root.child().id(a).parent(b).insert(());
                root.child().layout(At((), |_| Point::ZERO)).relative(a).insert(());
            }
        });
        assert_eq!(frame.area(a).is_some(), count != 0);
        assert_eq!(frame.area(b).is_some(), count != 0);
        retained_a = count != 0 && query;
        retained_b = count != 0 && query;
    }
}
