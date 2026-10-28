use super::*;

/// reports up to two visible contacts, capturing those its predicate accepts
#[derive(Clone, Copy)]
struct Watch(fn(&Contact<f32>) -> bool);

const WATCH: Watch = Watch(|_| false);
const CAPTURE: Watch = Watch(|_| true);

impl Interact<f32> for Watch {
    /// each contact with whether this update captured it
    type Response = [Option<(Contact<f32>, bool)>; 2];

    fn update(&mut self, cx: &mut InteractCx<'_, f32>) -> Self::Response {
        let mut seen = [None; 2];
        for (seen, contact) in seen.iter_mut().zip(cx.contacts().filter(|contact| contact.free)) {
            *seen = Some((contact, false));
        }
        for (contact, captured) in seen.iter_mut().flatten() {
            *captured = (self.0)(contact) && cx.capture(contact.id);
        }
        seen
    }
}

/// claims two contacts and reports their distance
#[derive(Clone, Copy)]
struct Pinch;

impl Interact<f32> for Pinch {
    type Response = Option<f32>;

    fn update(&mut self, cx: &mut InteractCx<'_, f32>) -> Option<f32> {
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
        ui.interact(Scroll::new());
        added.get() && ui.interact(Press::new()).hovered
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
    assert_eq!(first.source, ContactSource::Touch);
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
fn capture_cancels_click() {
    for ending in [Up, Cancel] {
        let (mut frame, mut context) = frame(Size::uniform(200.0));
        let build = |mut ui: Ui<'_>| {
            // takes a press once it moves, like a scroll view panning
            let seen = ui.interact(Watch(|contact| contact.phase == Changed))[0];
            let mut root = ui.layout(Overlay);
            let mut child = root.child().id(Id::new("button"));
            child.insert(Fill('B', Size::uniform(200.0)));
            (seen, child.interact(Press::new()))
        };
        render(&mut frame, &mut context, &build);
        let mut step = |input| render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
        // the button claims the press, hiding it from the ancestor
        let (seen, response) = step(pointer(Some(1), Down, [10.0, 10.0]));
        assert!(response.active && seen.is_none());
        let (seen, response) = step(pointer(Some(1), Move, [12.0, 10.0]));
        assert!(response.active && seen.is_none());
        // past the slop the button lets go first, then the ancestor takes it in the same build
        let (seen, response) = step(pointer(Some(1), Move, [30.0, 10.0]));
        assert!(seen.unwrap().1);
        assert!(response.deactivated && !response.active);
        assert!(!step(pointer(Some(1), ending, [30.0, 10.0])).1.clicked);
    }
}

#[test]
fn gestures_on_one_widget_share_captures() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let build = |mut ui: Ui<'_>| {
        ui.interact(CAPTURE);
        let pinch = ui.interact(Pinch);
        let mut root = ui.layout(Overlay);
        let mut child = root.child().id(Id::new("button"));
        child.insert(Fill('B', Size::uniform(200.0)));
        (pinch, child.interact(Press::new()).clicked)
    };
    render(&mut frame, &mut context, &build);
    let mut step = |input| render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
    // the button claims the first finger, the root captures the second
    assert_eq!(step(pointer(Some(1), Down, [10.0, 10.0])), (None, false));
    assert_eq!(step(pointer(Some(2), Down, [20.0, 10.0])), (None, false));
    // once the first finger moves the button lets go and the pinch joins the root's capture
    assert_eq!(step(pointer(Some(1), Move, [0.0, 10.0])), (Some(20.0), false));
    assert!(!step(pointer(Some(1), Up, [0.0, 10.0])).1);
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
    struct Count([u64; 32]);

    impl Interact<f32> for Count {
        type Response = u64;

        fn update(&mut self, cx: &mut InteractCx<'_, f32>) -> u64 {
            self.0[31] += 1;
            cx.wake_at(Duration::from_secs(1));
            self.0[31]
        }
    }

    #[derive(Clone, Copy)]
    struct Empty;

    impl Interact<f32> for Empty {
        type Response = ();

        fn update(&mut self, _: &mut InteractCx<'_, f32>) {}
    }

    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let rows = Cell::new((0, 3));
    let build = |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut updates = [None; 4];
        let (start, end) = rows.get();
        for (row, updates) in updates.iter_mut().enumerate().take(end).skip(start) {
            let mut child = root.child().id(Id::new(row));
            child.interact(Empty);
            *updates = Some(child.interact(Count([0; 32])));
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
            (
                ui.interact_widget(id, Press::new()).hovered,
                ui.interact_widget(id, Focus),
            )
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
            let response = ui.interact(Press::new());
            ui.interact(capture);
            response.clicked
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

    impl Interact<f32> for Count {
        type Response = ();

        fn update(&mut self, _: &mut InteractCx<'_, f32>) {
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

#[derive(Default)]
struct Integer;
impl Context for Integer {
    type Scalar = i32;
}

type Scene = InputScene<(), Press>;

/// one root node running a single interaction kind
struct InputScene<C: Context, K> {
    frame: Frame<C>,
    context: C,
    kind: K,
}

impl<C: Context + Default, K: Interact<C::Scalar>> InputScene<C, K> {
    fn new(kind: K) -> Self {
        let mut scene = Self {
            frame: Frame::default(),
            context: C::default(),
            kind,
        };
        scene.step(0, Input::None);
        scene
    }
    fn step(&mut self, time: u64, input: Input<C::Scalar>) -> K::Response {
        let output = self.frame.build(
            &mut self.context,
            FrameInfo::new(Size::uniform(C::Scalar::from_f32(200.0))),
            Duration::from_millis(time),
            input,
            |mut ui: blit::Ui<'_, C>| ui.interact(self.kind),
        );
        self.frame.layout(&mut self.context);
        output
    }
}

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
fn scroll() {
    // zero delta endings stay with the owner even outside its bounds
    for horizontal in [true, false] {
        let mut scene = InputScene::<Integer, _>::new(Scroll::new());
        let held = ScrollResponse {
            active: true,
            continuous: true,
            ..ScrollResponse::default()
        };
        let ended = ScrollResponse {
            deactivated: true,
            released: true,
            continuous: true,
            ..ScrollResponse::default()
        };
        for (phase, expected, after) in [
            (
                ScrollPhase::Started,
                ScrollResponse {
                    activated: true,
                    ..held
                },
                held,
            ),
            (ScrollPhase::Moved, held, held),
            (ScrollPhase::Ended, ended, ScrollResponse::default()),
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
            assert_eq!(response, ScrollResponse { delta, ..expected });
            assert_eq!(scene.step(11, Input::None), after);
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
            let cancelled = scene.step(21, cancel);
            assert!(cancelled.deactivated && !cancelled.released && !cancelled.active);
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
    assert!(response.active && wheel.delta != Point::ZERO);
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
        row.interact(Press::new()).hovered
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
    let mut wheel = |phase, continuous, delta_x, delta_y| {
        let input = Input::Scroll {
            position: Point::new(10.0, 10.0),
            delta_x,
            delta_y,
            continuous,
            phase,
            modifiers: Modifiers::NONE,
        };
        let (list, carousel) = render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
        (list.delta != Point::ZERO, carousel.delta != Point::ZERO)
    };
    assert_eq!(wheel(ScrollPhase::Moved, false, 5.0, 1.0), (false, true));
    assert_eq!(wheel(ScrollPhase::Moved, false, 1.0, 5.0), (true, false));
    // a trackpad sequence waits for a direction, then stays with whoever took it
    assert_eq!(wheel(ScrollPhase::Started, true, 0.0, 0.0), (false, false));
    assert_eq!(wheel(ScrollPhase::Moved, true, 1.0, 5.0), (true, false));
    assert_eq!(wheel(ScrollPhase::Moved, true, 5.0, 1.0), (true, false));
}

#[test]
fn removed_scroll_owner_keeps_its_sequence() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let owner = Cell::new("first");
    let build = |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut list = root.child().id(Id::new(owner.get()));
        list.insert(Fill('L', Size::uniform(200.0)));
        list.interact(Scroll::new())
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
    assert!(scroll(ScrollPhase::Started).activated);
    // another widget takes the owner's node while the gesture continues
    owner.set("second");
    assert_eq!(scroll(ScrollPhase::Moved), ScrollResponse::default());
    assert_eq!(scroll(ScrollPhase::Moved), ScrollResponse::default());
    assert!(scroll(ScrollPhase::Started).activated);
}
