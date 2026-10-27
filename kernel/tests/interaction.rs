use super::*;

#[test]
fn presses() {
    for id in [None, Some(1)] {
        let mut scene = Scene::new(Press::new());
        let down = scene.step(10, pointer(id, Down, [10.0, 10.0]));
        assert!(down.active && down.activated && !down.clicked);
        let up = scene.step(20, pointer(id, Up, [12.0, 10.0]));
        assert!(up.clicked && up.deactivated && !up.active);
        // moving past the slop or cancelling is not a click
        for ending in [Up, Cancel] {
            scene.step(30, pointer(id, Down, [10.0, 10.0]));
            let moved = scene.step(40, pointer(id, Move, [10.0, 300.0]));
            assert!(moved.deactivated && !moved.active);
            assert!(!scene.frame.has_pending_redraw());
            assert!(!scene.step(50, pointer(id, ending, [10.0, 10.0])).clicked);
        }
    }
}

#[test]
fn focus_follows_presses_and_taps() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let right_id = Id::new("right");
    let change_focus = Cell::new(false);
    let clear_focus = Cell::new(false);
    let build = |mut ui: Ui<'_>| {
        if clear_focus.replace(false) {
            ui.clear_focus();
        }
        let mut ui = ui.layout(Rects);
        let mut left = ui.child().item(Rect::new(0.0, 0.0, 100.0, 200.0)).id(Id::new("left"));
        let left_focused = left.interact(Focus);
        if change_focus.replace(false) {
            left.focus(right_id);
        }
        drop(left);
        let mut right = ui.child().item(Rect::new(100.0, 0.0, 100.0, 200.0)).id(right_id);
        (left_focused, right.interact(Focus))
    };
    render(&mut frame, &mut context, &build);
    let mut step = |input| render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
    assert_eq!(step(pointer(None, Down, [10.0, 10.0])), (true, false));
    step(pointer(None, Up, [10.0, 10.0]));
    // focus set during a build shows from the next one
    change_focus.set(true);
    assert_eq!(step(Input::None), (true, false));
    assert_eq!(step(Input::None), (false, true));
    // a touch takes focus on release unless something captured it
    clear_focus.set(true);
    step(pointer(Some(1), Down, [10.0, 10.0]));
    step(pointer(Some(1), Move, [30.0, 10.0]));
    assert_eq!(step(pointer(Some(1), Up, [30.0, 10.0])), (true, false));
    step(pointer(Some(1), Down, [110.0, 10.0]));
    assert_eq!(step(pointer(Some(1), Up, [110.0, 10.0])), (false, true));
}

#[test]
fn tapping_unregistered_background_clears_focus() {
    for id in [None, Some(1)] {
        let (mut frame, mut context) = frame(Size::uniform(200.0));
        let focus = Cell::new(true);
        let field_id = Id::new("field");
        let build = |mut ui: Ui<'_>| {
            if focus.replace(false) {
                ui.focus(field_id);
            }
            let mut root = ui.layout(Rects);
            let mut field = root.child().item(Rect::new(0.0, 0.0, 100.0, 200.0)).id(field_id);
            field.interact(Focus);
            field.is_focused(field_id)
        };
        assert!(render(&mut frame, &mut context, &build));
        let mut step = |input| render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
        // mouse clears focus on press and touch on release
        assert_eq!(step(pointer(id, Down, [150.0, 10.0])), id.is_some());
        assert!(!step(pointer(id, Up, [150.0, 10.0])));
    }
}

#[test]
fn scroll() {
    // zero delta endings stay with the owner even outside its bounds
    for horizontal in [true, false] {
        let mut scene = InputScene::<Integer, _>::new(Scroll::new());
        for (phase, expected) in [
            (ScrollPhase::Started, Started),
            (ScrollPhase::Moved, Changed),
            (ScrollPhase::Ended, Ended),
        ] {
            let delta = if phase == ScrollPhase::Ended {
                Point::ZERO
            } else if horizontal {
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
            let scroll = response.unwrap();
            assert_eq!((scroll.delta, scroll.phase, scroll.continuous), (delta, expected, true));
            assert_eq!(scene.step(11, Input::None), None);
        }
        // native and global cancellation both end capture
        for global in [false, true] {
            scene.step(
                20,
                Input::Scroll {
                    position: Point::new(10, 10),
                    delta_x: if horizontal { 0.5 } else { 0.0 },
                    delta_y: if horizontal { 0.0 } else { 0.5 },
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
            assert_eq!(scene.step(21, cancel).unwrap().phase, Cancelled);
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
                let response = ui.interact(Press::new());
                let wheel = ui.interact(Scroll::new());
                ui.insert(Fill('C', Size::new(5.0, 1.0)));
                (response, wheel)
            })
    };
    render(&mut frame, &mut context, &build);
    for (phase, x, active) in [(Down, 4.5, false), (Up, 4.5, false), (Down, 2.5, true)] {
        let (response, _) = render_inputs(
            &mut frame,
            &mut context,
            Duration::ZERO,
            [pointer(None, phase, [x, 0.5])],
            &build,
        );
        assert_eq!(response.active, active);
        assert!(!response.clicked);
    }
    let (response, wheel) = render_inputs(
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
    assert!(response.active && wheel.is_some());
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
            child.interact(Press::new())
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

#[test]
fn hover_tracks_changed_ancestry() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let parent = Cell::new(0);
    let build = |ui: Ui<'_>| {
        let mut root = ui.layout(Rects);
        let mut parent = root
            .child()
            .item(Rect::new(0.0, 0.0, 200.0, 200.0))
            .id(Id::new(parent.get()));
        let hovered = parent.interact(Press::new()).hovered;
        parent
            .layout(Rects)
            .child()
            .item(Rect::new(0.0, 0.0, 200.0, 200.0))
            .id(Id::new("leaf"))
            .interact(Press::new());
        hovered
    };
    render(&mut frame, &mut context, &build);
    assert!(render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [pointer(None, Move, [10.0, 10.0])],
        &build
    ));
    parent.set(1);
    assert!(!render(&mut frame, &mut context, &build));
    assert!(frame.has_pending_redraw());
    assert!(render(&mut frame, &mut context, &build));
}

#[test]
fn wheel_axis_leaves_other_directions_to_ancestors() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let build = |mut ui: Ui<'_>| {
        let list = ui.interact(Scroll::new().axis(Axis::Vertical));
        let mut root = ui.layout(Overlay);
        let mut carousel = root.child().id(Id::new("carousel"));
        carousel.insert(Fill('C', Size::uniform(200.0)));
        (list, carousel.interact(Scroll::new().axis(Axis::Horizontal)))
    };
    render(&mut frame, &mut context, &build);
    let mut wheel = |delta_x, delta_y| {
        let input = Input::Scroll {
            position: Point::new(10.0, 10.0),
            delta_x,
            delta_y,
            continuous: false,
            phase: ScrollPhase::Moved,
            modifiers: Modifiers::NONE,
        };
        let (list, carousel) = render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
        (list.is_some(), carousel.is_some())
    };
    assert_eq!(wheel(5.0, 1.0), (false, true));
    assert_eq!(wheel(1.0, 5.0), (true, false));
}
