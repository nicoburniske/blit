use super::*;

#[test]
fn local_bounds_apply_to_atoms_and_layouts() {
    let (mut frame, mut context) = frame(Size::new(8.0, 6.0));
    for with_layout in [false, true] {
        let measured = render(&mut frame, &mut context, |ui: Ui<'_>| {
            ui.build_measured(Constraints::loose(Size::new(5.0, 6.0)), |ui: Ui<'_>| {
                let mut ui = ui.constrain(Constraints::tight(Size::new(7.0, 2.0)));
                if with_layout {
                    ui.layout(()).insert(Fill('L', Size::uniform(1.0)));
                } else {
                    ui.insert(Fill('A', Size::uniform(1.0)));
                }
            })
            .1
        });
        assert_eq!(measured, Size::new(7.0, 2.0));
        assert_eq!(
            context.commands[0],
            Paint(if with_layout { 'L' } else { 'A' }, Rect::new(0.0, 0.0, 7.0, 2.0))
        );
    }
}

#[test]
fn local_bounds_reject_invalid_ranges() {
    let (mut frame, mut context) = frame(Size::uniform(4.0));
    for bounds in [
        Constraints::tight(Size::new(-1.0, 1.0)),
        Constraints::tight(Size::new(f32::INFINITY, 1.0)),
        Constraints::loose(Size::new(f32::NAN, 1.0)),
        Constraints {
            min: Size::uniform(2.0),
            max: Size::uniform(1.0),
        },
    ] {
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                render(&mut frame, &mut context, |ui: Ui<'_>| {
                    ui.constrain(bounds);
                });
            }))
            .is_err()
        );
    }
}

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
        column.child().place(Target::PARENT, At(|_| Point::ZERO));
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
    let id = "scratch";
    for (extent, millis) in [(1.0, 0), (2.0, 0), (2.0, 500)] {
        render_inputs(
            &mut frame,
            &mut context,
            Duration::from_millis(millis),
            [],
            |ui: Ui<'_>| {
                let mut root = ui.layout(SharedItems(3));
                root.child_id(id)
                    .motion(Motion::new(Tween::new(Duration::from_secs(1), Easing::LINEAR)).size())
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
        ui.scratch().slice::<u64>(usize::MAX);
    });
}

#[test]
fn owned_values_and_culling() {
    let result = Rc::new(Cell::new(ProbeResult::default()));
    let (mut frame, mut context) = frame(Size::new(3.0, 2.0));
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let probe = || Probe { result: result.clone() };
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

    // nodes paint past the screen since atoms may draw outside their area
    // while an empty clip hides everything under it
    let results = [0; 2].map(|_| Rc::new(Cell::new(ProbeResult::default())));
    context.info = FrameInfo::new(Size::new(3.0, 1.0));
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        root.child()
            .place(Target::PARENT, At(|_| Point::new(4.0, 0.0)))
            .insert(Probe {
                result: results[0].clone(),
            });
        root.child()
            .layout(Fixed(Size::ZERO))
            .clip(TestClip)
            .child()
            .insert(Probe {
                result: results[1].clone(),
            });
    });
    assert_eq!(
        results
            .each_ref()
            .map(|result| (result.get().paints, result.get().drops)),
        [(1, 1), (0, 1)]
    );
}

#[test]
fn anchors_and_clip_parents() {
    let (mut frame, mut context) = frame(Size::new(8.0, 5.0));
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        root.child_id("anchor").insert(Fill('T', Size::uniform(2.0)));
        root.child()
            .place(
                Target::PARENT.at("anchor"),
                At(|size| Point::new(size.width, size.height)),
            )
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
            let mut root = ui.layout(Overlay);
            let mut panel = root.child_id("panel").layout(Fixed(Size::uniform(3.0))).clip(TestClip);
            panel.insert(Fill('p', Size::ZERO));
            panel
                .child()
                .parent(if escape { Target::ROOT } else { Target::ROOT.at("panel") })
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
    let popup = Target::ROOT.scoped("popup");
    for open in [false, true, false] {
        let build = |mut ui: Ui<'_>| {
            let responses = [popup, "modal".into()].map(|target| ui.at(target).interact(WATCH)[0].is_some());
            let mut root = ui.layout(Overlay);
            if open {
                root.child_id("modal")
                    .place(Target::PARENT, Full(|_| Point::ZERO))
                    .z_index(1)
                    .insert(Fill('M', Size::ZERO));
            }
            root.child().clip(TestClip).build(|ui: Ui<'_>| {
                let outer = ui.node_id();
                ui.place(Target::PARENT, At(|_| Point::new(1.0, 0.0)))
                    .layout(Fixed(Size::uniform(5.0)))
                    .child()
                    .clip(TestClip)
                    .build(|ui: Ui<'_>| {
                        ui.place(Target::PARENT, At(|_| Point::new(2.0, 2.0)))
                            .layout(Fixed(Size::uniform(1.0)))
                            .child_in(Scope::ROOT, "popup")
                            .place(Target::PARENT, Full(|_| Point::new(-2.0, -2.0)))
                            .parent(outer)
                            .z_index(i16::MAX)
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
            assert_eq!(down, expected);
            let up = render_inputs(
                &mut frame,
                &mut context,
                Duration::ZERO,
                [pointer(None, Up, [x, 2.5])],
                &build,
            );
            assert_eq!(up, expected);
        }
    }
}
#[test]
fn targets_reject_invalid_references() {
    let cases: &[fn(Ui<'_>)] = &[
        |ui| {
            let id = ui.node_id();
            ui.parent(id).insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            let child = root.child().build(|ui: Ui<'_>| ui.node_id());
            root.parent(child).insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            root.child().parent(Target::PARENT.at("target")).insert(());
            root.child_id("target").insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            root.child()
                .place(Target::PARENT.at("target"), At(|_| Point::ZERO))
                .insert(());
            root.child_id("target").insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            root.child_id("target").parent(Target::PARENT.at("target")).insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            root.child_id("target").insert(());
            root.parent("target").insert(());
        },
        #[cfg(debug_assertions)]
        |ui| {
            let mut root = ui.layout(Overlay);
            root.child_id("target").insert(());
            root.child_id("target").insert(());
        },
        #[cfg(debug_assertions)]
        |ui| {
            let mut root = ui.layout(Overlay);
            root.child_in(Scope::ROOT, "target").insert(());
            root.child().layout(Overlay).child_in(Scope::ROOT, "target").insert(());
        },
    ];
    for (case, build) in cases.iter().enumerate() {
        let (mut frame, mut context) = frame(Size::uniform(1.0));
        // a previous build must not satisfy a current reference
        render(&mut frame, &mut context, |ui: Ui<'_>| {
            ui.layout(Overlay).child_id("target").insert(())
        });
        let result = catch_unwind(AssertUnwindSafe(|| {
            render(&mut frame, &mut context, build);
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
                        root.child().place(previous, At(|_| Point::ZERO)).insert(());
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
    let (a, b) = ("a", "b");
    // every area of the previous layout is available
    let mut laid_out = false;
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
                assert_eq!(ui.at(a).area().is_some(), laid_out);
                assert_eq!(ui.at(b).area().is_some(), laid_out);
            }
            ui.at(b).interact(WATCH);
            let mut root = ui.layout(Overlay);
            for _ in 0..count {
                root.child().insert(());
            }
            if count != 0 {
                root.child_id(b).insert(());
                root.child_id(a).parent(Target::PARENT.at(b)).insert(());
                root.child().place(Target::PARENT.at(a), At(|_| Point::ZERO)).insert(());
            }
        });
        assert_eq!(frame.area(a).is_some(), count != 0);
        assert_eq!(frame.area(b).is_some(), count != 0);
        laid_out = count != 0;
    }
}

#[test]
fn padding_insets_children_while_atoms_keep_the_full_box() {
    let (mut frame, mut context) = frame(Size::new(7.0, 5.0));
    let node = "padded";
    render(&mut frame, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut padded = root.child_id(node).layout(()).pad(2.0);
        padded.insert(Fill('B', Size::ZERO));
        padded.child().insert(Fill('C', Size::new(3.0, 1.0)));
    });
    assert_eq!(frame.area(node), Some(Rect::new(0.0, 0.0, 7.0, 5.0)));
    assert_eq!(
        context.commands,
        [
            Paint('B', Rect::new(0.0, 0.0, 7.0, 5.0)),
            Paint('C', Rect::new(2.0, 2.0, 3.0, 1.0)),
        ]
    );
}

#[test]
fn call_sites_identify_children() {
    let (mut frame, mut context) = frame(Size::uniform(1.0));
    let period = Duration::from_millis(500);
    // a child built first only later must not take the state of the children after it
    for (millis, extra, expected) in [(0, false, [false; 4]), (500, true, [true; 4])] {
        let time = Duration::from_millis(millis);
        let fired = render_inputs(&mut frame, &mut context, time, [], |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            if extra {
                assert!(!root.child().timer(period).fired());
            }
            let mut fired = [false; 4];
            for pair in fired.chunks_mut(2) {
                pair[0] = root.child().timer(period).fired();
                pair[1] = root.child().timer(period).fired();
            }
            fired
        });
        assert_eq!(fired, expected);
    }
}

#[test]
fn keyed_children_resolve_ahead_and_keep_state_across_parents() {
    let (mut frame, mut context) = frame(Size::uniform(10.0));
    let full = Some(Rect::new(0.0, 0.0, 10.0, 10.0));
    for (millis, column, expected) in [(0, 0, (None, None, false)), (500, 1, (full, full, true))] {
        let time = Duration::from_millis(millis);
        let result = render_inputs(&mut frame, &mut context, time, [], |mut ui: Ui<'_>| {
            let scope = ui.scope();
            let child = ui.at("child").area();
            let card = ui.at(scope.key("card")).area();
            let mut root = ui.layout(Overlay);
            root.child_id("child").insert(Fill('A', Size::uniform(10.0)));
            let mut fired = false;
            for index in 0..2 {
                let mut parent = root.child().layout(Overlay);
                if index == column {
                    let mut card = parent.child_in(scope, "card");
                    fired = card.timer(Duration::from_millis(500)).fired();
                    card.insert(Fill('B', Size::uniform(10.0)));
                }
            }
            (child, card, fired)
        });
        assert_eq!(result, expected);
    }
}
