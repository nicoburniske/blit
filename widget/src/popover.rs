use blit::{
    Id, InteractCx, Scalar, Scope, Sides, Ui,
    input::Input,
    state::{NoLayout, Open},
};

use crate::{
    Context, interact,
    layout::{
        Align, absolute,
        absolute::{Anchor, Sizing},
        flex,
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverClose {
    #[default]
    Click,
    Exit,
    Manual,
}

blit::builder! {
    pub struct Popover<'a, T: Scalar> {
        new(state: &'a mut PopoverState),
        parent: Id = Id::ROOT,
        target_anchor: Anchor = Anchor::BottomLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        x: T = T::ZERO,
        y: T = T::ZERO,
        #[into]
        width: Sizing<T> = Sizing::fit(),
        #[into]
        height: Sizing<T> = Sizing::fit(),
        hit: Sides<T> = Sides::all(T::ZERO),
        open_on_hover: bool = false,
        close: PopoverClose = PopoverClose::Click,
    }
}

blit::builder! {
    /// persistent popover visibility
    #[derive(Debug)]
    pub struct PopoverState {
        new(),
        open: bool = false,
    }
}

impl<T: Scalar> Popover<'_, T> {
    pub fn begin<'a, C, S>(self, mut ui: Ui<'a, C, S>) -> PopoverTrigger<'a, C>
    where
        C: Context<Scalar = T>,
        S: NoLayout,
    {
        let state = self.state;
        let scope = ui.scope();
        let trigger_id = ui.resolve(scope, TRIGGER);
        let content_id = ui.resolve(scope, CONTENT);
        let (press, trigger_area) = {
            let mut part = ui.node(trigger_id);
            (part.interact(interact::press()), part.area())
        };
        if self.open_on_hover && press.hovered {
            state.open = true;
        } else if !self.open_on_hover && press.activated {
            state.open = !state.open;
        }
        // requested while closed too so the opening frame knows where the trigger is
        let areas = [trigger_area, ui.node(content_id).area()];
        if state.open {
            let backdrop_id = ui.resolve(scope, BACKDROP);
            let backdrop = {
                let mut part = ui.node(backdrop_id);
                let backdrop = part.interact(interact::press());
                part.interact(BlockInput);
                backdrop
            };
            let content_hovered = ui.node(content_id).interact(BlockInput);
            let pointer_inside = content_hovered
                || ui
                    .mouse_position()
                    .is_some_and(|position| areas.into_iter().flatten().any(|area| area.contains(position)));
            if match (self.close, ui.input()) {
                (PopoverClose::Click, _) => backdrop.activated,
                (PopoverClose::Exit, Input::MouseMove { .. } | Input::MouseLeave) => !pointer_inside,
                _ => false,
            } {
                state.open = false;
            }
        }
        let open = state.open;
        let mut root = ui.layout(flex::column().align(Align::Start));
        if open && self.close != PopoverClose::Manual {
            root.child_in(scope, BACKDROP)
                .parent(self.parent)
                .z_index(1)
                .place(
                    self.parent,
                    absolute::place().width(Sizing::full()).height(Sizing::full()),
                )
                .insert(());
        }
        PopoverTrigger {
            press,
            hit: self.hit,
            next: PopoverContent {
                root,
                scope,
                trigger_id,
                open,
                parent: self.parent,
                hit: Sides::new()
                    .top(self.y.max(T::ZERO))
                    .right((-self.x).max(T::ZERO))
                    .bottom((-self.y).max(T::ZERO))
                    .left(self.x.max(T::ZERO)),
                place: absolute::place()
                    .target_anchor(self.target_anchor)
                    .child_anchor(self.child_anchor)
                    .x(self.x)
                    .y(self.y)
                    .width(self.width)
                    .height(self.height),
            },
        }
    }
}

#[must_use]
pub struct PopoverTrigger<'a, C: Context> {
    press: interact::press::Response,
    hit: Sides<C::Scalar>,
    next: PopoverContent<'a, C>,
}

impl<'a, C: Context> PopoverTrigger<'a, C> {
    pub fn trigger(
        mut self,
        trigger: impl FnOnce(Ui<'_, C>, interact::press::Response, bool),
    ) -> PopoverContent<'a, C> {
        let press = self.press;
        let open = self.next.open;
        self.next
            .root
            .child_in(self.next.scope, TRIGGER)
            .hit(self.hit)
            .layout(())
            .child()
            .build(|ui: Ui<'_, C>| trigger(ui, press, open));
        self.next
    }
}

#[must_use]
pub struct PopoverContent<'a, C: Context> {
    root: Ui<'a, C, Open<flex::Layout<C::Scalar>>>,
    scope: Scope,
    trigger_id: Id,
    open: bool,
    parent: Id,
    hit: Sides<C::Scalar>,
    place: absolute::Place<C::Scalar>,
}

impl<C: Context> PopoverContent<'_, C> {
    pub fn content(mut self, content: impl FnOnce(Ui<'_, C>)) {
        if !self.open {
            return;
        }
        // stacks above the backdrop as its later sibling
        self.root
            .child_in(self.scope, CONTENT)
            .parent(self.parent)
            .z_index(1)
            .hit(self.hit)
            .place(self.trigger_id, self.place)
            .layout(())
            .child()
            .build(content);
    }
}

const BACKDROP: u8 = 0;
const TRIGGER: u8 = 1;
const CONTENT: u8 = 2;

#[derive(Clone, Copy)]
struct BlockInput;

impl<C: Context> blit::Interact<C> for BlockInput {
    type State = ();
    type Response = bool;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, C::Scalar>) -> bool {
        for mut contact in cx.contacts() {
            contact.capture();
        }
        cx.capture_scroll();
        cx.hovered()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{
        Axis, Constraints, FrameInfo, Kernel, Point as InputPoint, Rect, Scope, Size,
        input::{Modifiers, MouseButton, ScrollPhase},
    };

    use super::*;
    use crate::{
        interact,
        test::{TestContext, step},
    };

    #[test]
    fn popover_blocks_background_scroll() {
        let mut kernel = Kernel::default();
        let mut state = PopoverState::new().open(true);
        let mut next = |input| {
            step(&mut kernel, input, |mut ui: Ui<'_, TestContext>| {
                let background = ui.interact(interact::scroll()).delta;
                let mut content = InputPoint::ZERO;
                Popover::new(&mut state)
                    .width(100.0)
                    .height(100.0)
                    .begin(ui)
                    .trigger(|_, _, _| ())
                    .content(|mut ui| content = ui.interact(interact::scroll().axis(Axis::Horizontal)).delta);
                (background, content)
            })
        };
        next(Input::None);
        for (position, delta) in [
            (InputPoint::new(150.0, 150.0), InputPoint::new(0.0, 5.0)),
            (InputPoint::new(20.0, 20.0), InputPoint::new(0.0, 5.0)),
            (InputPoint::new(20.0, 20.0), InputPoint::new(5.0, 0.0)),
        ] {
            let (background, content) = next(Input::Scroll {
                position,
                delta_x: delta.x,
                delta_y: delta.y,
                continuous: false,
                phase: ScrollPhase::Moved,
                modifiers: Modifiers::NONE,
            });
            assert_eq!(background, InputPoint::ZERO);
            assert_eq!(content, InputPoint::new(delta.x, 0.0));
        }
    }

    // returns the popover's node holding the content
    fn render(ui: Ui<'_, TestContext>, popover: Popover<'_, f32>) -> Option<Id> {
        let named_trigger = 1000;
        let named_content = 1001;
        let mut part = None;
        popover
            .begin(ui)
            .trigger(|ui, _, _| {
                ui.layout(())
                    .child_in(Scope::ROOT, named_trigger)
                    .constrain(Constraints::tight(Size::new(2.0, 1.0)))
                    .insert(())
            })
            .content(|ui| {
                part = Some(ui.parent_id());
                ui.layout(())
                    .child_in(Scope::ROOT, named_content)
                    .constrain(Constraints::tight(Size::new(4.0, 3.0)))
                    .insert(())
            });
        part
    }

    #[test]
    fn popover_uses_root_constraints_and_close_behavior() {
        let named_trigger = 1000;
        let named_content = 1001;
        let mut kernel = Kernel::default();
        let mut context = TestContext;
        let info = FrameInfo::new(Size::uniform(10.0));
        let mut state = PopoverState::new();

        kernel.build(
            &mut context,
            info,
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, TestContext>| render(ui, Popover::new(&mut state)),
        );
        kernel.layout(&mut context);
        let mut expected = [true, true, false].into_iter();
        for input in [
            Input::MouseMove {
                position: InputPoint::new(1.0, 0.5),
                modifiers: Modifiers::NONE,
            },
            Input::MouseMove {
                position: InputPoint::new(1.0, 1.5),
                modifiers: Modifiers::NONE,
            },
            Input::MouseMove {
                position: InputPoint::new(9.0, 9.0),
                modifiers: Modifiers::NONE,
            },
        ] {
            kernel.build(&mut context, info, Duration::ZERO, input, |ui: Ui<'_, TestContext>| {
                render(
                    ui,
                    Popover::new(&mut state)
                        .y(1.0)
                        .open_on_hover(true)
                        .close(PopoverClose::Exit),
                );
                assert_eq!(state.open, expected.next().unwrap());
            });
            kernel.layout(&mut context);
        }

        let mut expected = [true, true, false].into_iter();
        let (mut part, mut content_area) = (None, None);
        for input in [
            Input::MouseDown {
                position: InputPoint::new(1.0, 0.5),
                button: MouseButton::Primary,
                modifiers: Modifiers::NONE,
            },
            Input::MouseUp {
                position: InputPoint::new(1.0, 0.5),
                button: MouseButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
            Input::MouseDown {
                position: InputPoint::new(9.0, 9.0),
                button: MouseButton::Primary,
                modifiers: Modifiers::NONE,
            },
        ] {
            let released = matches!(input, Input::MouseUp { .. });
            kernel.build(&mut context, info, Duration::ZERO, input, |ui: Ui<'_, TestContext>| {
                part = render(ui, Popover::new(&mut state).width(5.0).height(4.0));
                assert_eq!(state.open, expected.next().unwrap());
            });
            kernel.layout(&mut context);
            if released {
                content_area = part.and_then(|id| kernel.area(id));
                assert_eq!(
                    kernel.find(Scope::ROOT, named_content).and_then(|id| kernel.area(id)),
                    Some(Rect::new(0.0, 1.0, 4.0, 3.0))
                );
                assert_eq!(
                    kernel.find(Scope::ROOT, named_trigger).and_then(|id| kernel.area(id)),
                    Some(Rect::new(0.0, 0.0, 2.0, 1.0)),
                );
            }
        }
        assert_eq!(content_area, Some(Rect::new(0.0, 1.0, 5.0, 4.0)));
    }
}
