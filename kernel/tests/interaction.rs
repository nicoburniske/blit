use super::*;

#[test]
fn presses() {
    for touch in [false, true] {
        let mut scene = Scene::new(Sense::CLICK | Sense::DOUBLE_TAP | Sense::LONG_PRESS);
        let id = touch.then_some(1);
        for (time, doubled) in [(10, false), (100, true), (450, false)] {
            let down = scene.step(time, pointer(id, Down, [10.0, 10.0]));
            assert!(down.active && down.activated && !down.clicked);
            let up = scene.step(time + 10, pointer(id, Up, [10.0, 10.0]));
            assert!(up.clicked && up.deactivated && !up.active);
            assert_eq!(up.double_tapped, doubled);
        }
        scene.step(500, pointer(id, Down, [10.0, 10.0]));
        assert_eq!(scene.frame.next_timer_deadline(), Some(Duration::from_millis(1000)));
        assert!(!scene.step(999, Input::None).long_pressed);
        assert!(!scene.frame.has_pending_redraw());
        assert!(scene.step(1000, Input::None).long_pressed);
        assert_eq!(scene.frame.next_timer_deadline(), None);
        assert!(!scene.step(1001, Input::None).long_pressed);
        assert!(!scene.step(1100, pointer(id, Up, [10.0, 10.0])).clicked);
        scene.step(1200, pointer(id, Down, [10.0, 10.0]));
        scene.step(1210, pointer(id, Move, [30.0, 10.0]));
        assert_eq!(scene.frame.next_timer_deadline(), None);
        assert!(!scene.step(1800, Input::None).long_pressed);
        scene.step(1900, pointer(id, Down, [10.0, 10.0]));
        let cancelled = scene.step(1910, pointer(id, Cancel, [10.0, 10.0]));
        assert!(!cancelled.clicked && !cancelled.active);
    }
    // overlapping fingers are not a double tap even in one frame
    for interval in [0, 10] {
        let mut scene = Scene::new(Sense::DOUBLE_TAP);
        scene.step(interval, pointer(Some(1), Down, [10.0, 10.0]));
        scene.step(interval * 2, pointer(Some(2), Down, [11.0, 10.0]));
        scene.step(interval * 3, pointer(Some(1), Up, [10.0, 10.0]));
        let released = scene.step(interval * 4, pointer(Some(2), Up, [11.0, 10.0]));
        assert!(!released.double_tapped);
    }
}

#[test]
fn movement() {
    // drag and touch scroll share the same lifecycle
    for (id, sense) in [
        (None, Sense::CLICK | Sense::DRAG | Sense::SWIPE),
        (Some(1), Sense::CLICK | Sense::DRAG | Sense::SWIPE),
        (Some(1), Sense::SCROLL_Y),
    ] {
        for ending in [Up, Cancel] {
            let mut scene = Scene::new(sense);
            scene.step(10, pointer(id, Down, [10.0, 10.0]));
            let moved = scene.step(60, pointer(id, Move, [10.0, 300.0]));
            assert!(moved.active && !moved.clicked);
            if sense == Sense::SCROLL_Y {
                let scroll = moved.scroll.unwrap();
                assert_eq!(scroll.phase, Started);
                assert_eq!(scroll.delta, Point::new(0.0, -290.0));
                assert_eq!(moved.drag, None);
            } else {
                let drag = moved.drag.unwrap();
                assert_eq!(drag.phase, Started);
                assert_eq!(drag.delta, Point::new(0.0, 290.0));
            }
            let idle = scene.step(61, Input::None);
            if let Some(drag) = idle.drag {
                assert_eq!((drag.delta, drag.velocity), (Point::ZERO, Point::ZERO));
            } else {
                assert_eq!(idle.scroll.unwrap().delta, Point::ZERO);
            }
            assert!(!scene.frame.has_pending_redraw());
            let ended = scene.step(100, pointer(id, ending, [10.0, 300.0]));
            assert_eq!(
                ended
                    .drag
                    .map(|drag| drag.phase)
                    .or(ended.scroll.map(|scroll| scroll.phase)),
                Some(if ending == Up { Ended } else { Cancelled })
            );
            assert_eq!(
                ended.swipe,
                (ending == Up && sense.intersects(Sense::SWIPE)).then_some(Direction::Down)
            );
            assert!(ended.deactivated && !ended.active && !ended.clicked);
            let idle = scene.step(101, pointer(id, Up, [10.0, 300.0]));
            assert_eq!((idle.drag, idle.scroll, idle.swipe), (None, None, None));
        }
    }
    // release can contain the first movement including in one frame
    for time in [10, 100] {
        let mut scene = Scene::new(Sense::CLICK | Sense::DRAG | Sense::SWIPE);
        scene.step(10, pointer(Some(1), Down, [10.0, 10.0]));
        let released = scene.step(time, pointer(Some(1), Up, [60.0, 10.0]));
        let drag = released.drag.unwrap();
        assert_eq!(drag.delta, Point::new(50.0, 0.0));
        assert_eq!(drag.phase, Ended);
        assert_eq!(released.swipe, Some(Direction::Right));
        assert!(!released.clicked);
    }
}

#[test]
fn contacts() {
    // one finger leaving must not end another finger's drag
    let mut scene = Scene::new(Sense::CLICK | Sense::DRAG);
    for (time, id, phase, x, state, drag_phase) in [
        (10, 1, Down, 10.0, (true, true, false), None),
        (20, 1, Move, 20.0, (true, false, false), Some(Started)),
        (30, 2, Down, 10.0, (true, false, false), Some(Changed)),
        (40, 2, Move, 20.0, (true, false, false), Some(Started)),
        (50, 1, Up, 20.0, (true, false, false), Some(Changed)),
        (60, 2, Cancel, 20.0, (false, false, true), Some(Cancelled)),
    ] {
        let response = scene.step(time, pointer(Some(id), phase, [x, 10.0]));
        assert_eq!(response.drag.map(|drag| drag.phase), drag_phase);
        assert_eq!((response.active, response.activated, response.deactivated), state);
    }

    // scroll hands off to pinch and resumes with the remaining finger
    let mut scene = Scene::new(Sense::SCROLL_Y | Sense::PINCH);
    scene.step(10, pointer(Some(1), Down, [50.0, 50.0]));
    let moved = scene.step(20, pointer(Some(1), Move, [50.0, 70.0]));
    assert_eq!(moved.scroll.unwrap().phase, Started);
    let joined = scene.step(30, pointer(Some(2), Down, [80.0, 70.0]));
    assert_eq!(joined.scroll.unwrap().phase, Cancelled);
    assert_eq!(joined.pinch.unwrap().phase, Started);
    let resumed = scene.step(40, pointer(Some(2), Up, [80.0, 70.0]));
    assert_eq!(resumed.pinch.unwrap().phase, Ended);
    assert_eq!(resumed.scroll.unwrap().phase, Started);
    assert!(resumed.active);
    let moved = scene.step(50, pointer(Some(1), Move, [50.0, 80.0]));
    assert_eq!(moved.scroll.unwrap().delta, Point::new(0.0, -10.0));

    // pinch and rotation rebase when contacts change
    for cancel in [false, true] {
        let mut scene = Scene::new(Sense::CLICK | Sense::DRAG | Sense::PINCH | Sense::ROTATE);
        scene.step(10, pointer(Some(1), Down, [50.0, 50.0]));
        let moved = scene.step(20, pointer(Some(1), Move, [60.0, 50.0]));
        assert_eq!(moved.drag.unwrap().phase, Started);
        let joined = scene.step(30, pointer(Some(2), Down, [80.0, 50.0]));
        assert_eq!(joined.pinch.unwrap().phase, Started);
        assert_eq!(joined.rotation.unwrap().phase, Started);
        assert_eq!(joined.drag.unwrap().phase, Cancelled);
        assert!(!joined.activated && !joined.deactivated);
        let moved = scene.step(130, pointer(Some(2), Move, [80.0, 70.0]));
        assert_eq!(moved.pinch.unwrap().phase, Changed);
        assert_eq!(moved.pinch.unwrap().center, Point::new(70.0, 60.0));
        assert_eq!(moved.rotation.unwrap().center, moved.pinch.unwrap().center);
        assert!((moved.pinch.unwrap().scale - 2.0_f32.sqrt()).abs() < 0.0001);
        assert!((moved.rotation.unwrap().radians - FRAC_PI_4).abs() < 0.0001);
        let rebased = scene.step(140, pointer(Some(3), Down, [100.0, 90.0]));
        assert_eq!(
            (rebased.pinch.unwrap().scale, rebased.rotation.unwrap().radians),
            (1.0, 0.0)
        );
        if cancel {
            let cancelled = scene.step(150, pointer(Some(2), Cancel, [80.0, 70.0]));
            assert_eq!(cancelled.pinch.unwrap().phase, Cancelled);
            assert_eq!(cancelled.rotation.unwrap().phase, Cancelled);
            assert!(cancelled.deactivated && !cancelled.active);
            for id in [1, 3] {
                let up = scene.step(160, pointer(Some(id), Up, [60.0, 50.0]));
                assert!(!up.clicked);
                assert_eq!((up.drag, up.pinch, up.rotation), (None, None, None));
            }
        } else {
            scene.step(150, pointer(Some(3), Up, [100.0, 90.0]));
            let resumed = scene.step(160, pointer(Some(2), Up, [80.0, 70.0]));
            assert!(resumed.active && !resumed.deactivated);
            assert_eq!(resumed.pinch.unwrap().phase, Ended);
            assert_eq!(resumed.rotation.unwrap().phase, Ended);
            let moved = scene.step(170, pointer(Some(1), Move, [70.0, 50.0]));
            assert_eq!(moved.drag.unwrap().delta, Point::new(10.0, 0.0));
            let ended = scene.step(180, pointer(Some(1), Up, [70.0, 50.0]));
            assert_eq!(ended.drag.unwrap().phase, Ended);
            assert!(ended.deactivated && !ended.clicked);
        }
    }
}

#[test]
fn routing() {
    let ids = ["back", "scroll", "button"].map(Id::new);
    let scene = |ui: Ui<'_>| {
        let mut back = ui.id(ids[0]);
        let back_response = back.interact(Sense::SWIPE_X);
        let mut back = back.layout(Rects);
        let mut scroll = back.child().item(Rect::new(0.0, 0.0, 200.0, 200.0)).id(ids[1]);
        let scroll_response = scroll.interact(Sense::SCROLL_Y | Sense::DRAG_Y);
        let mut scroll = scroll.layout(Rects);
        let mut button = scroll.child().item(Rect::new(0.0, 0.0, 200.0, 200.0)).id(ids[2]);
        [back_response, scroll_response, button.interact(Sense::CLICK)]
    };
    // movement chooses the interested ancestor
    for (id, horizontal) in [(None, false), (None, true), (Some(1), false), (Some(1), true)] {
        let (mut frame, mut context) = frame(Size::uniform(200.0));
        for (time, phase) in [(0, None), (10, Some(Down)), (20, Some(Move)), (30, Some(Up))] {
            let position = match (time, horizontal) {
                (10, _) => Point::new(10.0, 10.0),
                (_, true) => Point::new(60.0, 10.0),
                (_, false) => Point::new(10.0, 60.0),
            };
            let responses = render_inputs(
                &mut frame,
                &mut context,
                Duration::from_millis(time),
                [phase.map_or(Input::None, |phase| pointer(id, phase, [position.x, position.y]))],
                scene,
            );
            if time == 10 {
                assert!(responses[2].active && !responses[1].active);
            }
            if time == 20 {
                assert_eq!(responses[0].active, horizontal);
                assert_eq!(responses[1].scroll.is_some(), id.is_some() && !horizontal);
                assert_eq!(responses[1].drag.is_some(), !horizontal);
                assert!(responses[2].deactivated);
            }
            if time == 30 {
                assert_eq!(responses[0].swipe, horizontal.then_some(Direction::Right));
                assert!(!responses[2].clicked);
            }
        }
    }

    // removed widgets cannot regain capture
    let mut scene = Scene::new(Sense::CLICK | Sense::DRAG | Sense::LONG_PRESS);
    scene.step(10, pointer(Some(1), Down, [10.0, 10.0]));
    scene.frame.build(
        &mut scene.context,
        FrameInfo::new(Size::uniform(200.0)),
        Duration::from_millis(20),
        Input::None,
        |_: blit::Ui<'_, ()>| (),
    );
    scene.frame.layout(&mut scene.context);
    assert_eq!(scene.frame.next_timer_deadline(), None);
    for (time, phase) in [(30, Move), (40, Up)] {
        let response = scene.step(time, pointer(Some(1), phase, [60.0, 10.0]));
        assert!(!response.active && !response.clicked);
        assert_eq!((response.drag, response.scroll, response.swipe), (None, None, None));
    }
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
            assert_eq!((response.drag, response.pinch, response.rotation), (None, None, None));
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
