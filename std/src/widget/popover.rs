use std::marker::PhantomData;

use blit::{
    Absolute, Anchor, Input, Interaction, NodeTarget, Platform, Point, Sense, Sides, Sizing, Ui,
    Widget, WidgetId,
};

use crate::layout::single;

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

/// displays content relative to a trigger in the selected visual parent
///
/// the state's id names the popover's visual group
pub struct Popover<'a, R, T = (), C = ()> {
    state: &'a mut State,
    trigger: T,
    content: C,
    config: Config,
    marker: PhantomData<fn() -> R>,
}

impl<'a, R> Popover<'a, R> {
    pub fn new(state: &'a mut State) -> Self {
        Self {
            state,
            trigger: (),
            content: (),
            config: Config::default(),
            marker: PhantomData,
        }
    }
}

impl<'a, R, C> Popover<'a, R, (), C> {
    pub fn trigger<T>(self, trigger: T) -> Popover<'a, R, T, C>
    where
        R: Platform,
        T: FnOnce(Ui<'_, R>, Interaction, bool),
    {
        Popover {
            state: self.state,
            trigger,
            content: self.content,
            config: self.config,
            marker: PhantomData,
        }
    }
}

impl<'a, R, T> Popover<'a, R, T> {
    pub fn build<C>(self, content: C) -> Popover<'a, R, T, C> {
        Popover {
            state: self.state,
            trigger: self.trigger,
            content,
            config: self.config,
            marker: PhantomData,
        }
    }
}

impl<R, T, C> Popover<'_, R, T, C> {
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }
}

impl<R, T, C> Widget<R> for Popover<'_, R, T, C>
where
    R: Platform,
    T: FnOnce(Ui<'_, R>, Interaction, bool),
    C: Widget<R>,
{
    type Response = Option<C::Response>;

    fn build(self, mut ui: Ui<'_, R>) -> Self::Response {
        let trigger_id = self.state.id.child("popover trigger");
        let interaction = ui.interact(trigger_id, Sense::CLICK);
        if self.config.open_on_hover && interaction.hovered {
            self.state.open = true;
        } else if !self.config.open_on_hover && interaction.activated {
            self.state.open = !self.state.open;
        }
        let mut root = ui.layout(single::layout());
        // the wrapper keeps our anchor id separate from the trigger widget's id
        let anchor = {
            let mut trigger = root
                .child(single::item())
                .widget_id(trigger_id)
                .layout(single::layout());
            let anchor = trigger.id();
            trigger.child(single::item().grow()).build(|ui: Ui<'_, R>| {
                (self.trigger)(ui, interaction, self.state.open);
            });
            anchor
        };
        if !self.state.open {
            return None;
        }

        let backdrop_id = self.state.id.child("popover backdrop");
        let content_id = self.state.id.child("popover content");
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
        let close = match self.config.close {
            Close::Click => backdrop.activated,
            Close::Exit => pointer_exited,
            Close::Manual => false,
        };
        if close {
            self.state.open = false;
            return None;
        }

        let mut popup = root
            .absolute(
                Absolute {
                    target: self.config.parent,
                    ..Absolute::at(0.0, 0.0)
                }
                .width(Sizing::grow())
                .height(Sizing::grow()),
            )
            .parent(self.config.parent)
            .z_index(1)
            .widget_id(self.state.id)
            .layout(single::layout());
        if self.config.close != Close::Manual {
            popup
                .absolute(
                    Absolute::at(0.0, 0.0)
                        .width(Sizing::grow())
                        .height(Sizing::grow()),
                )
                .widget_id(backdrop_id)
                .insert(());
        }
        let response = popup
            .absolute(
                Absolute::attach(self.config.target_anchor, self.config.child_anchor)
                    .relative_to(anchor)
                    .offset(self.config.offset.x, self.config.offset.y)
                    .width(self.config.width)
                    .height(self.config.height),
            )
            .hit(
                Sides::new()
                    .top(self.config.offset.y.max(0.0))
                    .right((-self.config.offset.x).max(0.0))
                    .bottom((-self.config.offset.y).max(0.0))
                    .left(self.config.offset.x.max(0.0)),
            )
            .widget_id(content_id)
            .layout(single::layout())
            .child(single::item().grow())
            .build(self.content);
        Some(response)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Modifiers, PointerButton, Rect, Size};

    use super::*;

    struct TestPlatform;

    impl Platform for TestPlatform {
        fn begin(&mut self, _: FrameInfo) {}

        fn end(&mut self) {}
    }

    #[test]
    fn popover_uses_root_constraints_and_close_behavior() {
        fn build(ui: Ui<'_, TestPlatform>, state: &mut State, config: Config) {
            ui.build(
                Popover::new(state)
                    .config(config)
                    .trigger(|ui: Ui<'_, TestPlatform>, _, _| {
                        ui.widget_id(WidgetId::new("named trigger"))
                            .layout(single::layout())
                            .child(single::item().fixed(2.0, 1.0))
                            .build(())
                    })
                    .build(|ui: Ui<'_, TestPlatform>| {
                        ui.widget_id(WidgetId::new("named content"))
                            .layout(single::layout())
                            .child(single::item().fixed(4.0, 3.0))
                            .build(())
                    }),
            );
        }

        let mut frame = Frame::default();
        let mut platform = TestPlatform;
        let info = FrameInfo::new(Size::uniform(10.0));
        let mut state = State::new();
        let content_id = state.id.child("popover content");

        frame.render(&mut platform, info, |ui: Ui<'_, TestPlatform>| {
            build(ui, &mut state, Config::new());
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
                build(ui, &mut state, config);
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
                build(
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
