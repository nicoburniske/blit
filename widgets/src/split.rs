use blit::{Axis, Constraints, Context, LayoutCx, Point, Press, Scalar, Size, Ui, Widget};

use crate::Drag;

blit::builder! {
    pub struct Split<'a, C: Context> {
        new(state: &'a mut SplitState<C::Scalar>),
        leading<L = fn(Ui<'_, C>)>: L = |_| (),
        trailing<R = fn(Ui<'_, C>)>: R = |_| (),
        #[bounds(FnOnce(Ui<'_, C>, SplitDivider))]
        divider<D = fn(Ui<'_, C>, SplitDivider)>: D = |_, _| (),
        initial_extent: C::Scalar = C::Scalar::ZERO,
        axis: Axis = Axis::Horizontal,
        divider_extent: C::Scalar = C::Scalar::from_f32(1.0),
        minimum_leading: C::Scalar = C::Scalar::ZERO,
        minimum_trailing: C::Scalar = C::Scalar::ZERO,
    }
}

/// what a divider builder needs to draw itself
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitDivider {
    pub axis: Axis,
    pub hovered: bool,
    /// pressed or being dragged
    pub active: bool,
}

#[derive(Debug, Default)]
pub struct SplitState<T> {
    extent: Option<T>,
    changed: bool,
}

impl<T: Scalar> SplitState<T> {
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

impl<C: Context, L, R, D> Widget<C> for Split<'_, C, L, R, D>
where
    L: Widget<C>,
    R: Widget<C>,
    D: FnOnce(Ui<'_, C>, SplitDivider),
{
    type Response = ();
    fn build(self, mut ui: Ui<'_, C>) {
        let state = self.state;
        let leading = self.leading;
        let trailing = self.trailing;
        let divider = self.divider;
        let id = ui.current_id();
        let axis = self.axis;
        let leading_id = id.child("leading pane");
        let divider_id = id.child("divider");
        let trailing_id = id.child("trailing pane");
        let press = ui.interact_widget(divider_id, Press::new());
        let measured = ui.area(leading_id).map(|area| axis.extent(area.size()));
        let drag = ui.interact_widget(divider_id, Drag::new());
        let delta = match axis {
            Axis::Horizontal => drag.delta.x,
            Axis::Vertical => drag.delta.y,
        };
        if !state.changed
            && let Some(measured) = measured
        {
            state.extent = Some(measured);
        }
        if delta != C::Scalar::ZERO {
            let extent = state.extent.unwrap_or(self.initial_extent);
            state.extent = Some(extent + delta);
        }
        let extent = state.extent.unwrap_or(self.initial_extent);
        state.changed = false;
        let mut panes = ui.layout(Layout {
            axis,
            divider_extent: self.divider_extent,
            extent,
            minimum_leading: self.minimum_leading,
            minimum_trailing: self.minimum_trailing,
        });
        panes.child().id(leading_id).build(leading);
        panes.child().id(divider_id).build(|ui: Ui<'_, C>| {
            divider(
                ui,
                SplitDivider {
                    axis,
                    hovered: press.hovered,
                    active: press.active || drag.active,
                },
            )
        });
        panes.child().id(trailing_id).build(trailing);
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
        let mut point = Size::ZERO;
        self.axis.set_extent(&mut point, leading_extent);
        cx.layout_child(divider, Constraints::tight(size));
        cx.set_child_position(divider, Point::new(point.width, point.height));
        self.axis.set_extent(&mut size, main);
        bounds.constrain(size)
    }
}
