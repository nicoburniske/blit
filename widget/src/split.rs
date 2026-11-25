use blit::{
    Axis, Constraints, LayoutCx, Point, Scalar, Scope, Size, Ui,
    state::{NoLayout, Open},
};

use crate::{Context, interact};

blit::builder! {
    pub struct Split<'a, T: Scalar> {
        new(state: &'a mut SplitState<T>),
        initial_extent: T = T::ZERO,
        axis: Axis = Axis::Horizontal,
        divider_extent: T = T::from_f32(1.0),
        minimum_first: T = T::ZERO,
        minimum_second: T = T::ZERO,
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

impl<T: Scalar> Split<'_, T> {
    pub fn begin<'a, C, S>(self, mut ui: Ui<'a, C, S>) -> SplitFirst<'a, C>
    where
        C: Context<Scalar = T>,
        S: NoLayout,
    {
        let state = self.state;
        let axis = self.axis;
        let scope = ui.scope();
        let divider_id = ui.resolve(scope, DIVIDER);
        let (press, drag) = {
            let mut part = ui.node(divider_id);
            (part.interact(interact::press()), part.interact(interact::drag()))
        };
        let first_id = ui.resolve(scope, FIRST);
        let measured = ui.node(first_id).area().map(|area| axis.extent(area.size()));
        let delta = match axis {
            Axis::Horizontal => drag.delta.x,
            Axis::Vertical => drag.delta.y,
        };
        if !state.changed
            && let Some(measured) = measured
        {
            state.extent = Some(measured);
        }
        if delta != T::ZERO {
            let extent = state.extent.unwrap_or(self.initial_extent);
            state.extent = Some(extent + delta);
        }
        let extent = state.extent.unwrap_or(self.initial_extent);
        state.changed = false;
        let panes = ui.layout(Layout {
            axis,
            divider_extent: self.divider_extent,
            extent,
            minimum_first: self.minimum_first,
            minimum_second: self.minimum_second,
        });
        SplitFirst(SplitSeparator {
            panes,
            scope,
            divider: SplitDivider {
                axis,
                hovered: press.hovered,
                active: press.active || drag.active,
            },
        })
    }
}

#[must_use]
pub struct SplitFirst<'a, C: Context>(SplitSeparator<'a, C>);

impl<'a, C: Context> SplitFirst<'a, C> {
    pub fn first(mut self, first: impl FnOnce(Ui<'_, C>)) -> SplitSeparator<'a, C> {
        self.0.panes.child_in(self.0.scope, FIRST).build(first);
        self.0
    }
}

#[must_use]
pub struct SplitSeparator<'a, C: Context> {
    panes: Ui<'a, C, Open<Layout<C::Scalar>>>,
    scope: Scope,
    divider: SplitDivider,
}

impl<'a, C: Context> SplitSeparator<'a, C> {
    pub fn divider(mut self, divider: impl FnOnce(Ui<'_, C>, SplitDivider)) -> SplitSecond<'a, C> {
        let state = self.divider;
        self.panes
            .child_in(self.scope, DIVIDER)
            .build(|ui: Ui<'_, C>| divider(ui, state));
        SplitSecond { panes: self.panes }
    }
}

#[must_use]
pub struct SplitSecond<'a, C: Context> {
    panes: Ui<'a, C, Open<Layout<C::Scalar>>>,
}

impl<C: Context> SplitSecond<'_, C> {
    pub fn second(mut self, second: impl FnOnce(Ui<'_, C>)) {
        self.panes.child().build(second);
    }
}

const FIRST: u8 = 0;
const DIVIDER: u8 = 1;

#[derive(Clone, Copy)]
struct Layout<T> {
    axis: Axis,
    divider_extent: T,
    extent: T,
    minimum_first: T,
    minimum_second: T,
}

impl<C: Context<Scalar = T>, T: Scalar> blit::Layout<C> for Layout<T> {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<T>) -> Size<T> {
        let mut children = cx.children();
        let first = children.next();
        let divider = children.next();
        let second = children.next();
        let cross_axis = self.axis.other();
        let main = self.axis.extent(bounds.max);
        assert!(main.is_finite(), "split needs a finite main axis budget");
        let divider_extent = self.divider_extent.max(T::ZERO).min(main);
        let available = (main - divider_extent).max(T::ZERO);
        let minimum_first = self.minimum_first.max(T::ZERO);
        let minimum_second = self.minimum_second.max(T::ZERO);
        let desired = self.extent.max(T::ZERO);
        let first_extent = if minimum_first <= available - minimum_second {
            desired.clamp(minimum_first, available - minimum_second)
        } else {
            T::from_f32(
                available.to_f32() * minimum_first.to_f32() / (minimum_first.to_f32() + minimum_second.to_f32()),
            )
        }
        .min(available);
        let mut cross = cross_axis.extent(bounds.min);
        for (child, extent, offset) in [
            (first, first_extent, T::ZERO),
            (second, available - first_extent, first_extent + divider_extent),
        ] {
            let Some(child) = child else {
                continue;
            };
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
        if let Some(divider) = divider {
            let mut point = Size::ZERO;
            self.axis.set_extent(&mut point, first_extent);
            cx.layout_child(divider, Constraints::tight(size));
            cx.set_child_position(divider, Point::new(point.width, point.height));
        }
        self.axis.set_extent(&mut size, main);
        bounds.constrain(size)
    }
}
