use blit::{Axis, Constraints, LayoutCx, Point, Size};

pub use crate::size::{Item, item};
use crate::{Length, Padding, Sizing, flow_constraints, sizing_range};

blit::builder! {
    /// lays out at most one child
    ///
    /// percentages use the space offered to the layout, not the child's natural size
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout {
        new(),
        padding: Padding = Padding::all(0 as Length),
    }
}

pub fn layout() -> Layout {
    Layout::new()
}

impl<C> blit::Layout<C> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> Size {
        let padding: blit::Sides = self.padding.into();
        let mut children = cx.children();
        let Some(child) = children.next() else {
            return bounds.constrain(padding.size());
        };
        assert!(children.next().is_none(), "single accepts at most one flow child");
        let content = bounds.shrink(padding.size());
        let item = cx.item(child);
        let (width, height) = cx.size_overrides(child);
        let range = |axis: Axis, sizing: Sizing<f32>| {
            let minimum = axis.extent(content.min);
            let maximum = axis.extent(content.max);
            if matches!(sizing, Sizing::Grow { .. }) {
                // a single child forwards the budget instead of claiming its maximum
                (sizing.clamp(minimum), sizing.clamp(maximum).max(sizing.clamp(minimum)))
            } else {
                sizing_range(sizing, maximum)
            }
        };
        let child_bounds = flow_constraints(
            Axis::Horizontal,
            range(Axis::Horizontal, item.width.with_override(width)),
            range(Axis::Vertical, item.height.with_override(height)),
        );
        let size = cx.layout_child(child, child_bounds);
        cx.set_position(child, Point::new(padding.left, padding.top));
        bounds.constrain(size + padding.size())
    }
}
