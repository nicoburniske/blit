use super::*;

#[test]
fn presses_and_movement() {
    for id in [None, Some(1)] {
        let mut scene = Scene::new(Sense::CLICK | Sense::DRAG);
        let down = scene.step(10, pointer(id, Down, [10.0, 10.0]));
        assert!(down.active && down.activated && !down.clicked);
        let up = scene.step(20, pointer(id, Up, [10.0, 10.0]));
        assert!(up.clicked && up.deactivated && !up.active);
        for ending in [Up, Cancel] {
            scene.step(30, pointer(id, Down, [10.0, 10.0]));
            let moved = scene.step(40, pointer(id, Move, [10.0, 300.0]));
            assert_eq!(moved.drag.unwrap().phase, Started);
            assert_eq!(moved.drag.unwrap().delta, Point::new(0.0, 290.0));
            let idle = scene.step(41, Input::None);
            assert_eq!(idle.drag.unwrap().delta, Point::ZERO);
            assert!(!scene.frame.has_pending_redraw());
            let ended = scene.step(50, pointer(id, ending, [10.0, 300.0]));
            assert_eq!(ended.drag.unwrap().phase, if ending == Up { Ended } else { Cancelled });
            assert!(ended.deactivated && !ended.active && !ended.clicked);
            assert_eq!(scene.step(51, Input::None).drag, None);
        }
        scene.step(60, pointer(id, Down, [10.0, 10.0]));
        let ended = scene.step(70, pointer(id, Up, [60.0, 10.0]));
        assert_eq!(ended.drag.unwrap().phase, Ended);
        assert_eq!(ended.drag.unwrap().delta, Point::new(50.0, 0.0));
        assert!(!ended.clicked);
    }
}

#[test]
fn contact_capture() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let claim = Cell::new(false);
    let release = Cell::new(false);
    let build = |mut ui: Ui<'_>| {
        let contact = ui.contacts().next();
        if let Some(contact) = contact {
            if claim.get() {
                assert!(ui.capture(contact.id))
            }
            if release.get() {
                assert!(ui.release(contact.id))
            }
        }
        let parent = ui.contacts().next();
        let mut layout = ui.layout(Overlay);
        let mut child = layout.child().id(Id::new("button"));
        child.insert(Fill('B', Size::uniform(200.0)));
        let response = child.interact(Sense::CLICK);
        let child_contact = child.contacts().next();
        let recaptured = child_contact.is_some_and(|contact| child.capture(contact.id));
        (parent, response, child_contact, recaptured)
    };
    render(&mut frame, &mut context, &build);
    let down = render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [pointer(Some(1), Down, [10.0, 10.0])],
        &build,
    );
    assert!(down.1.active && down.3);
    claim.set(true);
    let moved = render_inputs(
        &mut frame,
        &mut context,
        Duration::from_millis(10),
        [pointer(Some(1), Move, [30.0, 10.0])],
        &build,
    );
    assert!(moved.0.unwrap().captured);
    assert_eq!(moved.2.unwrap().phase, Cancelled);
    assert!(!moved.1.active && !moved.1.clicked && !moved.3);
    claim.set(false);
    let idle = render(&mut frame, &mut context, &build);
    assert_eq!(idle.0.unwrap().delta, Point::ZERO);
    assert_eq!(idle.2, None);
    assert!(!frame.has_pending_redraw());
    release.set(true);
    let released = render(&mut frame, &mut context, &build);
    assert!(!released.0.unwrap().captured);
    release.set(false);
    let ended = render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [pointer(Some(1), Up, [30.0, 10.0])],
        &build,
    );
    assert!(!ended.1.clicked && !ended.1.active);
}

#[test]
fn independent_contacts() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let build = |mut ui: Ui<'_>| {
        let mut contacts = ui.contacts();
        let first = contacts.next();
        let second = contacts.next();
        (first, second)
    };
    render(&mut frame, &mut context, &build);
    let result = render_inputs(
        &mut frame,
        &mut context,
        Duration::from_millis(10),
        [
            pointer(Some(1), Down, [10.0, 10.0]),
            pointer(Some(2), Down, [20.0, 10.0]),
            pointer(Some(1), Move, [15.0, 10.0]),
        ],
        &build,
    );
    assert_eq!(result.0.unwrap().source, ContactSource::Touch(1));
    assert_eq!(result.0.unwrap().delta, Point::new(5.0, 0.0));
    assert_eq!(result.1.unwrap().source, ContactSource::Touch(2));
    assert_eq!(result.1.unwrap().phase, Changed);
    assert_eq!(result.1.unwrap().delta, Point::ZERO);
    let ended = render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [pointer(Some(1), Up, [17.0, 10.0])],
        &build,
    );
    assert_eq!(ended.0.unwrap().phase, Ended);
    assert_eq!(ended.0.unwrap().delta, Point::new(2.0, 0.0));
    let idle = render(&mut frame, &mut context, &build);
    assert_eq!(idle.0.unwrap().source, ContactSource::Touch(2));
    assert_eq!(idle.1, None);
}

#[test]
fn scroll() {
    // zero delta endings stay with the owner even outside its bounds
    for sense in [Sense::SCROLL_X, Sense::SCROLL_Y] {
        let mut scene = InputScene::<Integer>::new(sense);
        for (phase, expected) in [
            (ScrollPhase::Started, Started),
            (ScrollPhase::Moved, Changed),
            (ScrollPhase::Ended, Ended),
        ] {
            let delta = if phase == ScrollPhase::Ended {
                Point::ZERO
            } else if sense == Sense::SCROLL_X {
                Point::new(0.5, -0.25)
            } else {
                Point::new(0.25, -0.5)
            };
            let response = scene.step(
                10,
                Input::Scroll {
                    position: if phase == ScrollPhase::Ended {
                        Point::new(300, 300)
                    } else {
                        Point::new(10, 10)
                    },
                    delta_x: delta.x,
                    delta_y: delta.y,
                    continuous: true,
                    phase,
                    modifiers: Modifiers::NONE,
                },
            );
            let scroll = response.scroll.unwrap();
            assert_eq!((scroll.delta, scroll.phase, scroll.continuous), (delta, expected, true));
            assert_eq!(response.drag, None);
            assert_eq!(scene.step(11, Input::None).scroll, None);
        }
        // native and global cancellation both end capture
        for global in [false, true] {
            scene.step(
                20,
                Input::Scroll {
                    position: Point::new(10, 10),
                    delta_x: if sense == Sense::SCROLL_X { 0.5 } else { 0.0 },
                    delta_y: if sense == Sense::SCROLL_Y { 0.5 } else { 0.0 },
                    continuous: true,
                    phase: ScrollPhase::Started,
                    modifiers: Modifiers::NONE,
                },
            );
            let cancel = if global {
                Input::Cancel
            } else {
                Input::Scroll {
                    position: Point::new(300, 300),
                    delta_x: 0.0,
                    delta_y: 0.0,
                    continuous: true,
                    phase: ScrollPhase::Cancelled,
                    modifiers: Modifiers::NONE,
                }
            };
            assert_eq!(scene.step(21, cancel).scroll.unwrap().phase, Cancelled);
        }
    }
}

#[test]
fn clipping() {
    let (mut frame, mut context) = frame(Size::new(5.0, 1.0));
    let build = |ui: Ui<'_>| {
        ui.layout(Overlay)
            .child()
            .layout(Fixed(Size::new(3.0, 1.0)))
            .clip(TestClip)
            .child()
            .build(|mut ui: Ui<'_>| {
                let response = ui.interact(Sense::CLICK);
                ui.interact(Sense::SCROLL);
                ui.insert(Fill('C', Size::new(5.0, 1.0)));
                response
            })
    };
    render(&mut frame, &mut context, &build);
    for (phase, x, active) in [(Down, 4.5, false), (Up, 4.5, false), (Down, 2.5, true)] {
        let response = render_inputs(
            &mut frame,
            &mut context,
            Duration::ZERO,
            [pointer(None, phase, [x, 0.5])],
            &build,
        );
        assert_eq!(response.active, active);
        assert!(!response.clicked);
    }
    let response = render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [Input::Scroll {
            position: Point::new(2.5, 0.5),
            delta_x: 0.0,
            delta_y: 1.0,
            continuous: false,
            phase: ScrollPhase::Moved,
            modifiers: Modifiers::NONE,
        }],
        &build,
    );
    assert!(response.active && response.scroll.is_some());
}

#[test]
fn clip_shapes() {
    struct ShapeClip(ClipShape<f32>);
    impl Clip<TestContext> for ShapeClip {
        fn shape(&self, _: Rect<f32>) -> ClipShape<f32> {
            self.0
        }
        fn push(&self, _: &mut TestContext, _: Rect<f32>) {}
        fn pop(&self, _: &mut TestContext) {}
    }

    let (mut frame, mut context) = frame(Size::uniform(4.0));
    let rect = Rect::new(0.0, 0.0, 4.0, 4.0);
    for shape in [
        ClipShape::Unbounded,
        ClipShape::RoundedRect { rect, radius: 2.0 },
        ClipShape::Circle {
            center: Point::new(2.0, 2.0),
            radius: 2.0,
        },
    ] {
        let build = |ui: Ui<'_>| {
            let mut root = ui.clip(ShapeClip(shape)).layout(Rects);
            let mut panel = root.child().item(Rect::new(0.0, 0.0, 1.0, 1.0)).clip(()).layout(Rects);
            let mut child = panel.child().item(Rect::new(3.0, 3.0, 1.0, 1.0)).clip(TestClip);
            child.insert(Fill('C', Size::uniform(1.0)));
            child.interact(Sense::CLICK)
        };
        render(&mut frame, &mut context, &build);
        assert!(context.commands.contains(&Paint('C', Rect::new(3.0, 3.0, 1.0, 1.0))));
        for (position, inside) in [([3.1, 3.1], true), ([3.9, 3.9], shape == ClipShape::Unbounded)] {
            let response = render_inputs(
                &mut frame,
                &mut context,
                Duration::ZERO,
                [pointer(None, Down, position), pointer(None, Up, position)],
                &build,
            );
            assert_eq!(response.clicked, inside);
        }
    }
}
