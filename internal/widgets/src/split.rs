use blit::{
    Axis, Constraints, Layout as LayoutTrait, LayoutCx, Point, Sense, Size, Ui, Widget, WidgetId,
};

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

pub fn new<'a, C, L, T, D, W>(
    state: &'a mut State,
    id: WidgetId,
    config: Config,
    divider: D,
    leading: L,
    trailing: T,
) -> impl Widget<C> + 'a
where
    L: Widget<C> + 'a,
    T: Widget<C> + 'a,
    D: FnOnce(Axis, blit::Interaction) -> W + 'a,
    W: Widget<C>,
{
    move |mut ui: Ui<'_, C>| {
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
            .child_item(Item::Leading)
            .widget_id(leading_id)
            .build(leading);
        panes
            .child_item(Item::Divider)
            .widget_id(divider_id)
            .build(divider(axis, interaction));
        panes
            .child_item(Item::Trailing)
            .widget_id(trailing_id)
            .build(trailing);
    }
}

#[derive(Clone, Copy)]
enum Item {
    Leading,
    Divider,
    Trailing,
}

#[derive(Clone, Copy)]
struct Layout {
    axis: Axis,
    divider_extent: f32,
    extent: f32,
    minimum_leading: f32,
    minimum_trailing: f32,
}

impl<C> LayoutTrait<C> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> Size {
        let cross_axis = self.axis.other();
        let res = cx.resolution();
        let main = self.axis.extent(bounds.max);
        assert!(main.is_finite(), "split needs a finite main axis budget");
        let mut leading = None;
        let mut trailing = None;
        let mut divider = None;
        for child in cx.children() {
            match cx.item(child) {
                Item::Leading => leading = Some(child),
                Item::Trailing => trailing = Some(child),
                Item::Divider => divider = Some(child),
            }
        }
        let leading = leading.expect("missing split leading content");
        let trailing = trailing.expect("missing split trailing content");
        let divider = divider.expect("missing split divider");
        let divider_extent = res
            .extent(self.axis, self.divider_extent)
            .max(0.0)
            .min(main);
        let available = (main - divider_extent).max(0.0);
        let minimum_leading = res.extent(self.axis, self.minimum_leading).max(0.0);
        let minimum_trailing = res.extent(self.axis, self.minimum_trailing).max(0.0);
        let desired = res.extent(self.axis, self.extent).max(0.0);
        let leading_extent = if minimum_leading + minimum_trailing <= available {
            desired.clamp(minimum_leading, available - minimum_trailing)
        } else if minimum_leading + minimum_trailing > 0.0 {
            available * minimum_leading / (minimum_leading + minimum_trailing)
        } else {
            desired.min(available)
        };
        let mut cross = cross_axis.extent(bounds.min);
        for (child, extent, offset) in [
            (leading, leading_extent, 0.0),
            (
                trailing,
                available - leading_extent,
                leading_extent + divider_extent,
            ),
        ] {
            let mut child_bounds = bounds;
            self.axis.set_extent(&mut child_bounds.min, extent);
            self.axis.set_extent(&mut child_bounds.max, extent);
            let size = cx.layout_child(child, child_bounds);
            cross = cross.max(cross_axis.extent(size));
            let mut point = Size::ZERO;
            self.axis.set_extent(&mut point, offset);
            cx.set_child_position(child, Point::new(point.width, point.height));
        }
        let mut size = Size::ZERO;
        self.axis.set_extent(&mut size, divider_extent);
        cross_axis.set_extent(&mut size, cross);
        cx.layout_child(divider, Constraints::tight(size));
        let mut point = Size::ZERO;
        self.axis.set_extent(&mut point, leading_extent);
        cx.set_child_position(divider, Point::new(point.width, point.height));
        self.axis.set_extent(&mut size, main);
        bounds.constrain(size)
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Atom, Constraints, Frame, FrameInfo, Input, Rect, Size};
    use blit_layout::single;

    use super::*;
    use crate::test::TestContext;

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
        let context = &mut TestContext;
        frame.build(
            context,
            FrameInfo::new(Size::new(100.0, 20.0)),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                ui.layout(single::layout())
                    .child()
                    .item(single::item().grow())
                    .build(new(
                        &mut state,
                        id,
                        Config::new(30.0)
                            .minimum_leading(20.0)
                            .minimum_trailing(20.0)
                            .divider_extent(4.0),
                        |_, _| (),
                        |mut ui: Ui<'_, TestContext>| ui.insert(BoxAtom),
                        |mut ui: Ui<'_, TestContext>| ui.insert(BoxAtom),
                    ));
            },
        );
        frame.layout(context);
        assert_eq!(
            frame.geometry(id.child("leading pane")),
            Some(Rect::new(0.0, 0.0, 76.0, 20.0))
        );
    }
}
