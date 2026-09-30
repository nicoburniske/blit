use blit::{Axis, Constraints, IntrinsicQuery, IntrinsicSize, LayoutCx, LogicalPoint, LogicalSize, MeasureCx};

pub use crate::size::Item;
use crate::{Padding, Sizing, Unit, flow_constraints, intrinsic_child, sizing_range};

blit::builder! {
    #[const]
    /// lays out at most one child
    ///
    /// percentages use the space offered to the layout, not the child's natural size
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<U: Unit> {
        new(),
        padding: Padding<U> = Padding::all(U::ZERO),
    }
}

impl<C, U: Unit> blit::Layout<C> for Layout<U> {
    type Item = Item<U>;

    fn intrinsic(&self, cx: &mut MeasureCx<'_, C, Self::Item>, query: IntrinsicQuery) -> IntrinsicSize {
        let padding: blit::Sides = self.padding.into();
        let extent = query.axis.extent(padding.size());
        let mut children = cx.children();
        let Some(child) = children.next() else {
            return IntrinsicSize {
                min: extent,
                preferred: extent,
            };
        };
        assert!(children.next().is_none(), "single accepts at most one flow child");
        let (main, cross) = crate::flow_sizing(query.axis, child.item.width, child.item.height, (None, None));
        let size = intrinsic_child::<_, _, U>(
            cx,
            child.id,
            IntrinsicQuery {
                axis: query.axis,
                cross: query
                    .cross
                    .map(|value| (value - query.axis.other().extent(padding.size())).max(0.0)),
            },
            main,
            cross,
            false,
        );
        IntrinsicSize {
            min: size.min + extent,
            preferred: size.preferred + extent,
        }
    }

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> LogicalSize {
        let padding: blit::Sides = self.padding.into();
        let mut children = cx.children();
        let Some(child) = children.next() else {
            return bounds.constrain(padding.size());
        };
        assert!(children.next().is_none(), "single accepts at most one flow child");
        let content = bounds.shrink(padding.size());
        let item = child.item;
        let (width, height) = cx.size_overrides(child.id);
        let range = |axis: Axis, sizing: Sizing<f32>| {
            let minimum = axis.extent(content.min);
            let maximum = axis.extent(content.max);
            if matches!(sizing, Sizing::Grow { .. }) {
                // a single child forwards the budget instead of claiming its maximum
                (sizing.clamp(minimum), sizing.clamp(maximum).max(sizing.clamp(minimum)))
            } else {
                sizing_range::<U>(sizing, maximum)
            }
        };
        let child_bounds = flow_constraints::<U>(
            Axis::Horizontal,
            range(Axis::Horizontal, item.width.with_override(width)),
            range(Axis::Vertical, item.height.with_override(height)),
        );
        let size = cx.layout_child(child.id, child_bounds);
        cx.set_position(child.id, LogicalPoint::new(padding.left, padding.top));
        bounds.constrain(size + padding.size())
    }
}
