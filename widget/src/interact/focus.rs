use blit::{ContactSource, Context, InteractCx, Phase, input::MouseButton};

/// takes focus when pressed or tapped, returning whether this widget is focused
#[derive(Clone, Copy, Debug)]
pub struct Interact;

impl<C: Context> blit::Interact<C> for Interact {
    type State = ();
    type Response = bool;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, C::Scalar>) -> bool {
        if cx.contacts().any(|contact| {
            contact.free
                && match contact.source {
                    ContactSource::Mouse(MouseButton::Primary) => contact.phase == Phase::Started,
                    ContactSource::Touch(_) => contact.phase == Phase::Ended,
                    ContactSource::Mouse(_) => false,
                }
        }) {
            cx.focus();
        }
        cx.focused()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use blit::{
        Kernel, Rect, Scope, Ui,
        input::{
            Input,
            TouchPhase::{Down, Move, Up},
        },
    };

    use crate::{
        interact,
        layout::rect,
        test::{TestContext, pointer, step},
    };

    #[test]
    fn focus_follows_presses_and_taps() {
        let left = 1000;
        let right = 1001;
        let mut kernel = Kernel::<TestContext>::default();
        let change_focus = Cell::new(false);
        let clear_focus = Cell::new(false);
        let build = |mut ui: Ui<'_, TestContext>| {
            if clear_focus.replace(false) {
                ui.clear_focus();
            }
            let mut ui = ui.layout(rect::new());
            let mut left = ui.child_in(Scope::ROOT, left).item(Rect::new(0.0, 0.0, 100.0, 200.0));
            let left_focused = left.interact(interact::focus());
            if change_focus.replace(false) {
                left.node(Scope::ROOT.key(right)).focus();
            }
            drop(left);
            let mut right = ui
                .child_in(Scope::ROOT, right)
                .item(Rect::new(100.0, 0.0, 100.0, 200.0));
            (left_focused, right.interact(interact::focus()))
        };
        let mut next = |input| step(&mut kernel, input, build);
        next(Input::None);
        assert_eq!(next(pointer(None, Down, 10.0, 10.0)), (true, false));
        next(pointer(None, Up, 10.0, 10.0));
        // focus set during a build shows from the next one
        change_focus.set(true);
        assert_eq!(next(Input::None), (true, false));
        assert_eq!(next(Input::None), (false, true));
        // a touch takes focus on release unless something captured it
        clear_focus.set(true);
        next(pointer(Some(1), Down, 10.0, 10.0));
        next(pointer(Some(1), Move, 30.0, 10.0));
        assert_eq!(next(pointer(Some(1), Up, 30.0, 10.0)), (true, false));
        next(pointer(Some(1), Down, 110.0, 10.0));
        assert_eq!(next(pointer(Some(1), Up, 110.0, 10.0)), (false, true));
    }
}
