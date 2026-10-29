use blit::{Axis, ContactId, ContactSource, InteractCx, Phase, Point, PointerButton, Scalar, builder};

use crate::Context;

builder! {
    /// follows one primary press or touch once it moves [`Context::SLOP`]
    ///
    /// with an `axis`, starts only when movement is mostly along it
    #[derive(Clone, Copy, Debug)]
    pub struct Interact {
        new(),
        #[into]
        axis: Option<Axis> = None,
    }
}

pub fn new() -> Interact {
    Interact::new()
}

/// the response of [`Interact`]
///
/// `activated` implies `active` unless `deactivated`, and `released` implies `deactivated`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response<T> {
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

impl<C: Context> blit::Interact<C> for Interact {
    /// the dragged contact
    type State = Option<ContactId>;
    type Response = Response<C::Scalar>;

    fn update(&self, drag: &mut Option<ContactId>, cx: &mut InteractCx<'_, C::Scalar>) -> Self::Response {
        if let Some(id) = *drag {
            let contact = cx.contacts().find(|contact| contact.id == id && contact.free);
            let active = contact.is_some_and(|contact| !matches!(contact.phase, Phase::Ended | Phase::Cancelled));
            if !active {
                *drag = None;
            }
            // a contact this widget lost ends the drag without a release
            return Response {
                active,
                activated: false,
                deactivated: !active,
                released: contact.is_some_and(|contact| contact.phase == Phase::Ended),
                position: contact.map_or(Point::ZERO, |contact| contact.position),
                delta: contact.map_or(Point::ZERO, |contact| contact.delta),
            };
        }
        let slop = C::SLOP.to_f32();
        let contact = cx.contacts().find(|contact| {
            let x = (contact.position.x - contact.origin.x).to_f32();
            let y = (contact.position.y - contact.origin.y).to_f32();
            let distance = x * x + y * y;
            matches!(contact.phase, Phase::Changed | Phase::Ended)
                && contact.free
                && match contact.source {
                    ContactSource::Touch(_) => true,
                    ContactSource::Mouse(button) => button == PointerButton::Primary,
                }
                && distance > 0.0
                && distance >= slop * slop
                && self
                    .axis
                    .is_none_or(|axis| (axis == Axis::Horizontal) == (x.abs() >= y.abs()))
        });
        let Some(contact) = contact.filter(|contact| cx.capture(contact.id)) else {
            return Response::default();
        };
        // a release past the threshold starts and ends in one update
        let active = contact.phase == Phase::Changed;
        if active {
            *drag = Some(contact.id);
        }
        Response {
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
    use std::cell::Cell;

    use blit::{
        Frame, Id, Input,
        TouchPhase::{Down, Move, Up},
        Ui,
    };
    use blit_layout::single;

    use super::*;
    use crate::test::{TestContext, pointer, step};

    /// a horizontal slider inside a vertical pan
    fn slider(mut ui: Ui<'_, TestContext>) -> (Response<f32>, Response<f32>) {
        let pan = ui.interact(new().axis(Axis::Vertical));
        let slider = ui
            .layout(single::new().grow())
            .child()
            .build(|mut ui: Ui<'_, TestContext>| ui.interact(new().axis(Axis::Horizontal)));
        (pan, slider)
    }

    fn dragging(x: f32, y: f32, dx: f32, dy: f32) -> Response<f32> {
        Response {
            active: true,
            position: Point::new(x, y),
            delta: Point::new(dx, dy),
            ..Response::default()
        }
    }

    fn let_go(x: f32, y: f32, dx: f32, dy: f32) -> Response<f32> {
        Response {
            active: false,
            deactivated: true,
            released: true,
            ..dragging(x, y, dx, dy)
        }
    }

    #[test]
    fn drags_pick_contacts_by_axis() {
        let idle = Response::default();
        let mut frame = Frame::default();
        let mut next = |input| step(&mut frame, input, slider);
        next(Input::None);
        next(pointer(Some(1), Down, 10.0, 10.0));
        assert_eq!(next(pointer(Some(1), Move, 12.0, 10.0)), (idle, idle));
        let started = next(pointer(Some(1), Move, 30.0, 10.0));
        let activated = Response {
            activated: true,
            ..dragging(30.0, 10.0, 20.0, 0.0)
        };
        assert_eq!(started, (idle, activated));
        // a held drag stays active and ignores a second contact
        let held = dragging(30.0, 10.0, 0.0, 0.0);
        assert_eq!(next(pointer(Some(2), Down, 50.0, 50.0)), (idle, held));
        assert_eq!(next(pointer(Some(2), Move, 80.0, 50.0)), (idle, held));
        let ended = next(pointer(Some(1), Up, 35.0, 10.0));
        assert_eq!(ended, (idle, let_go(35.0, 10.0, 5.0, 0.0)));
        // once free, the slider takes the other contact on its next move
        let taken = next(pointer(Some(2), Move, 90.0, 50.0));
        assert!(taken.1.activated && taken.1.active && taken.1.delta == Point::new(40.0, 0.0));
        next(pointer(Some(2), Up, 90.0, 50.0));

        // vertical movement goes to the pan and the slider never takes it later
        next(pointer(Some(3), Down, 10.0, 10.0));
        let panned = next(pointer(Some(3), Move, 10.0, 30.0));
        assert!(panned.0.activated && panned.1 == idle);
        let sideways = next(pointer(Some(3), Move, 60.0, 30.0));
        assert_eq!(sideways, (dragging(60.0, 30.0, 50.0, 0.0), idle));
        let cancelled = next(Input::Cancel);
        let cancelled_state = Response {
            released: false,
            ..let_go(60.0, 30.0, 0.0, 0.0)
        };
        assert_eq!(cancelled, (cancelled_state, idle));
    }

    #[test]
    fn removing_the_dragged_widget_cancels() {
        let mut frame = Frame::<TestContext>::default();
        let mounted = Cell::new(true);
        let build = |mut ui: Ui<'_, TestContext>| {
            let handle = Id::new("handle");
            let drag = ui.interact_widget(handle, new());
            if mounted.get() {
                ui.layout(single::new().grow())
                    .child()
                    .id(handle)
                    .build(|_: Ui<'_, TestContext>| {});
            }
            drag
        };
        let mut next = |input| step(&mut frame, input, build);
        next(Input::None);
        next(pointer(Some(1), Down, 10.0, 10.0));
        assert!(next(pointer(Some(1), Move, 40.0, 10.0)).active);
        mounted.set(false);
        next(pointer(Some(1), Move, 50.0, 10.0));
        let removed = next(Input::None);
        assert!(removed.deactivated && !removed.released && !removed.active);
    }
}
