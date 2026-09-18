use blit::{
    Absolute, Anchor, Input, Interaction, NodeTarget, Platform, Point, Sense, Sides, Sizing, Ui,
    Widget, WidgetId,
};
use blit_layout::single;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Close {
    #[default]
    Click,
    Exit,
    Manual,
}

blit::builder! {
    /// popover placement
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Config {
        new(),
        parent: NodeTarget = NodeTarget::Root,
        target_anchor: Anchor = Anchor::BottomLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        offset: Point = Point::ZERO,
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
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
        id: WidgetId = WidgetId::unique(),
    }
}

pub fn build<P, T, C>(
    mut ui: Ui<'_, P>,
    state: &mut State,
    config: Config,
    trigger: T,
    content: C,
) -> Option<C::Response>
where
    P: Platform,
    T: FnOnce(Ui<'_, P>, Interaction, bool),
    C: Widget<P>,
{
    let trigger_id = state.id.child("popover trigger");
    let interaction = ui.interact(trigger_id, Sense::CLICK);
    if config.open_on_hover && interaction.hovered {
        state.open = true;
    } else if !config.open_on_hover && interaction.activated {
        state.open = !state.open;
    }
    let mut root = ui.layout(single::layout());
    let anchor = {
        let mut trigger_node = root
            .child(single::item())
            .widget_id(trigger_id)
            .layout(single::layout());
        let anchor = trigger_node.id();
        trigger_node
            .child(single::item().grow())
            .build(|ui: Ui<'_, P>| trigger(ui, interaction, state.open));
        anchor
    };
    if !state.open {
        return None;
    }

    let backdrop_id = state.id.child("popover backdrop");
    let content_id = state.id.child("popover content");
    let backdrop = root.interact(backdrop_id, Sense::ALL);
    let content_interaction = root.interact(content_id, Sense::ALL);
    let pointer_inside = content_interaction.hovered
        || root.pointer_position().is_some_and(|position| {
            [trigger_id, content_id]
                .into_iter()
                .filter_map(|id| root.geometry(id))
                .any(|area| area.contains(position))
        });
    let pointer_exited = !pointer_inside
        && matches!(
            root.input(),
            Input::PointerMove { .. } | Input::PointerLeave
        );
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

    let mut popup = root
        .absolute(
            Absolute {
                target: config.parent,
                ..Absolute::at(0.0, 0.0)
            }
            .width(Sizing::grow())
            .height(Sizing::grow()),
        )
        .parent(config.parent)
        .z_index(1)
        .widget_id(state.id)
        .layout(single::layout());
    if config.close != Close::Manual {
        popup
            .absolute(
                Absolute::at(0.0, 0.0)
                    .width(Sizing::grow())
                    .height(Sizing::grow()),
            )
            .widget_id(backdrop_id)
            .insert(());
    }
    Some(
        popup
            .absolute(
                Absolute::attach(config.target_anchor, config.child_anchor)
                    .relative_to(anchor)
                    .offset(config.offset.x, config.offset.y)
                    .width(config.width)
                    .height(config.height),
            )
            .hit(
                Sides::new()
                    .top(config.offset.y.max(0.0))
                    .right((-config.offset.x).max(0.0))
                    .bottom((-config.offset.y).max(0.0))
                    .left(config.offset.x.max(0.0)),
            )
            .widget_id(content_id)
            .layout(single::layout())
            .child(single::item().grow())
            .build(content),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Modifiers, PointerButton, Rect, Size};

    use super::*;

    struct TestPlatform;

    impl Platform for TestPlatform {}

    fn render(ui: Ui<'_, TestPlatform>, state: &mut State, config: Config) {
        build(
            ui,
            state,
            config,
            |ui: Ui<'_, TestPlatform>, _, _| {
                ui.widget_id(WidgetId::new("named trigger"))
                    .layout(single::layout())
                    .child(single::item().fixed(2.0, 1.0))
                    .build(())
            },
            |ui: Ui<'_, TestPlatform>| {
                ui.widget_id(WidgetId::new("named content"))
                    .layout(single::layout())
                    .child(single::item().fixed(4.0, 3.0))
                    .build(())
            },
        );
    }

    #[test]
    fn popover_uses_root_constraints_and_close_behavior() {
        let mut frame = Frame::default();
        let mut platform = TestPlatform;
        let info = FrameInfo::new(Size::uniform(10.0));
        let mut state = State::new();
        let content_id = state.id.child("popover content");

        frame.render(&mut platform, info, |ui: Ui<'_, TestPlatform>| {
            render(ui, &mut state, Config::new());
        });
        let config = Config::new()
            .offset(Point::new(0.0, 1.0))
            .open_on_hover(true)
            .close(Close::Exit);
        let mut expected = [true, true, false].into_iter();
        frame.render_inputs(
            &mut platform,
            info,
            Duration::ZERO,
            [
                Input::PointerMove {
                    position: Point::new(1.0, 0.5),
                    modifiers: Modifiers::NONE,
                },
                Input::PointerMove {
                    position: Point::new(1.0, 1.5),
                    modifiers: Modifiers::NONE,
                },
                Input::PointerMove {
                    position: Point::new(9.0, 9.0),
                    modifiers: Modifiers::NONE,
                },
            ],
            |ui: Ui<'_, TestPlatform>| {
                render(ui, &mut state, config);
                assert_eq!(state.open, expected.next().unwrap());
            },
        );

        let mut expected = [true, true, false].into_iter();
        let mut content_geometry = None;
        frame.render_inputs(
            &mut platform,
            info,
            Duration::ZERO,
            [
                Input::PointerDown {
                    position: Point::new(1.0, 0.5),
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                },
                Input::PointerUp {
                    position: Point::new(1.0, 0.5),
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                    leave: false,
                },
                Input::PointerDown {
                    position: Point::new(9.0, 9.0),
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                },
            ],
            |ui: Ui<'_, TestPlatform>| {
                if matches!(ui.input(), Input::PointerUp { .. }) {
                    content_geometry = ui.geometry(content_id);
                    assert_eq!(
                        ui.geometry(WidgetId::new("named content")),
                        content_geometry,
                    );
                    assert_eq!(
                        ui.geometry(WidgetId::new("named trigger")),
                        Some(Rect::new(0.0, 0.0, 2.0, 1.0)),
                    );
                }
                render(
                    ui,
                    &mut state,
                    Config::new()
                        .width(Sizing::fixed(5.0))
                        .height(Sizing::fixed(4.0)),
                );
                assert_eq!(state.open, expected.next().unwrap());
            },
        );
        assert_eq!(content_geometry, Some(Rect::new(0.0, 1.0, 5.0, 4.0)));
    }
}
