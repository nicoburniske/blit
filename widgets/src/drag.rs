use blit::{Axis, ContactId, ContactSource, Interact, InteractCx, Phase, Point, PointerButton, Scalar};

/// follows one primary press or touch once it moves past the drag threshold
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    axis: Option<Axis>,
    touch: bool,
    threshold: f32,
    contact: Option<ContactId>,
}

/// the response of [`Drag`]
///
/// `activated` implies `active` unless `deactivated`, and `released` implies `deactivated`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DragResponse<T> {
    /// a contact is being dragged
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    /// the drag ended by letting go rather than by cancellation or its widget disappearing
    pub released: bool,
    pub position: Point<T>,
    /// movement since the previous update, or since the press when activated
    pub delta: Point<T>,
}

impl Drag {
    pub const fn new() -> Self {
        Self {
            axis: None,
            touch: false,
            threshold: 6.0,
            contact: None,
        }
    }

    /// starts only when movement is mostly along `axis`
    pub const fn axis(self, axis: Axis) -> Self {
        Self {
            axis: Some(axis),
            ..self
        }
    }

    /// ignores the mouse when set, since it scrolls with its wheel instead
    pub const fn touch(self, touch: bool) -> Self {
        Self { touch, ..self }
    }

    /// movement in frame coordinates before the drag starts
    pub const fn threshold(self, threshold: f32) -> Self {
        Self { threshold, ..self }
    }
}

impl Default for Drag {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Scalar> Interact<T> for Drag {
    type Response = DragResponse<T>;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> Self::Response {
        if let Some(id) = self.contact {
            let contact = cx.contacts().find(|contact| contact.id == id);
            let active = contact.is_some_and(|contact| !matches!(contact.phase, Phase::Ended | Phase::Cancelled));
            if !active {
                self.contact = None;
            }
            // a contact that vanished with its widget ends the drag without a release
            return DragResponse {
                active,
                activated: false,
                deactivated: !active,
                released: contact.is_some_and(|contact| contact.phase == Phase::Ended),
                position: contact.map_or(Point::ZERO, |contact| contact.position),
                delta: contact.map_or(Point::ZERO, |contact| contact.delta),
            };
        }
        let contact = cx.contacts().find(|contact| {
            let x = (contact.position.x - contact.origin.x).to_f32();
            let y = (contact.position.y - contact.origin.y).to_f32();
            let distance = x * x + y * y;
            matches!(contact.phase, Phase::Changed | Phase::Ended)
                && match contact.source {
                    ContactSource::Touch => true,
                    ContactSource::Mouse(button) => !self.touch && button == PointerButton::Primary,
                }
                && distance > 0.0
                && distance >= self.threshold * self.threshold
                && self
                    .axis
                    .is_none_or(|axis| (axis == Axis::Horizontal) == (x.abs() >= y.abs()))
        });
        let Some(contact) = contact.filter(|contact| cx.capture(contact.id)) else {
            return DragResponse::default();
        };
        // a release past the threshold starts and ends in one update
        let active = contact.phase == Phase::Changed;
        if active {
            self.contact = Some(contact.id);
        }
        DragResponse {
            active,
            activated: true,
            deactivated: !active,
            released: !active,
            position: contact.position,
            delta: Point::new(
                contact.position.x - contact.origin.x,
                contact.position.y - contact.origin.y,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, time::Duration};

    use blit::{Frame, FrameInfo, Id, Input, Size, TouchPhase, Ui};
    use blit_layout::single;

    use super::*;
    use crate::test::TestContext;

    type States = (DragResponse<f32>, DragResponse<f32>);

    /// a horizontal slider inside a vertical touch pan
    fn step(frame: &mut Frame<TestContext>, input: Input<f32>) -> States {
        let output = frame.build(
            &mut TestContext,
            FrameInfo::new(Size::uniform(100.0)),
            Duration::ZERO,
            input,
            |mut ui: Ui<'_, TestContext>| {
                let pan = ui.interact(Drag::new().axis(Axis::Vertical).touch(true));
                let slider = ui
                    .layout(single::new().grow())
                    .child()
                    .build(|mut ui: Ui<'_, TestContext>| ui.interact(Drag::new().axis(Axis::Horizontal)));
                (pan, slider)
            },
        );
        frame.layout(&mut TestContext);
        output
    }

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> Input<f32> {
        Input::Touch {
            id,
            position: Point::new(x, y),
            phase,
        }
    }

    fn dragging(x: f32, y: f32, dx: f32, dy: f32) -> DragResponse<f32> {
        DragResponse {
            active: true,
            position: Point::new(x, y),
            delta: Point::new(dx, dy),
            ..DragResponse::default()
        }
    }

    fn let_go(x: f32, y: f32, dx: f32, dy: f32) -> DragResponse<f32> {
        DragResponse {
            active: false,
            deactivated: true,
            released: true,
            ..dragging(x, y, dx, dy)
        }
    }

    #[test]
    fn drags_pick_contacts_by_axis() {
        let idle = DragResponse::default();
        let mut frame = Frame::default();
        step(&mut frame, Input::None);
        step(&mut frame, touch(1, TouchPhase::Down, 10.0, 10.0));
        assert_eq!(step(&mut frame, touch(1, TouchPhase::Move, 12.0, 10.0)), (idle, idle));
        let started = step(&mut frame, touch(1, TouchPhase::Move, 30.0, 10.0));
        let activated = DragResponse {
            activated: true,
            ..dragging(30.0, 10.0, 20.0, 0.0)
        };
        assert_eq!(started, (idle, activated));
        // a held drag stays active and ignores a second contact
        let held = dragging(30.0, 10.0, 0.0, 0.0);
        assert_eq!(step(&mut frame, touch(2, TouchPhase::Down, 50.0, 50.0)), (idle, held));
        assert_eq!(step(&mut frame, touch(2, TouchPhase::Move, 80.0, 50.0)), (idle, held));
        let ended = step(&mut frame, touch(1, TouchPhase::Up, 35.0, 10.0));
        assert_eq!(ended, (idle, let_go(35.0, 10.0, 5.0, 0.0)));
        // once free, the slider takes the other contact on its next move
        let taken = step(&mut frame, touch(2, TouchPhase::Move, 90.0, 50.0));
        assert!(taken.1.activated && taken.1.active && taken.1.delta == Point::new(40.0, 0.0));
        step(&mut frame, touch(2, TouchPhase::Up, 90.0, 50.0));

        // vertical movement goes to the pan and the slider never takes it later
        step(&mut frame, touch(3, TouchPhase::Down, 10.0, 10.0));
        let panned = step(&mut frame, touch(3, TouchPhase::Move, 10.0, 30.0));
        assert!(panned.0.activated && panned.1 == idle);
        let sideways = step(&mut frame, touch(3, TouchPhase::Move, 60.0, 30.0));
        assert_eq!(sideways, (dragging(60.0, 30.0, 50.0, 0.0), idle));
        let cancelled = step(&mut frame, Input::Cancel);
        let cancelled_state = DragResponse {
            released: false,
            ..let_go(60.0, 30.0, 0.0, 0.0)
        };
        assert_eq!(cancelled, (cancelled_state, idle));
    }

    #[test]
    fn removing_the_dragged_widget_cancels() {
        let mut frame = Frame::<TestContext>::default();
        let mounted = Cell::new(true);
        let mut step = |input| {
            let output = frame.build(
                &mut TestContext,
                FrameInfo::new(Size::uniform(100.0)),
                Duration::ZERO,
                input,
                |mut ui: Ui<'_, TestContext>| {
                    let handle = Id::new("handle");
                    let drag = ui.interact_widget(handle, Drag::new());
                    if mounted.get() {
                        ui.layout(single::new().grow())
                            .child()
                            .id(handle)
                            .build(|_: Ui<'_, TestContext>| {});
                    }
                    drag
                },
            );
            frame.layout(&mut TestContext);
            output
        };
        step(Input::None);
        step(touch(1, TouchPhase::Down, 10.0, 10.0));
        assert!(step(touch(1, TouchPhase::Move, 40.0, 10.0)).active);
        mounted.set(false);
        step(touch(1, TouchPhase::Move, 50.0, 10.0));
        let removed = step(Input::None);
        assert!(removed.deactivated && !removed.released && !removed.active);
    }
}
