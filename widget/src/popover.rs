use blit::{InteractCx, Scalar, Sides, Target, Ui, Widget, input::Input};

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
    pub struct Popover<'a, C: Context> {
        new(state: &'a mut PopoverState),
        #[bounds(FnOnce(Ui<'_, C>, interact::press::Response, bool))]
        trigger<F = fn(Ui<'_, C>, interact::press::Response, bool)>: F = |_, _, _| (),
        content<W = fn(Ui<'_, C>)>: W = |_| (),
        #[into]
        parent: Target = Target::ROOT,
        target_anchor: Anchor = Anchor::BottomLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        x: C::Scalar = C::Scalar::ZERO,
        y: C::Scalar = C::Scalar::ZERO,
        #[into]
        width: Sizing<C::Scalar> = Sizing::fit(),
        #[into]
        height: Sizing<C::Scalar> = Sizing::fit(),
        hit: Sides<C::Scalar> = Sides::all(C::Scalar::ZERO),
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

impl<C: Context, F, W> Widget<C> for Popover<'_, C, F, W>
where
    F: FnOnce(Ui<'_, C>, interact::press::Response, bool),
    W: Widget<C>,
{
    type Response = Option<W::Response>;
    fn build(self, mut ui: Ui<'_, C>) -> Self::Response {
        let state = self.state;
        let trigger = self.trigger;
        let content = self.content;
        let (backdrop_id, trigger_id, content_id) = (0u8, 1u8, 2u8);
        let press = ui.at(trigger_id).interact(interact::press());
        if self.open_on_hover && press.hovered {
            state.open = true;
        } else if !self.open_on_hover && press.activated {
            state.open = !state.open;
        }
        // requested while closed too so the opening frame knows where the trigger is
        let areas = [trigger_id, content_id].map(|id| ui.at(id).area());
        if state.open {
            let backdrop = ui.at(backdrop_id).interact(interact::press());
            ui.at(backdrop_id).interact(BlockInput);
            let content_hovered = ui.at(content_id).interact(BlockInput);
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

        let mut root = ui.layout(flex::column().align(Align::Start));
        if state.open && self.close != PopoverClose::Manual {
            root.child_id(backdrop_id)
                .parent(self.parent)
                .z_index(1)
                .place(
                    self.parent,
                    absolute::place().width(Sizing::full()).height(Sizing::full()),
                )
                .insert(());
        }
        root.child_id(trigger_id)
            .hit(self.hit)
            .layout(())
            .child()
            .build(|ui: Ui<'_, C>| trigger(ui, press, state.open));
        if !state.open {
            return None;
        }
        // stacks above the backdrop as its later sibling
        Some(
            root.child_id(content_id)
                .parent(self.parent)
                .z_index(1)
                .hit(
                    Sides::new()
                        .top(self.y.max(C::Scalar::ZERO))
                        .right((-self.x).max(C::Scalar::ZERO))
                        .bottom((-self.y).max(C::Scalar::ZERO))
                        .left(self.x.max(C::Scalar::ZERO)),
                )
                .place(
                    Target::PARENT.at(trigger_id),
                    absolute::place()
                        .target_anchor(self.target_anchor)
                        .child_anchor(self.child_anchor)
                        .x(self.x)
                        .y(self.y)
                        .width(self.width)
                        .height(self.height),
                )
                .layout(())
                .child()
                .build(content),
        )
    }
}

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
        Axis, Constraints, FrameInfo, Kernel, Point as InputPoint, Rect, Size,
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
                let content = ui
                    .build(Popover::new(&mut state).width(100.0).height(100.0).content(
                        |mut ui: Ui<'_, TestContext>| ui.interact(interact::scroll().axis(Axis::Horizontal)).delta,
                    ))
                    .unwrap();
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

    fn render(ui: Ui<'_, TestContext>, popover: Popover<'_, TestContext>) {
        ui.build(
            popover
                .trigger(|ui, _, _| {
                    ui.layout(())
                        .child_in(blit::Scope::ROOT, "named trigger")
                        .constrain(Constraints::tight(Size::new(2.0, 1.0)))
                        .insert(())
                })
                .content(|ui: Ui<'_, TestContext>| {
                    ui.layout(())
                        .child_in(blit::Scope::ROOT, "named content")
                        .constrain(Constraints::tight(Size::new(4.0, 3.0)))
                        .insert(())
                }),
        );
    }

    #[test]
    fn popover_uses_root_constraints_and_close_behavior() {
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
        let mut content_area = None;
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
            kernel.build(
                &mut context,
                info,
                Duration::ZERO,
                input,
                |mut ui: Ui<'_, TestContext>| {
                    if let Input::MouseUp { .. } = input {
                        content_area = ui.at(2u8).area();
                    }
                    render(ui, Popover::new(&mut state).width(5.0).height(4.0));
                    assert_eq!(state.open, expected.next().unwrap());
                },
            );
            kernel.layout(&mut context);
            if let Input::MouseUp { .. } = input {
                assert_eq!(
                    kernel.area(Target::ROOT.scoped("named content")),
                    Some(Rect::new(0.0, 1.0, 4.0, 3.0))
                );
                assert_eq!(
                    kernel.area(Target::ROOT.scoped("named trigger")),
                    Some(Rect::new(0.0, 0.0, 2.0, 1.0)),
                );
            }
        }
        assert_eq!(content_area, Some(Rect::new(0.0, 1.0, 5.0, 4.0)));
    }
}
