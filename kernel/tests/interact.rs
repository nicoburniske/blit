use super::*;

/// whether the pointer is over this widget
#[derive(Clone, Copy)]
struct Hover;

impl<C: Context> Interact<C> for Hover {
    type State = ();
    type Response = bool;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, C::Scalar>) -> bool {
        cx.hovered()
    }
}

/// claims a contact as it ends, reporting whether the claim held
#[derive(Clone, Copy)]
struct ClaimEnded;

impl<C: Context> Interact<C> for ClaimEnded {
    type State = ();
    type Response = bool;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, C::Scalar>) -> bool {
        let ended = cx.contacts().find(|contact| contact.phase == Ended);
        ended.is_some_and(|contact| cx.claim(contact.id))
    }
}

/// scrolling over this widget, holding its sequence
#[derive(Clone, Copy)]
struct Wheel;

impl<C: Context> Interact<C> for Wheel {
    type State = ();
    type Response = Option<Point<f32>>;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, C::Scalar>) -> Option<Point<f32>> {
        let event = cx.scroll()?;
        cx.capture_scroll();
        Some(event.delta)
    }
}

/// claims two contacts and reports their distance
#[derive(Clone, Copy)]
struct Pinch;

impl<C: Context<Scalar = f32>> Interact<C> for Pinch {
    type State = ();
    type Response = Option<f32>;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, f32>) -> Option<f32> {
        let mut free = cx.contacts().filter(|contact| contact.free);
        let [a, b] = [free.next()?, free.next()?];
        if a.phase == Cancelled || b.phase == Cancelled {
            return None;
        }
        cx.capture(a.id);
        cx.capture(b.id);
        Some((b.position.x - a.position.x).hypot(b.position.y - a.position.y))
    }
}

#[test]
fn new_kind_gets_geometry_after_layout() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let added = Cell::new(false);
    let build = |mut ui: Ui<'_>| {
        ui.interact(WATCH);
        added.get() && ui.interact(Hover)
    };
    render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [pointer(None, Move, [10.0, 10.0])],
        &build,
    );
    // the app changes state and asks for a frame
    added.set(true);
    frame.request_frame();
    assert!(!render(&mut frame, &mut context, &build));
    assert!(render(&mut frame, &mut context, &build));
    assert!(!frame.has_pending_redraw());
}

#[test]
fn gesture_sees_independent_contacts() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let build = |mut ui: Ui<'_>| ui.interact(WATCH).map(|seen| seen.map(|(contact, _)| contact));
    render(&mut frame, &mut context, &build);
    let mut step = |input| render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
    step(pointer(Some(1), Down, [10.0, 10.0]));
    step(pointer(Some(2), Down, [20.0, 10.0]));
    let [first, second] = step(pointer(Some(1), Move, [15.0, 10.0])).map(Option::unwrap);
    assert_eq!(first.source, ContactSource::Touch(1));
    assert_eq!(second.source, ContactSource::Touch(2));
    assert_eq!(first.delta, Point::new(5.0, 0.0));
    assert_eq!(second.phase, Held);
    let [ended, held] = step(pointer(Some(1), Up, [17.0, 10.0])).map(Option::unwrap);
    assert_eq!((ended.phase, ended.delta), (Ended, Point::new(2.0, 0.0)));
    assert_eq!(held.id, second.id);
    assert_eq!(step(Input::None), [Some(held), None]);
}

#[test]
fn deepest_gesture_captures_first() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let child_id = Id::new("child");
    let build = |mut ui: Ui<'_>| {
        // requested out of preorder and before the child node exists
        let child = ui.interact_widget(child_id, CAPTURE)[0];
        let parent = ui.interact(CAPTURE)[0];
        let mut root = ui.layout(Overlay);
        root.child().id(child_id).insert(Fill('C', Size::uniform(200.0)));
        (parent, child)
    };
    render(&mut frame, &mut context, &build);
    let down = pointer(Some(1), Down, [10.0, 10.0]);
    let (parent, child) = render_inputs(&mut frame, &mut context, Duration::ZERO, [down], &build);
    assert_eq!(parent, None);
    assert!(child.unwrap().1);
}

#[test]
fn gestures_on_one_widget_share_captures() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let build = |mut ui: Ui<'_>| {
        ui.interact(CAPTURE);
        ui.interact(Pinch)
    };
    render(&mut frame, &mut context, &build);
    let mut step = |input| render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
    assert_eq!(step(pointer(Some(1), Down, [10.0, 10.0])), None);
    // the pinch still sees the fingers its widget captured
    assert_eq!(step(pointer(Some(2), Down, [20.0, 10.0])), Some(10.0));
    assert_eq!(step(pointer(Some(1), Move, [0.0, 10.0])), Some(20.0));
}

#[test]
fn unmounted_owner_retires_contact() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let mounted = Cell::new(true);
    let build = |mut ui: Ui<'_>| {
        let root = ui.interact(WATCH)[0];
        let mut layout = ui.layout(Overlay);
        let owned = mounted.get().then(|| {
            let mut child = layout.child().id(Id::new("child"));
            child.insert(Fill('B', Size::uniform(200.0)));
            child.interact(CAPTURE)[0]
        });
        (root, owned.flatten())
    };
    render(&mut frame, &mut context, &build);
    let mut step = |input| render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
    let (root, owned) = step(pointer(Some(1), Down, [10.0, 10.0]));
    assert!(root.is_none() && owned.unwrap().1);
    mounted.set(false);
    step(Input::None);
    assert_eq!(step(pointer(Some(1), Move, [30.0, 10.0])), (None, None));
}

#[test]
fn gesture_state_lives_while_requested() {
    // larger than any inline limit
    #[derive(Clone, Copy)]
    struct Count;

    impl<C: Context<Scalar = f32>> Interact<C> for Count {
        type State = [u64; 32];
        type Response = u64;

        fn update(&self, count: &mut [u64; 32], cx: &mut InteractCx<'_, f32>) -> u64 {
            count[31] += 1;
            cx.wake_at(Duration::from_secs(1));
            count[31]
        }
    }

    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let rows = Cell::new((0, 3));
    let build = |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut updates = [None; 4];
        let (start, end) = rows.get();
        for (row, updates) in updates.iter_mut().enumerate().take(end).skip(start) {
            let mut child = root.child().id(Id::new(row));
            child.interact(Hover);
            *updates = Some(child.interact(Count));
        }
        updates
    };
    let mut step = || render(&mut frame, &mut context, &build);
    assert_eq!(step(), [Some(1), Some(1), Some(1), None]);
    assert_eq!(step(), [Some(2), Some(2), Some(2), None]);
    rows.set((1, 4));
    assert_eq!(step(), [None, Some(3), Some(3), Some(1)]);
    // row 0 returns in a recycled cell with fresh state
    rows.set((0, 3));
    assert_eq!(step(), [Some(1), Some(4), Some(4), None]);
    assert_eq!(frame.next_timer_deadline(), Some(Duration::from_secs(1)));
    rows.set((0, 0));
    render(&mut frame, &mut context, &build);
    render(&mut frame, &mut context, &build);
    assert_eq!(frame.next_timer_deadline(), None);
}

#[test]
fn named_kinds_follow_reordered_nodes() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let order = Cell::new([Some(0), Some(1), Some(2)]);
    let build = |mut ui: Ui<'_>| {
        let responses = [0, 1, 2].map(|id| {
            let id = Id::new(id);
            let pressed = ui.interact_widget(id, WATCH)[0].is_some_and(|(contact, _)| contact.phase == Started);
            (ui.interact_widget(id, Hover), pressed)
        });
        let mut root = ui.layout(Rects);
        for (index, id) in order.get().into_iter().enumerate() {
            if let Some(id) = id {
                root.child()
                    .id(Id::new(id))
                    .item(Rect::new(index as f32 * 50.0, 0.0, 50.0, 100.0));
            }
        }
        responses
    };
    for nodes in [
        [Some(0), Some(1), Some(2)],
        [Some(2), Some(0), None],
        [Some(1), Some(2), Some(0)],
    ] {
        order.set(nodes);
        render(&mut frame, &mut context, &build);
        let responses = render_inputs(
            &mut frame,
            &mut context,
            Duration::ZERO,
            [pointer(None, Down, [5.0, 5.0])],
            &build,
        );
        let hovered = nodes[0].unwrap();
        assert_eq!(responses, [0, 1, 2].map(|id| (id == hovered, id == hovered)));
    }
}

#[test]
fn kind_order_follows_requests() {
    for seed in [false, true] {
        let (mut frame, mut context) = frame(Size::uniform(200.0));
        let capture = Watch(|contact| contact.phase == Ended);
        if seed {
            render(&mut frame, &mut context, |mut ui: Ui<'_>| {
                ui.interact_widget(Id::new("absent"), capture);
            });
        }
        let build = |mut ui: Ui<'_>| {
            let claimed = ui.interact(ClaimEnded);
            ui.interact(capture);
            claimed
        };
        render(&mut frame, &mut context, &build);
        assert!(!render_inputs(
            &mut frame,
            &mut context,
            Duration::ZERO,
            [pointer(Some(1), Down, [10.0, 10.0]), pointer(Some(1), Up, [10.0, 10.0])],
            &build,
        ));
    }
}

#[test]
fn kind_requested_twice_keeps_one_slot() {
    static UPDATES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    #[derive(Clone, Copy)]
    struct Count;

    impl<C: Context<Scalar = f32>> Interact<C> for Count {
        type State = ();
        type Response = ();

        fn update(&self, _: &mut (), _: &mut InteractCx<'_, f32>) {
            UPDATES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let build = |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        for id in 0..64 {
            let mut child = root.child().id(Id::new(id));
            child.interact(Count);
            child.interact(Count);
        }
        for id in 0..64 {
            root.interact_widget(Id::new(id), Count);
        }
    };
    for _ in 0..3 {
        UPDATES.store(0, std::sync::atomic::Ordering::Relaxed);
        render(&mut frame, &mut context, &build);
        assert_eq!(UPDATES.load(std::sync::atomic::Ordering::Relaxed), 64);
    }
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
            field.interact(WATCH);
            field.focused() == Some(field_id)
        };
        assert!(render(&mut frame, &mut context, &build));
        let mut step = |input| render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
        // mouse clears focus on press and touch on release
        assert_eq!(step(pointer(id, Down, [150.0, 10.0])), id.is_some());
        assert!(!step(pointer(id, Up, [150.0, 10.0])));
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
                let seen = ui.interact(WATCH)[0].is_some();
                let wheel = ui.interact(Wheel);
                ui.insert(Fill('C', Size::new(5.0, 1.0)));
                (seen, wheel)
            })
    };
    render(&mut frame, &mut context, &build);
    for (phase, x, inside) in [(Down, 4.5, false), (Up, 4.5, false), (Down, 2.5, true)] {
        let (seen, _) = render_inputs(
            &mut frame,
            &mut context,
            Duration::ZERO,
            [pointer(None, phase, [x, 0.5])],
            &build,
        );
        assert_eq!(seen, inside);
    }
    let (seen, wheel) = render_inputs(
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
    assert!(seen && wheel.is_some_and(|delta| delta != Point::ZERO));
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
            child.interact(WATCH)[0].is_some()
        };
        render(&mut frame, &mut context, &build);
        assert!(context.commands.contains(&Paint('C', Rect::new(3.0, 3.0, 1.0, 1.0))));
        for (position, inside) in [([3.1, 3.1], true), ([3.9, 3.9], shape == ClipShape::Unbounded)] {
            let seen = render_inputs(
                &mut frame,
                &mut context,
                Duration::ZERO,
                [pointer(None, Down, position), pointer(None, Up, position)],
                &build,
            );
            assert_eq!(seen, inside);
        }
    }
}

#[test]
fn input_settles_against_its_layout() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let offset = Cell::new(0.0);
    let build = |ui: Ui<'_>| {
        // scrolling moves the row under a still pointer
        if matches!(ui.input(), Input::Scroll { .. }) {
            offset.set(100.0);
        }
        let mut root = ui.layout(Rects);
        let mut row = root
            .child()
            .item(Rect::new(offset.get(), 0.0, 100.0, 200.0))
            .id(Id::new("row"));
        row.interact(Hover)
    };
    render(&mut frame, &mut context, &build);
    let scroll = Input::Scroll {
        position: Point::new(150.0, 10.0),
        delta_x: 0.0,
        delta_y: 1.0,
        modifiers: Modifiers::NONE,
        continuous: false,
        phase: ScrollPhase::Moved,
    };
    // the scrolled build still sees the layout from before it
    assert!(!render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [scroll],
        &build
    ));
    assert!(frame.has_pending_redraw());
    assert!(render(&mut frame, &mut context, &build));
    assert!(!frame.has_pending_redraw());
}

#[test]
fn removed_scroll_owner_keeps_its_sequence() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let owner = Cell::new("first");
    let build = |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut list = root.child().id(Id::new(owner.get()));
        list.insert(Fill('L', Size::uniform(200.0)));
        list.interact(Wheel)
    };
    render(&mut frame, &mut context, &build);
    let mut scroll = |phase| {
        let input = Input::Scroll {
            position: Point::new(10.0, 10.0),
            delta_x: 0.0,
            delta_y: 1.0,
            continuous: true,
            phase,
            modifiers: Modifiers::NONE,
        };
        render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build)
    };
    assert!(scroll(ScrollPhase::Started).is_some());
    // another widget takes the owner's node while the gesture continues
    owner.set("second");
    assert_eq!(scroll(ScrollPhase::Moved), None);
    assert_eq!(scroll(ScrollPhase::Moved), None);
    assert!(scroll(ScrollPhase::Started).is_some());
}
