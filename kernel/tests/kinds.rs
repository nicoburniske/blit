use super::*;

/// reports up to two routed contacts and captures those its predicate accepts
#[derive(Clone, Copy)]
struct Watch(fn(&Contact<f32>) -> bool);

const WATCH: Watch = Watch(|_| false);
const CAPTURE: Watch = Watch(|_| true);

impl Interact<f32> for Watch {
    type Output = [Option<Contact<f32>>; 2];

    fn update(&mut self, cx: &mut InteractCx<'_, f32>) -> Self::Output {
        let mut seen = [None; 2];
        for (seen, contact) in seen.iter_mut().zip(cx.contacts()) {
            *seen = Some(contact);
        }
        for contact in seen.iter_mut().flatten().filter(|contact| (self.0)(contact)) {
            cx.capture(contact.id);
            contact.hold = Hold::Captured;
        }
        seen
    }
}

/// claims two contacts and reports their distance
#[derive(Clone, Copy)]
struct Pinch;

impl Interact<f32> for Pinch {
    type Output = Option<f32>;

    fn update(&mut self, cx: &mut InteractCx<'_, f32>) -> Option<f32> {
        let [a, b] = cx
            .contacts()
            .next_array()
            .filter(|[a, b]| a.phase != Cancelled && b.phase != Cancelled)?;
        cx.capture(a.id);
        cx.capture(b.id);
        Some((b.position.x - a.position.x).hypot(b.position.y - a.position.y))
    }
}

#[test]
fn gesture_sees_independent_contacts() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let build = |mut ui: Ui<'_>| ui.interact(WATCH);
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
    assert!(child.unwrap().hold == Hold::Captured);
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
        let mut step = |input| {
            let output = render_inputs(&mut frame, &mut context, Duration::ZERO, [input], &build);
            (output, frame.has_pending_redraw())
        };
        let ((seen, response), _) = step(pointer(Some(1), Down, [10.0, 10.0]));
        assert!(response.active && seen.unwrap().hold == Hold::ClaimedElsewhere);
        // the button updates again after the capture, within the same build
        let ((seen, response), redraw) = step(pointer(Some(1), Move, [12.0, 10.0]));
        assert!(seen.unwrap().hold == Hold::Captured && !redraw);
        assert!(response.deactivated && !response.active);
        assert!(!step(pointer(Some(1), ending, [12.0, 10.0])).0.1.clicked);
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
    assert_eq!(step(pointer(Some(1), Down, [10.0, 10.0])).0, None);
    // the pinch joins the contact its widget already captured
    for (input, span) in [
        (pointer(Some(2), Down, [20.0, 10.0]), 10.0),
        (pointer(Some(1), Move, [0.0, 10.0]), 20.0),
    ] {
        assert_eq!(step(input), (Some(span), false));
    }
    assert!(!step(pointer(Some(2), Up, [20.0, 10.0])).1);
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
    assert!(root.is_none() && owned.unwrap().hold == Hold::Captured);
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
        type Output = u64;

        fn update(&mut self, cx: &mut InteractCx<'_, f32>) -> u64 {
            self.0[31] += 1;
            cx.wake_at(Duration::from_secs(1));
            self.0[31]
        }
    }

    #[derive(Clone, Copy)]
    struct Empty;

    impl Interact<f32> for Empty {
        type Output = ();

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
    assert_eq!(step(), [Some(0), Some(0), Some(0), None]);
    assert_eq!(step(), [Some(1), Some(1), Some(1), None]);
    rows.set((1, 4));
    assert_eq!(step(), [None, Some(2), Some(2), Some(0)]);
    // row 0 returns in a recycled cell with fresh state
    rows.set((0, 3));
    assert_eq!(step(), [Some(0), Some(3), Some(3), None]);
    assert_eq!(frame.next_timer_deadline(), Some(Duration::from_secs(1)));
    rows.set((0, 0));
    render(&mut frame, &mut context, &build);
    render(&mut frame, &mut context, &build);
    assert_eq!(frame.next_timer_deadline(), None);
}
