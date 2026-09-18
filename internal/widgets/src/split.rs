use blit::{Axis, Context, Sense, Ui, Widget, WidgetId};
use blit_layout::split::{Item, Layout};

blit::builder! {
    /// split behavior and geometry
    #[derive(Clone, Copy, Debug)]
    pub struct Config {
        new(initial_extent: f32),
        axis: Axis = Axis::Horizontal,
        divider_extent: f32 = 1.0,
        minimum_leading: f32 = 0.0,
        minimum_trailing: f32 = 0.0,
        sense: Sense = Sense::DRAG,
    }
}

#[derive(Debug, Default)]
pub struct State {
    extent: Option<f32>,
    changed: bool,
}

impl State {
    pub fn extent(&self) -> Option<f32> {
        self.extent
    }

    pub fn set_extent(&mut self, extent: f32) {
        self.extent = Some(extent.max(0.0));
        self.changed = true;
    }

    pub fn reset(&mut self) {
        self.extent = None;
        self.changed = true;
    }
}

pub fn build<C, L, T, D>(
    mut ui: Ui<'_, C>,
    state: &mut State,
    id: WidgetId,
    config: Config,
    leading: L,
    trailing: T,
    divider: impl FnOnce(Axis, blit::Interaction) -> D,
) where
    C: Context,
    L: Widget<C>,
    T: Widget<C>,
    D: Widget<C>,
{
    let axis = config.axis;
    let leading_id = id.child("leading pane");
    let divider_id = id.child("divider");
    let trailing_id = id.child("trailing pane");
    let interaction = ui.interact(divider_id, config.sense);
    let measured = ui.geometry(leading_id).map(|area| match axis {
        Axis::Horizontal => area.width,
        Axis::Vertical => area.height,
    });
    let delta = match axis {
        Axis::Horizontal => interaction.drag_delta.x,
        Axis::Vertical => interaction.drag_delta.y,
    };
    if !state.changed
        && let Some(measured) = measured
    {
        state.extent = Some(measured);
    }
    if delta != 0.0 {
        let extent = if state.changed {
            state.extent
        } else {
            measured.or(state.extent)
        }
        .unwrap_or(config.initial_extent);
        state.extent = Some(extent + delta);
    }
    let extent = state.extent.unwrap_or(config.initial_extent);
    state.changed = false;
    let mut panes = ui
        .layout(Layout {
            axis,
            divider_extent: config.divider_extent,
            extent,
            minimum_leading: config.minimum_leading,
            minimum_trailing: config.minimum_trailing,
        })
        .widget_id(id);
    panes
        .child(Item::Leading)
        .widget_id(leading_id)
        .build(leading);
    panes
        .child(Item::Divider)
        .widget_id(divider_id)
        .build(divider(axis, interaction));
    panes
        .child(Item::Trailing)
        .widget_id(trailing_id)
        .build(trailing);
}

#[cfg(test)]
mod tests {
    use blit::{Atom, Constraints, Frame, FrameInfo, Rect, Size};
    use blit_layout::single;

    use super::*;

    struct TestContext;

    impl Context for TestContext {}

    struct BoxAtom;

    impl Atom<TestContext> for BoxAtom {
        fn measure(&self, _: &mut TestContext, constraints: Constraints) -> Size {
            constraints.min
        }

        fn paint(&self, _: &mut TestContext, _: Rect) {}

        fn paint_bounds(&self, area: Rect) -> Rect {
            area
        }
    }

    #[test]
    fn clamps_the_leading_extent() {
        let mut frame = Frame::default();
        let mut state = State::default();
        let id = WidgetId::new("split pane");
        state.set_extent(90.0);
        frame.render(
            &mut TestContext,
            FrameInfo::new(Size::new(100.0, 20.0)),
            |ui: Ui<'_, TestContext>| {
                ui.layout(single::layout())
                    .child(single::item().grow())
                    .build(|ui: Ui<'_, TestContext>| {
                        build(
                            ui,
                            &mut state,
                            id,
                            Config::new(30.0)
                                .minimum_leading(20.0)
                                .minimum_trailing(20.0)
                                .divider_extent(4.0),
                            |mut ui: Ui<'_, TestContext>| ui.insert(BoxAtom),
                            |mut ui: Ui<'_, TestContext>| ui.insert(BoxAtom),
                            |_, _| (),
                        )
                    });
            },
        );
        assert_eq!(
            frame.geometry(id.child("leading pane")),
            Some(Rect::new(0.0, 0.0, 76.0, 20.0))
        );
    }
}
