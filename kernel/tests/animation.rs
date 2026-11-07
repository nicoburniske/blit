use blit::internal::animator::Animator;

use super::*;

#[test]
fn motions() {
    for named in [false, true] {
        let (mut frame, mut context) = frame(Size::new(1.0, 5.0));
        for (extent, gap, millis, area, sibling, pending) in [
            (1.0, 0.0, 0, Rect::new(0.0, 0.0, 1.0, 1.0), Some(1.0), false),
            (3.0, 2.0, 0, Rect::new(0.0, 0.0, 1.0, 1.0), Some(3.0), true),
            (3.0, 2.0, 500, Rect::new(0.0, 1.0, 1.0, 2.0), Some(4.0), true),
            (3.0, 2.0, 1000, Rect::new(0.0, 2.0, 1.0, 3.0), None, false),
        ] {
            let build = |ui: Ui<'_>| {
                let mut column = ui.layout(Column);
                let mut child = column.child().item(gap).motion(
                    Motion::new(Tween::new(Duration::from_secs(1), Easing::LINEAR))
                        .height()
                        .y(),
                );
                if named {
                    child = child.id(Id::new("motion"));
                }
                let (_, measured) = child.build_measured(
                    Constraints {
                        min: Size::new(1.0, 0.0),
                        max: Size::new(1.0, f32::INFINITY),
                    },
                    |ui: Ui<'_>| {
                        ui.layout(()).child().insert(Fill('X', Size::new(1.0, extent)));
                    },
                );
                assert_eq!(measured, area.size());
                column.child().insert(Fill('Y', Size::uniform(1.0)));
            };
            let time = Duration::from_millis(millis);
            render_inputs(&mut frame, &mut context, time, [], build);
            if let Some(y) = sibling {
                assert_eq!(
                    context.commands,
                    [Paint('X', area), Paint('Y', Rect::new(0.0, y, 1.0, 1.0))]
                );
            } else {
                assert_eq!(context.commands, [Paint('X', area)]);
            }
            // the frame after settles without changing the motion
            render_inputs(&mut frame, &mut context, time, [], build);
            assert_eq!(frame.has_pending_redraw(), pending);
        }
    }
}

#[test]
fn animation_and_timers() {
    let (mut frame, mut context) = frame(Size::uniform(1.0));
    let animation = Id::new("animation");
    let timer = Id::new("timer");
    for (target, millis, expected, fired) in [(0.0, 0, 0.0, false), (1.0, 0, 0.0, false), (1.0, 500, 0.5, true)] {
        let result = render_inputs(
            &mut frame,
            &mut context,
            Duration::from_millis(millis),
            [],
            |mut ui: Ui<'_>| {
                (
                    ui.animate(
                        animation,
                        target,
                        Tween::new(Duration::from_secs(1), Easing::LINEAR),
                        Default::default(),
                    ),
                    ui.timer(timer, Duration::from_millis(500)),
                )
            },
        );
        assert_eq!(result, (expected, fired));
    }
    assert!(frame.has_pending_redraw());
}

#[test]
fn looping_switches_to_target_motion() {
    let (mut frame, mut context) = frame(Size::uniform(1.0));
    let id = Id::new("looping animation");
    let tween = Tween::new(Duration::from_secs(1), Easing::LINEAR);
    for (millis, duration, expected, active) in [
        (0, Some(1000), 0.0, true),
        (250, Some(1000), 0.25, true),
        (1000, Some(1000), 0.0, true),
        (1250, Some(2000), 0.0, true),
        (1750, Some(2000), 0.25, true),
        (1750, None, 0.25, true),
        (2250, None, 0.625, true),
        (2250, Some(0), 0.0, false),
        (2250, None, 0.0, true),
        (2750, None, 0.5, true),
        (3250, None, 1.0, false),
        (3250, Some(1000), 0.0, true),
        (3750, Some(1000), 0.5, true),
        (3750, Some(0), 0.0, false),
    ] {
        let now = Duration::from_millis(millis);
        for _ in 0..2 {
            let value = render_inputs(&mut frame, &mut context, now, [], |mut ui: Ui<'_>| match duration {
                Some(duration) => ui.animate_loop(
                    id,
                    Tween::new(Duration::from_millis(duration), Easing::Function(|t| (t, 1.0))),
                ),
                None => ui.animate(id, 1.0, tween, Default::default()),
            });
            assert_eq!(value, expected);
        }
        assert_eq!(frame.has_pending_redraw(), active);
    }
}

#[test]
fn tween_retargeting() {
    let options = AnimationOptions::default();
    for restart in [false, true] {
        let mut animator = Animator::new([0.0], Animation::default(), options);
        animator.advance(
            [100.0],
            [true],
            Tween::new(Duration::from_secs(1), Easing::EASE_IN_QUAD),
            options,
            Duration::ZERO,
            true,
        );
        let animation = Tween::new(Duration::from_secs(2), Easing::LINEAR);
        let options = AnimationOptions { restart, ..options };
        for (millis, expected) in [
            (500, [25.0, 25.0]),
            (750, [56.015625, 36.875]),
            (1000, [76.875, 48.75]),
            (2500, [120.0, 120.0]),
        ] {
            assert_eq!(
                animator
                    .advance([120.0], [true], animation, options, Duration::from_millis(millis), true)
                    .0,
                [expected[usize::from(restart)]],
            );
        }
        assert!(!animator.is_active());
    }

    let mut animator = Animator::new([0.0], Animation::default(), options);
    let linear = Tween::new(Duration::from_secs(1), Easing::LINEAR);
    animator.advance([100.0], [true], linear, options, Duration::ZERO, true);
    animator.advance([120.0], [true], linear, options, Duration::from_millis(500), true);
    let mut restarted = animator.clone();
    animator.advance([-20.0], [true], linear, options, Duration::from_millis(750), true);
    let value = animator
        .advance([-20.0], [true], linear, options, Duration::from_millis(1000), true)
        .0[0];
    assert!((value - 68.65234).abs() < 0.0001);
    let restart = AnimationOptions {
        restart: true,
        ..options
    };
    restarted.advance([-20.0], [true], linear, restart, Duration::from_millis(750), true);
    assert_eq!(
        restarted
            .advance([-20.0], [true], linear, restart, Duration::from_millis(1000), true)
            .0,
        [56.171875]
    );
    assert_eq!(
        animator
            .advance(
                [-20.0],
                [true],
                Animation::default(),
                options,
                Duration::from_millis(1100),
                true
            )
            .0,
        [-20.0]
    );
    assert!(!animator.is_active());
}

#[test]
fn tween_retargeting_carries_velocity() {
    let options = AnimationOptions::default();
    let mut animator = Animator::new([0.0], Animation::default(), options);
    let animation = Tween::new(Duration::from_secs(1), Easing::EASE_IN_QUAD);
    let mut previous = 0.0;
    for (millis, target) in [(0, 100.0), (200, 200.0), (300, -100.0), (400, 300.0), (500, -200.0)] {
        let now = Duration::from_millis(millis);
        let next = now + Duration::from_micros(100);
        let mut unchanged = animator.clone();
        let position = unchanged.advance([previous], [true], animation, options, now, true).0;
        let continuing = unchanged.advance([previous], [true], animation, options, next, true).0[0];
        assert_eq!(
            animator.advance([target], [true], animation, options, now, false).0,
            position
        );
        assert_eq!(
            animator.advance([target], [true], animation, options, now, true).0,
            position
        );
        let value = animator.advance([target], [true], animation, options, next, true).0[0];
        assert!((value - continuing).abs() < 0.001);
        previous = target;
    }
    assert_eq!(
        animator.advance([previous], [true], animation, options, Duration::from_secs(2), true),
        ([previous], [false])
    );
}

#[test]
fn retargeted_motion_measurement() {
    let (mut frame, mut context) = frame(Size::uniform(100.0));
    let id = Id::new("retargeted height");
    for (extent, millis, expected) in [
        (1.0, 0, 1.0),
        (3.0, 0, 1.0),
        (5.0, 500, 2.0),
        (5.0, 750, 3.03125),
        (5.0, 1000, 3.75),
        (5.0, 1500, 5.0),
        (5.0, 1500, 5.0),
    ] {
        render_inputs(
            &mut frame,
            &mut context,
            Duration::from_millis(millis),
            [],
            |ui: Ui<'_>| {
                let mut column = ui.layout(Column);
                let (_, size) = column
                    .child()
                    .id(id)
                    .motion(Motion::new(Tween::new(Duration::from_secs(1), Easing::LINEAR)).height())
                    .build_measured(Constraints::loose(Size::uniform(100.0)), |mut ui: Ui<'_>| {
                        ui.insert(Fill('X', Size::new(1.0, extent)));
                    });
                assert_eq!(size.height, expected);
                column.child().insert(Fill('Y', Size::uniform(1.0)));
            },
        );
        assert_eq!(frame.area(id).unwrap().height, expected);
        assert_eq!(
            context.commands.last(),
            Some(&Paint('Y', Rect::new(0.0, expected, 1.0, 1.0)))
        );
    }
    assert!(!frame.has_pending_redraw());
}

#[test]
fn spring_retargeting_and_velocity() {
    let options = AnimationOptions::default();
    let spring = Spring::new(Duration::from_millis(500), 0.3);
    let mut animator = Animator::new([0.0], Animation::default(), options);
    animator.advance([1.0], [true], spring, options, Duration::ZERO, true);
    let time = Duration::from_millis(30);
    let position = animator.advance([1.0], [true], spring, options, time, true).0[0];
    let mut restarted = animator.clone();
    assert_eq!(
        animator.advance([-1.0], [true], spring, options, time, true).0,
        [position]
    );
    let restart = AnimationOptions {
        restart: true,
        ..options
    };
    assert_eq!(
        restarted.advance([-1.0], [true], spring, restart, time, true).0,
        [position]
    );
    assert!(
        animator
            .advance([-1.0], [true], spring, options, time + Duration::from_millis(1), true)
            .0[0]
            > position
    );
    assert!(
        restarted
            .advance([-1.0], [true], spring, restart, time + Duration::from_millis(1), true)
            .0[0]
            < position
    );
    assert_eq!(
        animator
            .advance([-1.0], [true], spring, options, Duration::from_secs(10), true)
            .0,
        [-1.0]
    );
    assert!(!animator.is_active());

    let mut launched = Animator::new([0.0], Animation::default(), options);
    let launch = AnimationOptions {
        velocity: Some(-10.0),
        ..options
    };
    launched.advance([1.0], [true], spring, launch, Duration::ZERO, true);
    assert!(
        launched
            .advance([1.0], [true], spring, launch, Duration::from_millis(1), true)
            .0[0]
            < 0.0
    );

    let mut at_target = Animator::new([0.0], Animation::default(), options);
    let launch = AnimationOptions {
        velocity: Some(5.0),
        ..options
    };
    at_target.advance([0.0], [true], spring, launch, Duration::ZERO, true);
    assert!(
        at_target
            .advance([0.0], [true], spring, launch, Duration::from_millis(10), true)
            .0[0]
            > 0.0
    );
    assert_eq!(
        at_target
            .advance([0.0], [true], spring, launch, Duration::from_secs(10), true)
            .0,
        [0.0]
    );
    at_target.advance([0.0], [true], spring, launch, Duration::from_secs(11), true);
    assert!(!at_target.is_active());

    let mut tween = Animator::new([0.0], Animation::default(), options);
    let linear = Tween::new(Duration::from_secs(1), Easing::Function(|t| (t, 1.0)));
    tween.advance([1.0], [true], linear, options, Duration::ZERO, true);
    assert_eq!(
        tween
            .advance([0.5], [true], spring, options, Duration::from_millis(500), true)
            .0,
        [0.5]
    );
    assert!(
        tween
            .advance([0.5], [true], spring, options, Duration::from_millis(501), true)
            .0[0]
            > 0.5
    );
}

#[test]
fn spring_to_tween_carries_velocity() {
    let options = AnimationOptions::default();
    let spring = Spring::new(Duration::from_millis(500), 0.3);
    for easing in [Easing::LINEAR, Easing::EASE_IN_QUAD, Easing::EASE_OUT_QUAD] {
        for restart in [false, true] {
            let mut animator = Animator::new([0.0], Animation::default(), options);
            animator.advance([1.0], [true], spring, options, Duration::ZERO, true);
            let now = Duration::from_millis(30);
            let next = now + Duration::from_micros(100);
            let position = animator.advance([1.0], [true], spring, options, now, false).0[0];
            let continuing = animator.advance([1.0], [true], spring, options, next, false).0[0];
            let tween = Tween::new(Duration::from_secs(1), easing);
            let options = AnimationOptions { restart, ..options };
            assert_eq!(
                animator.advance([-1.0], [true], tween, options, now, false).0,
                [position]
            );
            assert_eq!(
                animator.advance([-1.0], [true], tween, options, now, true).0,
                [position]
            );
            let value = animator.advance([-1.0], [true], tween, options, next, true).0[0];
            let speed = (value - position) / 0.0001;
            let expected =
                (-1.0 - position) * easing.sample(0.0).1 + if restart { 0.0 } else { (continuing - position) / 0.0001 };
            assert!(
                (speed - expected).abs() < 0.02,
                "{easing:?}, {restart}: {speed} != {expected}"
            );
            assert_eq!(
                animator.advance([-1.0], [true], tween, options, Duration::from_secs(2), true),
                ([-1.0], [false]),
            );
        }
    }
}

#[test]
fn explicit_velocity_overrides_carried_velocity() {
    for animation in [
        Animation::Spring(Spring::new(Duration::from_millis(500), 0.3)),
        Tween::new(Duration::from_secs(1), Easing::LINEAR).into(),
        Tween::new(Duration::from_secs(1), Easing::EASE_IN_QUAD).into(),
        Tween::new(Duration::from_secs(1), Easing::EASE_OUT_QUAD).into(),
    ] {
        for restart in [false, true] {
            for velocity in [-2.0, 0.0, 2.0] {
                let options = AnimationOptions {
                    restart,
                    velocity: Some(velocity),
                };
                let mut animator = Animator::new([0.0], Animation::default(), AnimationOptions::default());
                let mut now = Duration::ZERO;
                for target in [0.0, 1.0, -1.0] {
                    let position = animator.values;
                    let expected = match animation {
                        Animation::Spring(_) => velocity,
                        Animation::Tween(tween) => {
                            velocity
                                + (target - position[0]) * tween.easing.sample(0.0).1 / tween.duration.as_secs_f32()
                        }
                    };
                    assert_eq!(
                        animator.advance([target], [true], animation, options, now, false).0,
                        position
                    );
                    assert_eq!(
                        animator.advance([target], [true], animation, options, now, true).0,
                        position
                    );
                    now += Duration::from_micros(100);
                    let value = animator.advance([target], [true], animation, options, now, true).0[0];
                    let speed = (value - position[0]) / 0.0001;
                    assert!((speed - expected).abs() < 0.02, "{animation:?}: {speed} != {expected}");
                }
                assert_eq!(
                    animator.advance([-1.0], [true], animation, options, Duration::from_secs(10), true),
                    ([-1.0], [false]),
                );
                assert_eq!(
                    animator.advance([-1.0], [true], animation, options, Duration::from_secs(11), true),
                    ([-1.0], [false]),
                );
            }
        }
    }
}

#[test]
fn spring_timestep_and_bounce() {
    let options = AnimationOptions::default();
    for bounce in [1.0, 0.5, 0.000001, 0.0, -0.000001, -0.5, -0.999] {
        let spring = Spring::new(Duration::from_millis(500), bounce);
        let options = AnimationOptions {
            velocity: Some(3.0),
            ..options
        };
        let mut direct = Animator::new([0.0], Animation::default(), options);
        direct.advance([10.0], [true], spring, options, Duration::ZERO, true);
        let mut divided = direct.clone();
        let expected = direct
            .advance([10.0], [true], spring, options, Duration::from_millis(100), true)
            .0[0];
        let mut actual = 0.0;
        for millis in 1..=100 {
            actual = divided
                .advance([10.0], [true], spring, options, Duration::from_millis(millis), true)
                .0[0];
        }
        assert!((actual - expected).abs() < 0.0001, "{bounce}: {actual} != {expected}");
        assert!(
            direct
                .advance([10.0], [true], spring, options, Duration::from_secs(100_000), true)
                .0[0]
                .is_finite()
        );
        if bounce < 1.0 {
            assert!(!direct.is_active());
        }
    }
    let mut animator = Animator::new([0.0], Animation::default(), options);
    let instant = Spring::new(Duration::ZERO, 0.3);
    assert_eq!(
        animator.advance([10.0], [true], instant, options, Duration::ZERO, true),
        ([10.0], [false])
    );
}

#[test]
fn integer_springs_preserve_fractional_motion() {
    let options = AnimationOptions::default();
    let spring = Animation::Spring(Spring::new(Duration::from_millis(500), 0.0));
    let base = 2_000_000_000;
    let mut cells = Animator::new([base], Animation::default(), options);
    let mut pixels = Animator::new([0.0], Animation::default(), options);
    cells.advance([base + 8], [true], spring, options, Duration::ZERO, true);
    pixels.advance([8.0], [true], spring, options, Duration::ZERO, true);
    let mut cell = base;
    for millis in 1..=2000 {
        let time = Duration::from_millis(millis);
        cell = cells.advance([base + 8], [true], spring, options, time, true).0[0];
        let pixel = pixels.advance([8.0], [true], spring, options, time, true).0[0];
        assert!(((cell - base) as f32 - pixel).abs() <= 0.50001);
    }
    assert_eq!(cell, base + 8);
    assert!(!cells.is_active());
}

#[test]
fn spring_motion_measurement_and_settling() {
    let (mut frame, mut context) = frame(Size::uniform(100.0));
    let id = Id::new("spring motion");
    let spring = Spring::new(Duration::from_millis(500), 0.0);
    for (extent, millis) in [(1.0, 0), (3.0, 0), (3.0, 100), (3.0, 10_000)] {
        let time = Duration::from_millis(millis);
        let measured = render_inputs(&mut frame, &mut context, time, [], |ui: Ui<'_>| {
            ui.layout(Column)
                .child()
                .id(id)
                .motion(Motion::new(spring).height())
                .build_measured(Constraints::loose(Size::uniform(100.0)), |mut ui: Ui<'_>| {
                    ui.insert(Fill('X', Size::new(1.0, extent)));
                })
                .1
        });
        let expected = if millis == 0 {
            1.0
        } else if millis == 100 {
            let omega_t = std::f32::consts::TAU * 2.0 * 0.1;
            3.0 - 2.0 * (1.0 + omega_t) * (-omega_t).exp()
        } else {
            3.0
        };
        assert!((measured.height - expected).abs() < 0.00001);
        assert_eq!(frame.area(id).unwrap().height, measured.height);
    }
    render_inputs(&mut frame, &mut context, Duration::from_secs(10), [], |ui: Ui<'_>| {
        ui.layout(Column)
            .child()
            .id(id)
            .motion(Motion::new(spring).height())
            .insert(Fill('X', Size::new(1.0, 3.0)));
    });
    assert!(!frame.has_pending_redraw());
}

#[test]
fn easing_curves() {
    assert_eq!(Easing::Function(|t| (t * t, 2.0 * t)).sample(0.5), (0.25, 1.0));
    for [x1, y1, x2, y2] in [
        [0.8, -0.5, 0.2, 1.5],
        [0.0, 1.0, 0.0, 1.0],
        [1.0, 0.0, 1.0, 0.0],
        [1.0 / 3.0, -0.5, 2.0 / 3.0, 1.5],
    ] {
        let easing = Easing::Bezier([x1, y1, x2, y2]);
        assert_eq!(easing.sample(0.0).0, 0.0);
        assert_eq!(easing.sample(1.0).0, 1.0);
        for i in 1..100 {
            let t = i as f32 / 100.0;
            let x = 3.0 * (1.0 - t).powi(2) * t * x1 + 3.0 * (1.0 - t) * t * t * x2 + t.powi(3);
            let y = 3.0 * (1.0 - t).powi(2) * t * y1 + 3.0 * (1.0 - t) * t * t * y2 + t.powi(3);
            let expected = if x1 == 0.0 && x2 == 0.0 {
                (1.0 - (1.0 - (x as f64).cbrt()).powi(3)) as f32
            } else if x1 == 1.0 && x2 == 1.0 {
                (1.0 - (1.0 - x as f64).cbrt()).powi(3) as f32
            } else {
                y
            };
            assert!(
                (easing.sample(x).0 - expected).abs() < 0.00001,
                "{x}: {} != {expected}",
                easing.sample(x).0
            );
        }
    }
    for y in [-f32::MAX, f32::MAX] {
        let easing = Easing::Bezier([1.0 / 3.0, y, 2.0 / 3.0, y]);
        assert_eq!(easing.sample(0.0).0, 0.0);
        assert_eq!(easing.sample(1.0).0, 1.0);
        assert!(easing.sample(0.5).0.is_finite());
    }
    assert!(Easing::Bezier([0.2, 1.5, 0.8, 1.5]).sample(0.5).0 > 1.0);
}

#[test]
fn spring_sizes_stay_nonnegative_and_resize_snaps() {
    let (mut frame, mut context) = frame(Size::uniform(100.0));
    let id = Id::new("bounded spring");
    let build = |extent| {
        move |ui: Ui<'_>| {
            ui.layout(Column)
                .child()
                .id(id)
                .motion(Motion::new(Spring::new(Duration::from_millis(250), 0.4)).size())
                .insert(Fill('X', Size::uniform(extent)));
        }
    };
    render_inputs(&mut frame, &mut context, Duration::ZERO, [], build(1.0));
    render_inputs(&mut frame, &mut context, Duration::ZERO, [], build(0.0));
    render_inputs(&mut frame, &mut context, Duration::from_millis(125), [], build(0.0));
    assert_eq!(frame.area(id).unwrap().size(), Size::ZERO);
    context.info = FrameInfo::new(Size::uniform(200.0));
    render_inputs(&mut frame, &mut context, Duration::from_millis(130), [], build(3.0));
    assert_eq!(frame.area(id).unwrap().size(), Size::uniform(3.0));
    render_inputs(&mut frame, &mut context, Duration::from_secs(1), [], build(3.0));
    assert_eq!(frame.area(id).unwrap().size(), Size::uniform(3.0));
    assert!(!frame.has_pending_redraw());
}
