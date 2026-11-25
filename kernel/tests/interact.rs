use blit::KeyboardEvent;

use super::*;

#[test]
fn keyboard_input_bubbles_until_claimed() {
    #[derive(Clone, Copy)]
    struct Keyboard(char, Option<char>);

    impl Interact<TestContext> for Keyboard {
        type State = ();
        type Response = Option<char>;

        fn update(&self, _: &mut (), cx: &mut InteractCx<'_, f32>) -> Self::Response {
            let input = cx.keyboard()?;
            if self.1.is_some_and(|character| input != KeyboardEvent::Text(character)) {
                return None;
            }
            cx.claim_keyboard();
            Some(self.0)
        }
    }

    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let focused = Cell::new(None);
    let build = |mut ui: Ui<'_>| {
        if let Some(key) = focused.get() {
            ui.node(Scope::ROOT.key(key as u32)).focus();
        } else {
            ui.clear_focus();
        }
        let mut root = ui.layout(Overlay);
        let mut branch = |parent, child| {
            let mut branch = root.child_key(Scope::ROOT.key(parent as u32));
            let parent = branch.interact(Keyboard(parent, None));
            let child = branch
                .layout(Overlay)
                .child_key(Scope::ROOT.key(child as u32))
                .interact(Keyboard(child, Some(child)));
            [parent, child]
        };
        let [a, b] = branch('a', 'b');
        let [c, d] = branch('c', 'd');
        let mut handled = [a, b, c, d].into_iter().flatten();
        let winner = handled.next();
        assert_eq!(handled.next(), None, "input was handled more than once");
        winner
    };
    assert_eq!(render(&mut kernel, &mut context, build), None);
    let mut next = |input| render_inputs(&mut kernel, &mut context, Duration::ZERO, [input], build);
    assert_eq!(next(Input::Text('b')), Some('b'));
    assert_eq!(next(Input::Text('d')), Some('a'));
    focused.set(Some('d'));
    next(Input::None);
    assert_eq!(next(Input::Text('d')), Some('d'));
    assert_eq!(next(Input::Text('x')), Some('c'));
    assert_eq!(next(Input::MouseLeave), None);
}

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
        ended.is_some_and(|mut contact| contact.claim())
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
        let [mut a, mut b] = [free.next()?, free.next()?];
        if a.phase == Cancelled || b.phase == Cancelled {
            return None;
        }
        a.capture();
        b.capture();
        Some((b.position.x - a.position.x).hypot(b.position.y - a.position.y))
    }
}

#[test]
fn new_kind_gets_geometry_after_layout() {
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let added = Cell::new(false);
    let build = |mut ui: Ui<'_>| {
        ui.interact(WATCH);
        added.get() && ui.interact(Hover)
    };
    render_inputs(
        &mut kernel,
        &mut context,
        Duration::ZERO,
        [pointer(None, Move, [10.0, 10.0])],
        &build,
    );
    // the app changes state and asks for a frame
    added.set(true);
    kernel.request_frame();
    assert!(!render(&mut kernel, &mut context, &build));
    assert!(render(&mut kernel, &mut context, &build));
    assert!(!kernel.has_pending_redraw());
}

#[test]
fn gesture_sees_independent_contacts() {
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let build = |mut ui: Ui<'_>| ui.interact(WATCH).map(|seen| seen.map(|(contact, _)| contact));
    render(&mut kernel, &mut context, &build);
    let mut step = |input| render_inputs(&mut kernel, &mut context, Duration::ZERO, [input], &build);
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
    let child_key = 1000;
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let build = |mut ui: Ui<'_>| {
        // requested out of preorder and before the child node exists
        let child = ui.node(Scope::ROOT.key(child_key)).interact(CAPTURE)[0];
        let parent = ui.interact(CAPTURE)[0];
        let mut root = ui.layout(Overlay);
        root.child_key(Scope::ROOT.key(child_key))
            .insert(Fill('C', Size::uniform(200.0)));
        (parent, child)
    };
    render(&mut kernel, &mut context, &build);
    let down = pointer(Some(1), Down, [10.0, 10.0]);
    let (parent, child) = render_inputs(&mut kernel, &mut context, Duration::ZERO, [down], &build);
    assert_eq!(parent, None);
    assert!(child.unwrap().1);
}

#[test]
fn gestures_on_one_widget_share_captures() {
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let build = |mut ui: Ui<'_>| {
        ui.interact(CAPTURE);
        ui.interact(Pinch)
    };
    render(&mut kernel, &mut context, &build);
    let mut step = |input| render_inputs(&mut kernel, &mut context, Duration::ZERO, [input], &build);
    assert_eq!(step(pointer(Some(1), Down, [10.0, 10.0])), None);
    // the pinch still sees the fingers its widget captured
    assert_eq!(step(pointer(Some(2), Down, [20.0, 10.0])), Some(10.0));
    assert_eq!(step(pointer(Some(1), Move, [0.0, 10.0])), Some(20.0));
}

#[test]
fn unmounted_owner_retires_contact() {
    let child = 1000;
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let mounted = Cell::new(true);
    let build = |mut ui: Ui<'_>| {
        let root = ui.interact(WATCH)[0];
        let mut layout = ui.layout(Overlay);
        let owned = mounted.get().then(|| {
            let mut child = layout.child_key(Scope::ROOT.key(child));
            child.insert(Fill('B', Size::uniform(200.0)));
            child.interact(CAPTURE)[0]
        });
        (root, owned.flatten())
    };
    render(&mut kernel, &mut context, &build);
    let mut step = |input| render_inputs(&mut kernel, &mut context, Duration::ZERO, [input], &build);
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

    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let rows = Cell::new((0, 3));
    let build = |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut updates = [None; 4];
        let (start, end) = rows.get();
        for (row, updates) in updates.iter_mut().enumerate().take(end).skip(start) {
            let mut child = root.child_key(Scope::ROOT.key(row));
            child.interact(Hover);
            *updates = Some(child.interact(Count));
        }
        updates
    };
    let mut step = || render(&mut kernel, &mut context, &build);
    assert_eq!(step(), [Some(1), Some(1), Some(1), None]);
    assert_eq!(step(), [Some(2), Some(2), Some(2), None]);
    rows.set((1, 4));
    assert_eq!(step(), [None, Some(3), Some(3), Some(1)]);
    // row 0 returns in a recycled cell with fresh state
    rows.set((0, 3));
    assert_eq!(step(), [Some(1), Some(4), Some(4), None]);
    assert_eq!(kernel.next_timer_deadline(), Some(Duration::from_secs(1)));
    rows.set((0, 0));
    render(&mut kernel, &mut context, &build);
    render(&mut kernel, &mut context, &build);
    assert_eq!(kernel.next_timer_deadline(), None);
}

#[test]
fn named_kinds_follow_reordered_nodes() {
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let order = Cell::new([Some(0), Some(1), Some(2)]);
    let build = |mut ui: Ui<'_>| {
        let responses = [0, 1, 2].map(|id| {
            let pressed =
                ui.node(Scope::ROOT.key(id)).interact(WATCH)[0].is_some_and(|(contact, _)| contact.phase == Started);
            (ui.node(Scope::ROOT.key(id)).interact(Hover), pressed)
        });
        let mut root = ui.layout(Rects);
        for (index, id) in order.get().into_iter().enumerate() {
            if let Some(id) = id {
                root.child_key(Scope::ROOT.key(id))
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
        render(&mut kernel, &mut context, &build);
        let responses = render_inputs(
            &mut kernel,
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
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let claim_first = Cell::new(true);
    let build = |mut ui: Ui<'_>| {
        let capture = Watch(|contact| contact.phase == Ended);
        if claim_first.get() {
            let claimed = ui.interact(ClaimEnded);
            ui.interact(capture);
            claimed
        } else {
            ui.interact(capture);
            ui.interact(ClaimEnded)
        }
    };
    for first in [true, false] {
        claim_first.set(first);
        render(&mut kernel, &mut context, &build);
        let claimed = render_inputs(
            &mut kernel,
            &mut context,
            Duration::ZERO,
            [pointer(Some(1), Down, [10.0, 10.0]), pointer(Some(1), Up, [10.0, 10.0])],
            &build,
        );
        assert_eq!(claimed, first);
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

    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let build = |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        for id in 0..64 {
            let mut child = root.child_key(Scope::ROOT.key(id));
            child.interact(Count);
            child.interact(Count);
        }
        for id in 0..64 {
            root.node(Scope::ROOT.key(id)).interact(Count);
        }
    };
    for _ in 0..3 {
        UPDATES.store(0, std::sync::atomic::Ordering::Relaxed);
        render(&mut kernel, &mut context, &build);
        assert_eq!(UPDATES.load(std::sync::atomic::Ordering::Relaxed), 64);
    }
}

#[test]
fn tapping_unregistered_background_clears_focus() {
    let field = 1001;
    for id in [None, Some(1)] {
        let (mut kernel, mut context) = kernel(Size::uniform(200.0));
        let focus = Cell::new(true);
        let build = |mut ui: Ui<'_>| {
            if focus.replace(false) {
                ui.node(Scope::ROOT.key(field)).focus();
            }
            let mut root = ui.layout(Rects);
            let mut field = root
                .child_key(Scope::ROOT.key(field))
                .item(Rect::new(0.0, 0.0, 100.0, 200.0));
            field.interact(WATCH);
            field.focused()
        };
        // programmatic focus shows from the next build
        assert!(!render(&mut kernel, &mut context, &build));
        assert!(render(&mut kernel, &mut context, &build));
        let mut step = |input| render_inputs(&mut kernel, &mut context, Duration::ZERO, [input], &build);
        // mouse clears focus on press and touch on release
        assert_eq!(step(pointer(id, Down, [150.0, 10.0])), id.is_some());
        assert!(!step(pointer(id, Up, [150.0, 10.0])));
    }
}

#[test]
fn clipping() {
    let (mut kernel, mut context) = kernel(Size::new(5.0, 1.0));
    let build = |ui: Ui<'_>| {
        ui.layout(Overlay)
            .child()
            .layout(Fixed(Size::new(3.0, 1.0)))
            .clip(0.0)
            .child()
            .build(|mut ui: Ui<'_>| {
                let seen = ui.interact(WATCH)[0].is_some();
                let wheel = ui.interact(Wheel);
                ui.insert(Fill('C', Size::new(5.0, 1.0)));
                (seen, wheel)
            })
    };
    render(&mut kernel, &mut context, &build);
    for (phase, x, inside) in [(Down, 4.5, false), (Up, 4.5, false), (Down, 2.5, true)] {
        let (seen, _) = render_inputs(
            &mut kernel,
            &mut context,
            Duration::ZERO,
            [pointer(None, phase, [x, 0.5])],
            &build,
        );
        assert_eq!(seen, inside);
    }
    let (seen, wheel) = render_inputs(
        &mut kernel,
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
fn rounded_clip() {
    let (mut kernel, mut context) = kernel(Size::uniform(4.0));
    let rect = Rect::new(0.0, 0.0, 4.0, 4.0);
    let child = Rect::new(3.0, 3.0, 1.0, 1.0);
    // full radii round every corner, a single radius leaves the bottom right square
    for (radius, square) in [(Corners::all(2.0), false), (Corners::new().top_left(2.0), true)] {
        let build = |ui: Ui<'_>| {
            let mut root = ui.clip(radius).layout(Rects);
            let mut child = root.child().item(child).clip(0.0);
            child.insert(Fill('C', Size::uniform(1.0)));
            child.interact(WATCH)[0].is_some()
        };
        render(&mut kernel, &mut context, &build);
        assert_eq!(
            context.commands,
            [PushClip(rect), PushClip(child), Paint('C', child), PopClip, PopClip]
        );
        for (position, inside) in [([3.1, 3.1], true), ([3.9, 3.9], square)] {
            let seen = render_inputs(
                &mut kernel,
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
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let offset = Cell::new(0.0);
    let build = |ui: Ui<'_>| {
        // scrolling moves the row under a still pointer
        if matches!(ui.input(), Input::Scroll { .. }) {
            offset.set(100.0);
        }
        let mut root = ui.layout(Rects);
        let mut row = root.child().item(Rect::new(offset.get(), 0.0, 100.0, 200.0));
        row.interact(Hover)
    };
    render(&mut kernel, &mut context, &build);
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
        &mut kernel,
        &mut context,
        Duration::ZERO,
        [scroll],
        &build
    ));
    assert!(kernel.has_pending_redraw());
    assert!(render(&mut kernel, &mut context, &build));
    assert!(!kernel.has_pending_redraw());
}

#[test]
fn removed_scroll_owner_keeps_its_sequence() {
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let owner = Cell::new(1100);
    let build = |ui: Ui<'_>| {
        let mut root = ui.layout(Overlay);
        let mut list = root.child_key(Scope::ROOT.key(owner.get()));
        list.insert(Fill('L', Size::uniform(200.0)));
        list.interact(Wheel)
    };
    render(&mut kernel, &mut context, &build);
    let mut scroll = |phase| {
        let input = Input::Scroll {
            position: Point::new(10.0, 10.0),
            delta_x: 0.0,
            delta_y: 1.0,
            continuous: true,
            phase,
            modifiers: Modifiers::NONE,
        };
        render_inputs(&mut kernel, &mut context, Duration::ZERO, [input], &build)
    };
    assert!(scroll(ScrollPhase::Started).is_some());
    // another widget takes the owner's node while the gesture continues
    owner.set(1101);
    assert_eq!(scroll(ScrollPhase::Moved), None);
    assert_eq!(scroll(ScrollPhase::Moved), None);
    assert!(scroll(ScrollPhase::Started).is_some());
}

#[test]
fn focus_within_reads_the_previous_layout() {
    let (mut kernel, mut context) = kernel(Size::uniform(100.0));
    let focus = Cell::new(false);
    let child = Cell::new(None);
    let seen = Cell::new(None);
    let build = |ui: Ui<'_>| {
        let mut list = ui.layout(Rects);
        // before any child is built
        seen.set(list.focus_within());
        let mut field = list.child_key(Scope::ROOT.key(0)).item(Rect::new(0.0, 0.0, 10.0, 10.0));
        field.interact(WATCH);
        child.set(Some(field.id()));
        if focus.get() {
            field.focus();
        }
    };
    render(&mut kernel, &mut context, &build);
    focus.set(true);
    render(&mut kernel, &mut context, &build);
    focus.set(false);
    render(&mut kernel, &mut context, &build);
    assert!(seen.get().is_some() && seen.get() == child.get());
}

#[test]
fn focused_descendant_scrolls_into_view() {
    let (mut kernel, mut context) = kernel(Size::uniform(100.0));
    let offset = Cell::new(0.0);
    let request = Cell::new(None);
    let last = Cell::new(None);
    // a list scrolls its focused field into view once whenever focus moves to it
    let build = |ui: Ui<'_>| {
        let mut list = ui.layout(Rects);
        for index in 0..10 {
            let y = index as f32 * 30.0 - offset.get();
            let mut field = list
                .child_key(Scope::ROOT.key(index))
                .item(Rect::new(0.0, y, 100.0, 30.0));
            field.interact(WATCH);
            if request.get() == Some(index) {
                field.focus();
            }
        }
        let focus = list.focus_within();
        if focus != last.replace(focus)
            && let Some(area) = focus.and_then(|id| list.node(id).area())
        {
            let below = area.y + area.height - 100.0;
            offset.set(offset.get() + below.max(0.0) + area.y.min(0.0));
        }
    };
    // the user scrolls first, which a focus that stays put must not undo
    for (scrolled, index, expected) in [(0.0, 8, 170.0), (0.0, 1, 0.0), (200.0, 2, 60.0)] {
        offset.set(scrolled);
        render(&mut kernel, &mut context, &build);
        assert_eq!(offset.get(), scrolled);
        request.set(Some(index));
        render(&mut kernel, &mut context, &build);
        request.set(None);
        render(&mut kernel, &mut context, &build);
        assert_eq!(offset.get(), expected);
        render(&mut kernel, &mut context, &build);
        let y = index as f32 * 30.0 - expected;
        assert_eq!(
            kernel.area(Scope::ROOT.key(index)),
            Some(Rect::new(0.0, y, 100.0, 30.0))
        );
    }
}

#[test]
fn unmounted_target_cancels_pending_contact() {
    let (mut kernel, mut context) = kernel(Size::uniform(200.0));
    let mounted = Cell::new(true);
    let build = |mut ui: Ui<'_>| {
        let root = ui.interact(WATCH)[0];
        let mut layout = ui.layout(Overlay);
        if mounted.get() {
            let mut child = layout.child();
            child.insert(Fill('B', Size::uniform(200.0)));
            child.interact(WATCH);
        }
        root
    };
    render(&mut kernel, &mut context, &build);
    let mut step = |input| render_inputs(&mut kernel, &mut context, Duration::ZERO, [input], &build);
    assert!(step(pointer(Some(1), Down, [10.0, 10.0])).is_some());
    mounted.set(false);
    step(Input::None);
    // the ancestor does not inherit the contact
    assert_eq!(step(pointer(Some(1), Move, [30.0, 10.0])), None);
}
