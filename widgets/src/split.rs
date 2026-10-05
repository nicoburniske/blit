use blit::{Axis, Constraints, Context, LayoutCx, Point, Scalar, Sense, Size, Ui, Widget};

blit::builder! {
    /// split behavior and geometry
    #[derive(Clone, Copy, Debug)]
    pub struct Config<T: Scalar> {
        new(initial_extent: T),
        axis: Axis = Axis::Horizontal,
        divider_extent: T = T::from_f32(1.0),
        minimum_leading: T = T::ZERO,
        minimum_trailing: T = T::ZERO,
        sense: Sense = Sense::DRAG,
    }
}

#[derive(Debug, Default)]
pub struct State<T> {
    extent: Option<T>,
    changed: bool,
}

impl<T: Scalar> State<T> {
    pub fn extent(&self) -> Option<T> {
        self.extent
    }

    pub fn set_extent(&mut self, extent: T) {
        self.extent = Some(extent.max(T::ZERO));
        self.changed = true;
    }

    pub fn reset(&mut self) {
        self.extent = None;
        self.changed = true;
    }
}

pub fn new<'a, C: Context, L, T, D, W>(
    state: &'a mut State<C::Scalar>,
    config: Config<C::Scalar>,
    divider: D,
    leading: L,
    trailing: T,
) -> impl Widget<C> + 'a
where
    L: Widget<C> + 'a,
    T: Widget<C> + 'a,
    D: FnOnce(Axis, blit::Interaction<C::Scalar>) -> W + 'a,
    W: Widget<C>,
{
    move |mut ui: Ui<'_, C>| {
        let id = ui.current_widget_id();
        let axis = config.axis;
        let leading_id = id.child("leading pane");
        let divider_id = id.child("divider");
        let trailing_id = id.child("trailing pane");
        let interaction = ui.interact_widget(divider_id, config.sense);
        let measured = ui.geometry(leading_id).map(|area| axis.extent(area.size()));
        let delta = match axis {
            Axis::Horizontal => interaction.drag_delta.x,
            Axis::Vertical => interaction.drag_delta.y,
        };
        if !state.changed
            && let Some(measured) = measured
        {
            state.extent = Some(measured);
        }
        if delta != C::Scalar::ZERO {
            let extent = state.extent.unwrap_or(config.initial_extent);
            state.extent = Some(extent + delta);
        }
        let extent = state.extent.unwrap_or(config.initial_extent);
        state.changed = false;
        let mut panes = ui.layout(Layout {
            axis,
            divider_extent: config.divider_extent,
            extent,
            minimum_leading: config.minimum_leading,
            minimum_trailing: config.minimum_trailing,
        });
        panes.child().widget_id(leading_id).build(leading);
        panes.child().widget_id(divider_id).build(divider(axis, interaction));
        panes.child().widget_id(trailing_id).build(trailing);
    }
}

#[derive(Clone, Copy)]
struct Layout<T> {
    axis: Axis,
    divider_extent: T,
    extent: T,
    minimum_leading: T,
    minimum_trailing: T,
}

impl<C: Context<Scalar = T>, T: Scalar> blit::Layout<C> for Layout<T> {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<T>) -> Size<T> {
        let mut children = cx.children();
        let leading = children.next().expect("missing split leading content");
        let divider = children.next().expect("missing split divider");
        let trailing = children.next().expect("missing split trailing content");
        let cross_axis = self.axis.other();
        let main = self.axis.extent(bounds.max);
        assert!(main.is_finite(), "split needs a finite main axis budget");
        let divider_extent = self.divider_extent.max(T::ZERO).min(main);
        let available = (main - divider_extent).max(T::ZERO);
        let minimum_leading = self.minimum_leading.max(T::ZERO);
        let minimum_trailing = self.minimum_trailing.max(T::ZERO);
        let desired = self.extent.max(T::ZERO);
        let leading_extent = if minimum_leading <= available - minimum_trailing {
            desired.clamp(minimum_leading, available - minimum_trailing)
        } else {
            T::from_f32(
                available.to_f32() * minimum_leading.to_f32() / (minimum_leading.to_f32() + minimum_trailing.to_f32()),
            )
        }
        .min(available);
        let mut cross = cross_axis.extent(bounds.min);
        for (child, extent, offset) in [
            (leading, leading_extent, T::ZERO),
            (trailing, available - leading_extent, leading_extent + divider_extent),
        ] {
            let mut child_bounds = bounds;
            self.axis.set_extent(&mut child_bounds.min, extent);
            self.axis.set_extent(&mut child_bounds.max, extent);
            let mut point = Size::ZERO;
            self.axis.set_extent(&mut point, offset);
            let size = cx.layout_child(child, child_bounds);
            cx.set_child_position(child, Point::new(point.width, point.height));
            cross = cross.max(cross_axis.extent(size));
        }
        let mut size = Size::ZERO;
        self.axis.set_extent(&mut size, divider_extent);
        cross_axis.set_extent(&mut size, cross);
        let divider_size = size;
        let mut point = Size::ZERO;
        self.axis.set_extent(&mut point, leading_extent);
        self.axis.set_extent(&mut size, main);
        cx.layout_child(divider, Constraints::tight(divider_size));
        cx.set_child_position(divider, Point::new(point.width, point.height));
        bounds.constrain(size)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Atom, Constraints, Frame, FrameInfo, Input, Rect, Size, WidgetId};
    use blit_layout::single;

    use super::*;
    use crate::test::TestContext;

    struct BoxAtom;

    impl Atom<TestContext> for BoxAtom {
        fn measure(&self, _: &mut TestContext, constraints: Constraints<f32>) -> Size<f32> {
            constraints.min
        }

        fn paint(&self, _: &mut TestContext, _: Rect<f32>) {}

        fn paint_bounds(&self, area: Rect<f32>) -> Rect<f32> {
            area
        }
    }

    #[test]
    fn clamps_the_leading_extent() {
        let mut frame = Frame::default();
        let mut state = State::default();
        state.set_extent(90.0);
        let id = WidgetId::new("split pane");
        let context = &mut TestContext;
        frame.build(
            context,
            FrameInfo::new(Size::new(100.0, 20.0)),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                ui.layout(single::new().grow()).child().widget_id(id).build(new(
                    &mut state,
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
