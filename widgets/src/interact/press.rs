use blit::{ContactId, ContactSource, InteractCx, Phase, PointerButton, Scalar};

use crate::Context;

/// clicks and press feedback
///
/// a press that moves [`Context::SLOP`] is no longer a click
#[derive(Clone, Copy, Debug)]
pub struct Interact;

pub fn new() -> Interact {
    Interact
}

/// the response of [`Interact`]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response {
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    pub hovered: bool,
    pub clicked: bool,
}

impl<C: Context> blit::Interact<C> for Interact {
    /// the press in progress
    type State = Option<ContactId>;
    type Response = Response;

    fn update(&self, press: &mut Option<ContactId>, cx: &mut InteractCx<'_, C::Scalar>) -> Response {
        let was_active = press.is_some();
        let mut active = false;
        let mut clicked = false;
        // claims last one update, so keep claiming while it is still a click
        if let Some(id) = *press
            && let Some(contact) = cx.contacts().find(|contact| contact.id == id)
        {
            let x = (contact.position.x - contact.origin.x).to_f32();
            let y = (contact.position.y - contact.origin.y).to_f32();
            let slop = C::SLOP.to_f32();
            // this widget capturing it, like with its own drag, ends the click
            if x * x + y * y < slop * slop && cx.claim(id) {
                active = !matches!(contact.phase, Phase::Ended | Phase::Cancelled);
                clicked = contact.phase == Phase::Ended && contact.over;
            }
        }
        // one press at a time, deeper widgets claimed theirs first
        if !active {
            *press = cx
                .contacts()
                .find(|contact| {
                    contact.phase == Phase::Started
                        && contact.free
                        && matches!(
                            contact.source,
                            ContactSource::Touch(_) | ContactSource::Mouse(PointerButton::Primary)
                        )
                })
                .map(|contact| contact.id)
                .filter(|id| cx.claim(*id));
            active = press.is_some();
        }
        Response {
            active,
            activated: active && !was_active,
            deactivated: was_active && !active,
            hovered: cx.hovered(),
            clicked,
        }
    }
}

#[cfg(test)]
mod tests {
    use blit::{
        Frame, Input,
        TouchPhase::{Cancel, Down, Move, Up},
        Ui,
    };
    use blit_layout::single;

    use super::*;
    use crate::{
        interact::drag,
        test::{TestContext, pointer, step},
    };

    #[test]
    fn presses() {
        for id in [None, Some(1)] {
            let mut frame = Frame::<TestContext>::default();
            let mut press = |input| step(&mut frame, input, |mut ui: Ui<'_, TestContext>| ui.interact(new()));
            press(Input::None);
            let down = press(pointer(id, Down, 10.0, 10.0));
            assert!(down.active && down.activated && !down.clicked);
            let up = press(pointer(id, Up, 12.0, 10.0));
            assert!(up.clicked && up.deactivated && !up.active);
            // moving past the slop or cancelling is not a click
            for ending in [Up, Cancel] {
                press(pointer(id, Down, 10.0, 10.0));
                let moved = press(pointer(id, Move, 10.0, 300.0));
                assert!(moved.deactivated && !moved.active);
                assert!(!press(pointer(id, ending, 10.0, 10.0)).clicked);
            }
        }
    }

    #[test]
    fn capture_cancels_click() {
        for ending in [Up, Cancel] {
            let mut frame = Frame::<TestContext>::default();
            let build = |mut ui: Ui<'_, TestContext>| {
                // takes a press once it moves, like a scroll view panning
                let pan = ui.interact(drag::new());
                let press = ui
                    .layout(single::new().grow())
                    .child()
                    .build(|mut ui: Ui<'_, TestContext>| ui.interact(new()));
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
            assert!(!next(pointer(Some(1), ending, 30.0, 10.0)).1.clicked);
        }
    }
}
