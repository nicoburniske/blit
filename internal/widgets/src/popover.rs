use blit::{Input, Interaction, NodeTarget, Sense, Sides, Ui, Widget};
use blit_layout::{
    Sizing, Unit,
    absolute::{self, Anchor},
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
    #[const]
    /// popover placement
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Config<U: Unit> {
        new(),
        parent: NodeTarget = NodeTarget::Root,
        target_anchor: Anchor = Anchor::BottomLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        offset: blit::Point<U::Offset> = blit::Point::new(U::Offset::ZERO, U::Offset::ZERO),
        width: Sizing<U> = Sizing::fit(),
        height: Sizing<U> = Sizing::fit(),
        open_on_hover: bool = false,
        close: Close = Close::Click,
    }
}

blit::builder! {
    #[const]
    /// persistent popover visibility
    #[derive(Debug)]
    pub struct State {
        new(),
        open: bool = false,
    }
}

pub fn new<'a, C, U: Unit, T, W>(
    state: &'a mut State,
    config: Config<U>,
    trigger: T,
    content: W,
) -> impl Widget<C, Response = Option<W::Response>> + 'a
where
    T: FnOnce(Ui<'_, C>, Interaction, bool) + 'a,
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
        let mut root = ui.layout(single::Layout::<U>::new());
        let anchor = {
            let mut trigger_node = root.child().widget_id(trigger_id).layout(single::Layout::<U>::new());
            let anchor = trigger_node.id();
            trigger_node
                .child()
                .item(single::Item::new().grow())
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
        let pointer_exited = !pointer_inside && matches!(root.input(), Input::PointerMove { .. } | Input::PointerLeave);
        if match config.close {
            Close::Click => backdrop.activated,
            Close::Exit => pointer_exited,
            Close::Manual => false,
        } {
            state.open = false;
        }
        if !state.open {
            return None;
        }

        let mut overlay = root
            .target(config.parent)
            .visual_parent(config.parent)
            .z_index(1)
            .layout(
                absolute::Layout::<_, U>::new(single::Layout::<U>::new())
                    .width(Sizing::grow())
                    .height(Sizing::grow()),
            );
        if config.close != Close::Manual {
            overlay
                .child()
                .item(single::Item::new().grow())
                .widget_id(backdrop_id)
                .insert(());
        }
        let offset_x = config.offset.x.into_float();
        let offset_y = config.offset.y.into_float();
        Some(
            overlay
                .target(anchor)
                .layout(
                    absolute::Layout::<_, U>::new(single::Layout::<U>::new())
                        .target_anchor(config.target_anchor)
                        .child_anchor(config.child_anchor)
                        .x(config.offset.x)
                        .y(config.offset.y)
                        .width(config.width)
                        .height(config.height),
                )
                .hit(
                    Sides::new()
                        .top(offset_y.max(0.0))
                        .right((-offset_x).max(0.0))
                        .bottom((-offset_y).max(0.0))
                        .left(offset_x.max(0.0)),
                )
                .widget_id(content_id)
                .child()
                .item(single::Item::new().grow())
                .build(content),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, LogicalPoint, LogicalRect, LogicalSize, Modifiers, PointerButton, WidgetId};

    use super::*;
    use crate::test::TestContext;

    fn render(ui: Ui<'_, TestContext>, state: &mut State, config: Config<f32>) {
        ui.widget_id(WidgetId::new("test popover")).build(new(
            state,
            config,
            |ui: Ui<'_, TestContext>, _, _| {
                ui.widget_id(WidgetId::new("named trigger"))
                    .layout(single::Layout::<f32>::new())
                    .child()
                    .item(single::Item::new().fixed(2.0, 1.0))
                    .build(())
            },
            |ui: Ui<'_, TestContext>| {
                ui.widget_id(WidgetId::new("named content"))
                    .layout(single::Layout::<f32>::new())
                    .child()
                    .item(single::Item::new().fixed(4.0, 3.0))
                    .build(())
            },
        ));
    }

    #[test]
    fn popover_uses_root_constraints_and_close_behavior() {
        let mut frame = Frame::default();
        let mut context = TestContext;
        let info = FrameInfo::new(LogicalSize::uniform(10.0));
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
        let config = Config::new()
            .offset(LogicalPoint::new(0.0, 1.0))
            .open_on_hover(true)
            .close(Close::Exit);
        let mut expected = [true, true, false].into_iter();
        for input in [
            Input::PointerMove {
                position: LogicalPoint::new(1.0, 0.5),
                modifiers: Modifiers::NONE,
            },
            Input::PointerMove {
                position: LogicalPoint::new(1.0, 1.5),
                modifiers: Modifiers::NONE,
            },
            Input::PointerMove {
                position: LogicalPoint::new(9.0, 9.0),
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
                position: LogicalPoint::new(1.0, 0.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
            Input::PointerUp {
                position: LogicalPoint::new(1.0, 0.5),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
            Input::PointerDown {
                position: LogicalPoint::new(9.0, 9.0),
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
        ] {
            let inspect = matches!(input, Input::PointerUp { .. });
            frame.build(
                &mut context,
                info,
                Duration::ZERO,
                input,
                |mut ui: Ui<'_, TestContext>| {
                    if inspect {
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
            if inspect {
                assert_eq!(frame.geometry(WidgetId::new("named content")), content_geometry);
                assert_eq!(
                    frame.geometry(WidgetId::new("named trigger")),
                    Some(LogicalRect::new(0.0, 0.0, 2.0, 1.0)),
                );
            }
        }
        assert_eq!(content_geometry, Some(LogicalRect::new(0.0, 1.0, 5.0, 4.0)));
    }
}
