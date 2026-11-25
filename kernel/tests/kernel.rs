use super::*;

#[test]
fn local_bounds_apply_to_content_and_layouts() {
    let (mut kernel, mut context) = kernel(Size::new(8.0, 6.0));
    for with_layout in [false, true] {
        let measured = render(&mut kernel, &mut context, |ui: Ui<'_>| {
            let mut root = ui.layout(());
            let child = root.child().constrain(Constraints::tight(Size::new(7.0, 2.0)));
            let node = child.id();
            if with_layout {
                child.layout(()).insert(Fill('L', Size::uniform(1.0)));
            } else {
                child.build(|mut ui: Ui<'_>| ui.insert(Fill('A', Size::uniform(1.0))));
            }
            root.layout_child(node, Constraints::loose(Size::new(5.0, 6.0)))
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
    let (mut kernel, mut context) = kernel(Size::uniform(4.0));
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
                render(&mut kernel, &mut context, |ui: Ui<'_>| {
                    ui.constrain(bounds);
                });
            }))
            .is_err()
        );
    }
}

#[test]
fn layout_and_paint() {
    let (mut kernel, mut context) = kernel(Size::new(8.0, 6.0));
    render(&mut kernel, &mut context, |ui: Ui<'_>| {
        let mut column = ui.layout(Column);
        {
            let mut transparent = column.child().layout(());
            transparent.insert(Fill('A', Size::uniform(1.0)));
            transparent.child().insert(Fill('A', Size::new(3.0, 1.0)));
            transparent.child().insert(Fill('A', Size::new(2.0, 1.0)));
        }
        column.child().item(1.0).build(|ui: Ui<'_>| {
            let mut panel = ui.layout(Overlay).clip(0.0);
            panel.child().insert(Fill('b', Size::new(5.0, 3.0)));
            panel.child().insert(Fill('C', Size::uniform(1.0)));
        });
        column.child();
        column.child().place(Id::ROOT, At(|_| Point::ZERO));
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
fn content_and_paint_order() {
    let (mut kernel, mut context) = kernel(Size::new(5.0, 4.0));
    render(&mut kernel, &mut context, |ui: Ui<'_>| {
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
    let (mut kernel, mut context) = kernel(Size::uniform(4.0));
    let id = 1100;
    for (extent, millis) in [(1.0, 0), (2.0, 0), (2.0, 500)] {
        render_inputs(
            &mut kernel,
            &mut context,
            Duration::from_millis(millis),
            [],
            |ui: Ui<'_>| {
                let mut root = ui.layout(SharedItems(3));
                root.child_in(Scope::ROOT, id)
                    .motion(Motion::new(Tween::new(1.0, Easing::LINEAR)).size())
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
            kernel
                .find(Scope::ROOT, id)
                .and_then(|id| kernel.area(id))
                .unwrap()
                .size(),
            Size::uniform(if millis == 500 { 1.5 } else { 1.0 })
        );
    }
    assert!(context.prepared >= 6);
}

#[test]
#[should_panic(expected = "too much scratch data")]
fn scratch_rejects_size_overflow() {
    let (mut kernel, mut context) = kernel(Size::uniform(4.0));
    render(&mut kernel, &mut context, |ui: Ui<'_>| {
        ui.scratch().slice::<u64>(usize::MAX);
    });
}

#[test]
fn owned_values_and_culling() {
    let result = Rc::new(Cell::new(ProbeResult::default()));
    let (mut kernel, mut context) = kernel(Size::new(3.0, 2.0));
    render(&mut kernel, &mut context, |ui: Ui<'_>| {
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

    // nodes paint past the screen since painters may draw outside their area
    // while an empty clip hides everything under it
    let results = [0; 2].map(|_| Rc::new(Cell::new(ProbeResult::default())));
    context.info = FrameInfo::new(Size::new(3.0, 1.0));
    render(&mut kernel, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        root.child()
            .place(Id::ROOT, At(|_| Point::new(4.0, 0.0)))
            .insert(Probe {
                result: results[0].clone(),
            });
        root.child().layout(Fixed(Size::ZERO)).clip(0.0).child().insert(Probe {
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
    let anchor = 1000;
    let panel_key = 1001;
    let (mut kernel, mut context) = kernel(Size::new(8.0, 5.0));
    render(&mut kernel, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut target = root.child_in(Scope::ROOT, anchor);
        let anchor_id = target.id();
        target.insert(Fill('T', Size::uniform(2.0)));
        drop(target);
        root.child()
            .place(anchor_id, At(|size| Point::new(size.width, size.height)))
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
        render(&mut kernel, &mut context, |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            let mut panel = root
                .child_in(Scope::ROOT, panel_key)
                .layout(Fixed(Size::uniform(3.0)))
                .clip(0.0);
            let panel_id = panel.id();
            panel.insert(Fill('p', Size::ZERO));
            panel
                .child()
                .parent(if escape { Id::ROOT } else { panel_id })
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
    let modal = 1002;
    let popup_key = 1003;
    let (mut kernel, mut context) = kernel(Size::new(7.0, 5.0));
    for open in [false, true, false] {
        let build = |mut ui: Ui<'_>| {
            let popup = ui.resolve(Scope::ROOT, popup_key);
            let modal_id = ui.resolve(Scope::ROOT, modal);
            let responses = [popup, modal_id].map(|id| ui.node(id).interact(WATCH)[0].is_some());
            let mut root = ui.layout(Overlay);
            if open {
                root.child_in(Scope::ROOT, modal)
                    .place(Id::ROOT, Full(|_| Point::ZERO))
                    .z_index(1)
                    .insert(Fill('M', Size::ZERO));
            }
            root.child().clip(0.0).build(|ui: Ui<'_>| {
                let outer = ui.id();
                ui.place(Id::ROOT, At(|_| Point::new(1.0, 0.0)))
                    .layout(Fixed(Size::uniform(5.0)))
                    .child()
                    .clip(0.0)
                    .build(|ui: Ui<'_>| {
                        let inner = ui.id();
                        ui.place(outer, At(|_| Point::new(2.0, 2.0)))
                            .layout(Fixed(Size::uniform(1.0)))
                            .child_in(Scope::ROOT, popup_key)
                            .place(inner, Full(|_| Point::new(-2.0, -2.0)))
                            .parent(outer)
                            .z_index(i16::MAX)
                            .insert(Fill('P', Size::ZERO));
                    });
            });
            responses
        };
        render(&mut kernel, &mut context, &build);
        assert_eq!(
            kernel.find(Scope::ROOT, popup_key).and_then(|id| kernel.area(id)),
            Some(Rect::new(1.0, 0.0, 5.0, 5.0))
        );
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
                &mut kernel,
                &mut context,
                Duration::ZERO,
                [pointer(None, Down, [x, 2.5])],
                &build,
            );
            assert_eq!(down, expected);
            let up = render_inputs(
                &mut kernel,
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
fn unbuilt_targets_keep_their_own_parent() {
    const TARGET: u32 = 1004;
    let cases: &[fn(Ui<'_>)] = &[
        |ui| {
            let id = ui.id();
            ui.parent(id).insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            let child = root.child().build(|ui: Ui<'_>| ui.id());
            root.parent(child).insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            let target = root.resolve(Scope::ROOT, TARGET);
            root.child().parent(target).insert(());
            root.child_in(Scope::ROOT, TARGET).insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            let target = root.resolve(Scope::ROOT, TARGET);
            root.child().place(target, At(|_| Point::ZERO)).insert(());
            root.child_in(Scope::ROOT, TARGET).insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            let target = root.resolve(Scope::ROOT, TARGET);
            root.child_in(Scope::ROOT, TARGET).parent(target).insert(());
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            let target = root.resolve(Scope::ROOT, TARGET);
            root.child_in(Scope::ROOT, TARGET).insert(());
            root.parent(target).insert(());
        },
    ];
    for build in cases {
        let (mut kernel, mut context) = kernel(Size::uniform(1.0));
        // a previous build must not satisfy a current reference
        render(&mut kernel, &mut context, |ui: Ui<'_>| {
            ui.layout(Overlay).child_in(Scope::ROOT, TARGET).insert(())
        });
        render(&mut kernel, &mut context, build);
    }
}

#[test]
fn duplicate_keys_are_reported() {
    const TARGET: u32 = 1004;
    let cases: &[fn(Ui<'_>) -> u32] = &[
        |ui| {
            let mut root = ui.layout(Overlay);
            root.child_in(Scope::ROOT, TARGET).insert(());
            root.child_in(Scope::ROOT, TARGET).insert(());
            line!() - 1
        },
        |ui| {
            let mut root = ui.layout(Overlay);
            root.child_in(Scope::ROOT, TARGET).insert(());
            root.child().layout(Overlay).child_in(Scope::ROOT, TARGET).insert(());
            line!() - 1
        },
    ];
    for build in cases {
        let (mut kernel, mut context) = kernel(Size::uniform(1.0));
        // the second holder is reported as it is built
        let line = render(&mut kernel, &mut context, build);
        assert_eq!(context.duplicates, [line]);
    }
}

#[test]
fn bindings_and_geometry() {
    let (mut kernel, mut context) = kernel(Size::uniform(10.0));
    let (a, b) = (1101, 1102);
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
        render(&mut kernel, &mut context, |mut ui: Ui<'_>| {
            let a_id = ui.resolve(Scope::ROOT, a);
            let b_id = ui.resolve(Scope::ROOT, b);
            if query {
                assert_eq!(ui.node(a_id).area().is_some(), laid_out);
                assert_eq!(ui.node(b_id).area().is_some(), laid_out);
            }
            ui.node(b_id).interact(WATCH);
            let mut root = ui.layout(Overlay);
            for _ in 0..count {
                root.child().insert(());
            }
            if count != 0 {
                root.child_in(Scope::ROOT, b).insert(());
                root.child_in(Scope::ROOT, a).parent(b_id).insert(());
                root.child().place(a_id, At(|_| Point::ZERO)).insert(());
            }
        });
        assert_eq!(
            kernel.find(Scope::ROOT, a).and_then(|id| kernel.area(id)).is_some(),
            count != 0
        );
        assert_eq!(
            kernel.find(Scope::ROOT, b).and_then(|id| kernel.area(id)).is_some(),
            count != 0
        );
        laid_out = count != 0;
    }
}

#[test]
fn padding_insets_children_while_content_keeps_the_full_box() {
    let (mut kernel, mut context) = kernel(Size::new(7.0, 5.0));
    let node = 1103;
    let padded_id = render(&mut kernel, &mut context, |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut padded = root.child_in(Scope::ROOT, node).layout(()).pad(2.0);
        let id = padded.id();
        padded.insert(Fill('B', Size::ZERO));
        padded.child().insert(Fill('C', Size::new(3.0, 1.0)));
        id
    });
    assert_eq!(kernel.area(padded_id), Some(Rect::new(0.0, 0.0, 7.0, 5.0)));
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
    let (mut kernel, mut context) = kernel(Size::uniform(1.0));
    let period = Duration::from_millis(500);
    // a child built first only later must not take the state of the children after it
    for (millis, extra, expected) in [(0, false, [false; 4]), (500, true, [true; 4])] {
        let time = Duration::from_millis(millis);
        let fired = render_inputs(&mut kernel, &mut context, time, [], |ui: Ui<'_>| {
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
    let child_key = 1005;
    let card_key = 1006;
    let (mut kernel, mut context) = kernel(Size::uniform(10.0));
    let full = Some(Rect::new(0.0, 0.0, 10.0, 10.0));
    for (millis, column, expected) in [(0, 0, (None, None, false)), (500, 1, (full, full, true))] {
        let time = Duration::from_millis(millis);
        let result = render_inputs(&mut kernel, &mut context, time, [], |mut ui: Ui<'_>| {
            let scope = ui.scope();
            let child_id = ui.resolve(Scope::ROOT, child_key);
            let card_id = ui.resolve(scope, card_key);
            let child = ui.node(child_id).area();
            let card = ui.node(card_id).area();
            let mut root = ui.layout(Overlay);
            root.child_in(Scope::ROOT, child_key)
                .insert(Fill('A', Size::uniform(10.0)));
            let mut fired = false;
            for index in 0..2 {
                let mut parent = root.child().layout(Overlay);
                if index == column {
                    let mut card = parent.child_in(scope, card_key);
                    fired = card.timer(Duration::from_millis(500)).fired();
                    card.insert(Fill('B', Size::uniform(10.0)));
                }
            }
            (child, card, fired)
        });
        assert_eq!(result, expected);
    }
}

#[test]
fn ids_follow_their_node() {
    let (mut kernel, mut context) = kernel(Size::uniform(1.0));
    let mut build = |extra: bool, keys: [u32; 2]| {
        render(&mut kernel, &mut context, |ui: Ui<'_>| {
            let mut root = ui.layout(Overlay);
            if extra {
                root.child().insert(());
            }
            let site = root.child().id();
            (site, keys.map(|key| root.child_in(Scope::ROOT, key).id()))
        })
    };
    let (site, [a, b]) = build(false, [1, 2]);
    // a sibling from another call site and reordered keys leave every id in place
    assert_eq!(build(true, [2, 1]), (site, [b, a]));
    // a node that leaves frees its id, which never reaches the node reusing its slot
    build(false, [3, 4]);
    let (_, [c, d]) = build(false, [5, 6]);
    assert!(kernel.area(a).is_none() && kernel.area(b).is_none());
    assert!(![c, d].contains(&a) && ![c, d].contains(&b));
}
