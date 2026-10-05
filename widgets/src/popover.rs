use blit::{Context, Input, Interaction, NodeTarget, Scalar, Sense, Sides, Ui, Widget};
use blit_layout::{
    absolute,
    absolute::{Anchor, Sizing},
    single,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Close {
    #[default]
    Click,
    Exit,
    Manual,
}

blit::builder! {
    /// popover placement with x and y offsets from its anchors
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Config<T: Scalar> {
        new(),
        parent: NodeTarget = NodeTarget::Root,
        target_anchor: Anchor = Anchor::BottomLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        x: T = T::ZERO,
        y: T = T::ZERO,
        width: Sizing<T> = Sizing::fit(),
        height: Sizing<T> = Sizing::fit(),
        open_on_hover: bool = false,
        close: Close = Close::Click,
    }
}

blit::builder! {
    /// persistent popover visibility
    #[derive(Debug)]
    pub struct State {
        new(),
        open: bool = false,
    }
}

pub fn new<'a, C: Context, T, W>(
    state: &'a mut State,
    config: Config<C::Scalar>,
    trigger: T,
    content: W,
) -> impl Widget<C, Response = Option<W::Response>> + 'a
where
    T: FnOnce(Ui<'_, C>, Interaction<C::Scalar>, bool) + 'a,
    W: Widget<C> + 'a,
{
    move |mut ui: Ui<'_, C>| {
        let id = ui.current_widget_id();
        let trigger_id = id.child("popover trigger");
        let interaction = ui.interact_widget(trigger_id, Sense::CLICK);
        if config.open_on_hover && interaction.hovered {
            state.open = true;
        } else if !config.open_on_hover && interaction.activated {
            state.open = !state.open;
        }
        let mut root = ui.layout(single::new());
        let anchor = {
            let mut trigger_node = root.child().widget_id(trigger_id).layout(());
            let anchor = trigger_node.id();
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
        let backdrop = root.interact_widget(backdrop_id, Sense::ALL);
        let content_interaction = root.interact_widget(content_id, Sense::ALL);
        let pointer_inside = content_interaction.hovered
            || root.pointer_position().is_some_and(|position| {
                [trigger_id, content_id]
                    .into_iter()
                    .filter_map(|id| root.geometry(id))
                    .any(|area| area.contains(position))
            });
        if match (config.close, root.input()) {
            (Close::Click, _) => backdrop.activated,
            (Close::Exit, Input::PointerMove { .. } | Input::PointerLeave) => !pointer_inside,
            _ => false,
        } {
            state.open = false;
        }
        if !state.open {
            return None;
        }

        let mut popup = root.child().parent(config.parent).z_index(1).layout(
            absolute::place(())
                .target(config.parent)
                .width(Sizing::full())
                .height(Sizing::full()),
        );
        if config.close != Close::Manual {
            popup
                .child()
                .layout(absolute::place(()).width(Sizing::full()).height(Sizing::full()))
                .widget_id(backdrop_id)
                .insert(());
        }
        Some(
            popup
                .child()
                .hit(
                    Sides::new()
                        .top(config.y.max(C::Scalar::ZERO))
                        .right((-config.x).max(C::Scalar::ZERO))
                        .bottom((-config.y).max(C::Scalar::ZERO))
                        .left(config.x.max(C::Scalar::ZERO)),
                )
                .widget_id(content_id)
                .layout(
                    absolute::place(())
                        .target_anchor(config.target_anchor)
                        .child_anchor(config.child_anchor)
                        .target(anchor.into())
                        .x(config.x)
                        .y(config.y)
                        .width(config.width)
                        .height(config.height),
                )
                .child()
                .build(content),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Modifiers, Point as InputPoint, PointerButton, Rect, Size, WidgetId};

    use super::*;
    use crate::test::TestContext;

    fn render(ui: Ui<'_, TestContext>, state: &mut State, config: Config<f32>) {
        ui.widget_id(WidgetId::new("test popover")).build(new(
            state,
            config,
            |ui: Ui<'_, TestContext>, _, _| {
                ui.widget_id(WidgetId::new("named trigger"))
                    .layout(single::new().fixed(2.0, 1.0))
                    .child()
                    .build(())
            },
            |ui: Ui<'_, TestContext>| {
                ui.widget_id(WidgetId::new("named content"))
                    .layout(single::new().fixed(4.0, 3.0))
                    .child()
                    .build(())
            },
        ));
    }

    #[test]
    fn popover_uses_root_constraints_and_close_behavior() {
        let mut frame = Frame::default();
        let mut context = TestContext;
        let info = FrameInfo::new(Size::uniform(10.0));
        let mut state = State::new();
        let content_id = WidgetId::new("test popover").child("popover content");

        frame.build(
            &mut context,
            info,
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, TestContext>| render(ui, &mut state, Config::new()),
        );
        frame.layout(&mut context);
        let config = Config::new().y(1.0).open_on_hover(true).close(Close::Exit);
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
                render(ui, &mut state, config);
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
                        &mut state,
                        Config::new().width(Sizing::fixed(5.0)).height(Sizing::fixed(4.0)),
                    );
                    assert_eq!(state.open, expected.next().unwrap());
                },
            );
            frame.layout(&mut context);
            if let Input::PointerUp { .. } = input {
                assert_eq!(frame.geometry(WidgetId::new("named content")), content_geometry);
                assert_eq!(
                    frame.geometry(WidgetId::new("named trigger")),
                    Some(Rect::new(0.0, 0.0, 2.0, 1.0)),
                );
            }
        }
        assert_eq!(content_geometry, Some(Rect::new(0.0, 1.0, 5.0, 4.0)));
    }
}
