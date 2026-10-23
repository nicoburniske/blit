use super::*;

#[test]
fn pending_contact_survives_child_unmount() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let mounted = Cell::new(true);
    let capture = Cell::new(false);
    let child_id = Id::new("child");
    let build = |mut ui: Ui<'_>| {
        let contact = ui.contacts().next();
        if capture.get() {
            assert!(ui.capture(contact.unwrap().id));
        }
        let contact = ui.contacts().next();
        ui.geometry(child_id);
        let mut ui = ui.layout(Overlay);
        if mounted.get() {
            ui.child().id(child_id).build(|mut ui: Ui<'_>| {
                ui.insert(Fill('B', Size::uniform(200.0)));
                ui.interact(Sense::CLICK);
            });
        }
        contact
    };
    render(&mut frame, &mut context, &build);
    let down = render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [pointer(Some(1), Down, [10.0, 10.0])],
        &build,
    )
    .unwrap();
    mounted.set(false);
    render(&mut frame, &mut context, &build);
    capture.set(true);
    let moved = render_inputs(
        &mut frame,
        &mut context,
        Duration::ZERO,
        [pointer(Some(1), Move, [300.0, 10.0])],
        &build,
    )
    .unwrap();
    assert_eq!(moved.id, down.id);
    assert_eq!(moved.phase, Changed);
    assert!(moved.captured);
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
        let hovered = parent.interact(Sense::CLICK).hovered;
        parent
            .layout(Rects)
            .child()
            .item(Rect::new(0.0, 0.0, 200.0, 200.0))
            .id(Id::new("leaf"))
            .interact(Sense::CLICK);
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
fn late_capture_and_reused_source() {
    for phase in [Move, Up] {
        let (mut frame, mut context) = frame(Size::uniform(200.0));
        let claim = Cell::new(false);
        let stale = Cell::new(None);
        let build = |mut ui: Ui<'_>| {
            ui.contacts().count();
            if let Some(id) = stale.get() {
                assert!(!ui.capture(id));
            }
            let mut ui = ui.layout(Overlay);
            let contacts = ui.child().id(Id::new("child")).build(|mut ui: Ui<'_>| {
                ui.insert(Fill('B', Size::uniform(200.0)));
                let contacts = {
                    let mut contacts = ui.contacts();
                    [contacts.next(), contacts.next()]
                };
                for contact in contacts
                    .into_iter()
                    .flatten()
                    .filter(|contact| contact.phase == Started)
                {
                    assert!(ui.capture(contact.id));
                }
                contacts
            });
            if claim.get() {
                let id = ui.contacts().find(|contact| contact.phase != Cancelled).unwrap().id;
                assert!(ui.capture(id));
            }
            contacts
        };
        render(&mut frame, &mut context, &build);
        let first = render_inputs(
            &mut frame,
            &mut context,
            Duration::from_millis(10),
            [pointer(Some(1), Down, [10.0, 10.0])],
            &build,
        )[0]
        .unwrap();
        claim.set(true);
        let before = render_inputs(
            &mut frame,
            &mut context,
            Duration::from_millis(20),
            [pointer(Some(1), phase, [20.0, 10.0])],
            &build,
        )[0]
        .unwrap();
        assert_eq!(before.phase, if phase == Up { Ended } else { Changed });
        claim.set(false);
        if phase == Up {
            stale.set(Some(first.id));
        }
        let next = render_inputs(
            &mut frame,
            &mut context,
            Duration::from_millis(30),
            [if phase == Up {
                pointer(Some(1), Down, [10.0, 10.0])
            } else {
                Input::None
            }],
            &build,
        );
        assert_eq!(next[0].unwrap().id, first.id);
        assert_eq!(next[0].unwrap().phase, Cancelled);
        if phase == Up {
            let fresh = next[1].unwrap();
            assert_eq!(fresh.phase, Started);
            assert_eq!(fresh.source, first.source);
            assert_ne!(fresh.id, first.id);
        }
        let idle = render_inputs(
            &mut frame,
            &mut context,
            Duration::from_millis(40),
            [Input::None],
            &build,
        );
        assert!(idle.into_iter().flatten().all(|contact| contact.phase != Cancelled));
    }
}

#[test]
fn release_and_recapture() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let recapture = Cell::new(false);
    let build = |mut ui: Ui<'_>| {
        let contact = ui.contacts().next();
        if let Some(contact) = contact {
            if recapture.get() {
                assert!(ui.release(contact.id));
            }
            assert!(ui.capture(contact.id));
        }
        ui.contacts().next()
    };
    render(&mut frame, &mut context, &build);
    let down = render_inputs(
        &mut frame,
        &mut context,
        Duration::from_millis(10),
        [pointer(Some(1), Down, [10.0, 10.0])],
        &build,
    )
    .unwrap();
    recapture.set(true);
    let moved = render_inputs(
        &mut frame,
        &mut context,
        Duration::from_millis(20),
        [pointer(Some(1), Move, [20.0, 10.0])],
        &build,
    )
    .unwrap();
    assert_eq!(moved.id, down.id);
    assert_eq!(moved.phase, Changed);
    assert!(moved.captured);
}

#[test]
fn independent_motion_endings() {
    let mut scene = Scene::new(Sense::CLICK | Sense::DRAG);
    scene.step(10, pointer(Some(1), Down, [10.0, 10.0]));
    scene.step(20, pointer(Some(1), Move, [30.0, 10.0]));
    scene.step(30, pointer(Some(2), Down, [10.0, 10.0]));
    let ended = scene.step(40, pointer(Some(1), Up, [30.0, 10.0]));
    assert!(ended.active);
    assert_eq!(ended.drag.unwrap().phase, Ended);
    assert_eq!(scene.step(50, Input::None).drag, None);

    let mut scene = Scene::new(Sense::DRAG | Sense::SCROLL);
    scene.step(10, pointer(Some(1), Down, [10.0, 10.0]));
    scene.step(20, pointer(Some(1), Move, [20.0, 10.0]));
    scene.step(30, pointer(None, Down, [10.0, 10.0]));
    let ended = scene.step(40, pointer(Some(1), Up, [20.0, 10.0]));
    assert!(ended.active);
    assert_eq!(ended.drag.unwrap().phase, Changed);
    assert_eq!(ended.scroll.unwrap().phase, Ended);
    let mouse_end = scene.step(50, pointer(None, Up, [10.0, 10.0]));
    assert_eq!(mouse_end.drag.unwrap().phase, Ended);
    assert_eq!(mouse_end.scroll, None);
}

#[test]
fn raw_observation_preserves_ordinary_cancellation() {
    let (mut frame, mut context) = frame(Size::uniform(200.0));
    let release = Cell::new(false);
    let build = |mut ui: Ui<'_>| {
        let response = ui.interact(Sense::DRAG);
        if release.get() {
            let id = ui.contacts().next().unwrap().id;
            assert!(ui.release(id));
            let physical = ui.contacts().next().unwrap();
            assert!(!physical.captured && physical.phase != Cancelled);
        }
        response
    };
    render(&mut frame, &mut context, &build);
    render_inputs(
        &mut frame,
        &mut context,
        Duration::from_millis(10),
        [pointer(Some(1), Down, [10.0, 10.0])],
        &build,
    );
    release.set(true);
    render_inputs(
        &mut frame,
        &mut context,
        Duration::from_millis(20),
        [pointer(Some(1), Move, [20.0, 10.0])],
        &build,
    );
    release.set(false);
    let cancelled = render_inputs(
        &mut frame,
        &mut context,
        Duration::from_millis(30),
        [Input::None],
        &build,
    );
    assert_eq!(cancelled.drag.unwrap().phase, Cancelled);
    assert!(cancelled.deactivated);
}
