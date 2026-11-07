use blit::{ContactId, ContactSource, InteractCx, MouseButton, Phase, Scalar};

use crate::Context;

/// clicks and press feedback
///
/// a press that moves [`Context::SLOP`] is no longer a click
#[derive(Clone, Copy, Debug)]
pub struct Interact;

/// the response of [`Interact`]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response {
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    pub hovered: bool,
    pub clicked: bool,
    /// the contact for the active press or completed click
    pub contact: Option<ContactId>,
}

impl<C: Context> blit::Interact<C> for Interact {
    /// the press in progress
    type State = Option<ContactId>;
    type Response = Response;

    fn update(&self, press: &mut Option<ContactId>, cx: &mut InteractCx<'_, C::Scalar>) -> Self::Response {
        let was_active = press.is_some();
        let mut active = false;
        let mut clicked = false;
        let mut contact_id = None;
        // claims last one update, so keep claiming while it is still a click
        if let Some(id) = *press
            && let Some(mut contact) = cx.contacts().find(|contact| contact.id == id)
        {
            let x = (contact.position.x - contact.origin.x).to_f32();
            let y = (contact.position.y - contact.origin.y).to_f32();
            let slop = C::SLOP.to_f32();
            // this widget capturing it, like with its own drag, ends the click
            if x * x + y * y < slop * slop && contact.claim() {
                active = !matches!(contact.phase, Phase::Ended | Phase::Cancelled);
                clicked = contact.phase == Phase::Ended && contact.over;
                if active || clicked {
                    contact_id = Some(contact.id);
                }
            }
        }
        // one press at a time, deeper widgets claimed theirs first
        if !active {
            *press = None;
            if let Some(mut contact) = cx.contacts().find(|contact| {
                contact.phase == Phase::Started
                    && contact.free
                    && matches!(
                        contact.source,
                        ContactSource::Touch(_) | ContactSource::Mouse(MouseButton::Primary)
                    )
            }) && contact.claim()
            {
                *press = Some(contact.id);
                active = true;
                contact_id = Some(contact.id);
            }
        }
        Response {
            active,
            activated: active && !was_active,
            deactivated: was_active && !active,
            hovered: cx.hovered(),
            clicked,
            contact: contact_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use blit::{
        Frame, Input, Point,
        TouchPhase::{Cancel, Down, Move, Up},
        Ui,
    };

    use crate::{
        interact,
        test::{TestContext, pointer, step},
    };

    #[test]
    fn presses() {
        for id in [None, Some(1)] {
            let mut frame = Frame::<TestContext>::default();
            let mut last_contact = None;
            let mut press = |input| {
                step(&mut frame, input, |mut ui: Ui<'_, TestContext>| {
                    let press = ui.interact(interact::press());
                    last_contact = press.contact.or(last_contact);
                    (
                        press,
                        last_contact.and_then(|id| ui.contact_position(id)),
                        ui.mouse_position(),
                    )
                })
            };
            assert_eq!(press(Input::None).0.contact, None);
            let (down, position, mouse) = press(pointer(id, Down, 10.0, 10.0));
            assert!(down.active && down.activated && !down.clicked);
            assert!(down.contact.is_some());
            assert_eq!(position, Some(Point::new(10.0, 10.0)));
            assert_eq!(mouse, id.is_none().then_some(Point::new(10.0, 10.0)));
            let (held, _, _) = press(Input::None);
            assert!(held.active && !held.activated);
            assert_eq!(held.contact, down.contact);
            let (moved, _, mouse) = press(pointer(id, Move, 11.0, 10.0));
            assert_eq!(moved.contact, down.contact);
            for phase in [Down, Move, Up] {
                let (held, position, cursor) = press(pointer(Some(2), phase, 100.0, 100.0));
                assert!(held.active && !held.activated && !held.clicked);
                assert_eq!(held.contact, down.contact);
                assert_eq!(position, Some(Point::new(11.0, 10.0)));
                assert_eq!(cursor, mouse);
            }
            let (up, position, _) = press(pointer(id, Up, 12.0, 10.0));
            assert!(up.clicked && up.deactivated && !up.active);
            assert_eq!(up.contact, down.contact);
            assert_eq!(position, Some(Point::new(12.0, 10.0)));
            let (idle, position, _) = press(Input::None);
            assert_eq!(idle.contact, None);
            assert_eq!(position, None);
            // moving past the slop or cancelling ends the press
            for (phase, y) in [(Move, 300.0), (Cancel, 10.0)] {
                press(pointer(id, Down, 10.0, 10.0));
                let (ended, position, _) = press(pointer(id, phase, 10.0, y));
                assert!(ended.deactivated && !ended.active && !ended.clicked);
                assert_eq!(ended.contact, None);
                assert_eq!(position, Some(Point::new(10.0, y)));
                if phase == Move {
                    assert!(!press(pointer(id, Up, 10.0, y)).0.clicked);
                }
                assert_eq!(press(Input::None).1, None);
            }
        }
    }

    #[test]
    fn capture_cancels_click() {
        for ending in [Up, Cancel] {
            let mut frame = Frame::<TestContext>::default();
            let build = |mut ui: Ui<'_, TestContext>| {
                // takes a press once it moves, like a scroll view panning
                let pan = ui.interact(interact::drag());
                let press = ui
                    .layout(())
                    .child()
                    .build(|mut ui: Ui<'_, TestContext>| ui.interact(interact::press()));
                (pan, press)
            };
            let mut next = |input| step(&mut frame, input, build);
            next(Input::None);
            // the button claims the press, hiding it from the ancestor
            let (pan, press) = next(pointer(Some(1), Down, 10.0, 10.0));
            assert!(press.active && !pan.active);
            let (pan, press) = next(pointer(Some(1), Move, 12.0, 10.0));
            assert!(press.active && !pan.active);
            // past the slop the button lets go first, then the ancestor takes it in the same build
            let (pan, press) = next(pointer(Some(1), Move, 30.0, 10.0));
            assert!(pan.activated && press.deactivated && !press.active);
            assert_eq!(press.contact, None);
            assert!(!next(pointer(Some(1), ending, 30.0, 10.0)).1.clicked);
        }
    }
}
