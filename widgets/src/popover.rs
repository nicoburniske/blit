use blit::{Context, Input, Interaction, NodeTarget, Scalar, Sense, Sides, Ui, Widget};
use blit_layout::{
    absolute,
    absolute::{Anchor, Sizing},
    single,
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
        #[bounds(FnOnce(Ui<'_, C>, Interaction, bool))]
        trigger<F = fn(Ui<'_, C>, Interaction, bool)>: F = |_, _, _| (),
        content<W = fn(Ui<'_, C>)>: W = |_| (),
        #[into]
        parent: NodeTarget = NodeTarget::Root,
        target_anchor: Anchor = Anchor::BottomLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        x: C::Scalar = C::Scalar::ZERO,
        y: C::Scalar = C::Scalar::ZERO,
        width: Sizing<C::Scalar> = Sizing::fit(),
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
    F: FnOnce(Ui<'_, C>, Interaction, bool),
    W: Widget<C>,
{
    type Response = Option<W::Response>;
    fn build(self, mut ui: Ui<'_, C>) -> Self::Response {
        let state = self.state;
        let trigger = self.trigger;
        let content = self.content;
        let id = ui.current_id();
        let trigger_id = id.child("popover trigger");
        let interaction = ui.interact_widget(trigger_id).sense(Sense::CLICK);
        if self.open_on_hover && interaction.hovered {
            state.open = true;
        } else if !self.open_on_hover && interaction.activated {
            state.open = !state.open;
        }
        let mut root = ui.layout(single::new());
        let anchor = {
            let mut trigger_node = root.child().id(trigger_id).hit(self.hit).layout(());
            let anchor = trigger_node.node_id();
            trigger_node
                .child()
                .build(|ui: Ui<'_, C>| trigger(ui, interaction, state.open));
            anchor
        };
        if !state.open {
            return None;
        }

        let backdrop_id = id.child("popover backdrop");
        let content_id = id.child("popover content");
        let backdrop = root.interact_widget(backdrop_id).sense(Sense::ALL);
        let content_interaction = root.interact_widget(content_id).sense(Sense::ALL);
        let pointer_inside = content_interaction.hovered
            || root.pointer_position().is_some_and(|position| {
                [trigger_id, content_id]
                    .into_iter()
                    .filter_map(|id| root.geometry(id))
                    .any(|area| area.contains(position))
            });
        if match (self.close, root.interact().input()) {
            (PopoverClose::Click, _) => backdrop.activated,
            (PopoverClose::Exit, Input::PointerMove { .. } | Input::PointerLeave) => !pointer_inside,
            _ => false,
        } {
            state.open = false;
        }
        if !state.open {
            return None;
        }

        let mut popup = root.child().parent(self.parent).z_index(1).layout(
            absolute::place(())
                .target(self.parent)
                .width(Sizing::full())
                .height(Sizing::full()),
        );
        if self.close != PopoverClose::Manual {
            popup
                .child()
                .layout(absolute::place(()).width(Sizing::full()).height(Sizing::full()))
                .id(backdrop_id)
                .insert(());
        }
        Some(
            popup
                .child()
                .hit(
                    Sides::new()
                        .top(self.y.max(C::Scalar::ZERO))
                        .right((-self.x).max(C::Scalar::ZERO))
                        .bottom((-self.y).max(C::Scalar::ZERO))
                        .left(self.x.max(C::Scalar::ZERO)),
                )
                .id(content_id)
                .layout(
                    absolute::place(())
                        .target_anchor(self.target_anchor)
                        .child_anchor(self.child_anchor)
                        .target(anchor)
                        .x(self.x)
                        .y(self.y)
                        .width(self.width)
                        .height(self.height),
                )
                .child()
                .build(content),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Id, Modifiers, Point as InputPoint, PointerButton, Rect, Size};

    use super::*;
    use crate::test::TestContext;

    fn render(ui: Ui<'_, TestContext>, popover: Popover<'_, TestContext>) {
        ui.id(Id::new("test popover")).build(
            popover
                .trigger(|ui, _, _| {
                    ui.id(Id::new("named trigger"))
                        .layout(single::new().fixed(2.0, 1.0))
                        .child()
                        .insert(())
                })
                .content(|ui: Ui<'_, TestContext>| {
                    ui.id(Id::new("named content"))
                        .layout(single::new().fixed(4.0, 3.0))
                        .child()
                        .insert(())
                }),
        );
    }

    #[test]
    fn popover_uses_root_constraints_and_close_behavior() {
        let mut frame = Frame::default();
        let mut context = TestContext;
        let info = FrameInfo::new(Size::uniform(10.0));
        let mut state = PopoverState::new();
        let content_id = Id::new("test popover").child("popover content");

        frame.build(
            &mut context,
            info,
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, TestContext>| render(ui, Popover::new(&mut state)),
        );
        frame.layout(&mut context);
        let mut expected = [true, true, false].into_iter();
        for input in [
            Input::PointerMove {
                position: InputPoint::new(1.0, 0.5),
                modifiers: Modifiers::NONE,
            },
            Input::PointerMove {
                position: InputPoint::new(1.0, 1.5),
                modifiers: Modifiers::NONE,
            },
            Input::PointerMove {
                position: InputPoint::new(9.0, 9.0),
                modifiers: Modifiers::NONE,
            },
        ] {
            frame.build(&mut context, info, Duration::ZERO, input, |ui: Ui<'_, TestContext>| {
                render(
                    ui,
                    Popover::new(&mut state)
                        .y(1.0)
                        .open_on_hover(true)
                        .close(PopoverClose::Exit),
                );
                assert_eq!(state.open, expected.next().unwrap());
            });
            frame.layout(&mut context);
        }

        let mut expected = [true, true, false].into_iter();
        let mut content_geometry = None;
        for input in [
            Input::PointerDown {
                position: InputPoint::new(1.0, 0.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
            Input::PointerUp {
                position: InputPoint::new(1.0, 0.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
            Input::PointerDown {
                position: InputPoint::new(9.0, 9.0),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
        ] {
            frame.build(
                &mut context,
                info,
                Duration::ZERO,
                input,
                |mut ui: Ui<'_, TestContext>| {
                    if let Input::PointerUp { .. } = input {
                        content_geometry = ui.geometry(content_id);
                    }
                    render(
                        ui,
                        Popover::new(&mut state)
                            .width(Sizing::fixed(5.0))
                            .height(Sizing::fixed(4.0)),
                    );
                    assert_eq!(state.open, expected.next().unwrap());
                },
            );
            frame.layout(&mut context);
            if let Input::PointerUp { .. } = input {
                assert_eq!(frame.geometry(Id::new("named content")), content_geometry);
                assert_eq!(
                    frame.geometry(Id::new("named trigger")),
                    Some(Rect::new(0.0, 0.0, 2.0, 1.0)),
                );
            }
        }
        assert_eq!(content_geometry, Some(Rect::new(0.0, 1.0, 5.0, 4.0)));
    }
}
