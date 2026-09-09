use blit::{Axis, Constraints, LayoutCx, Platform, Point, Sides, Size, Sizing};

pub use super::sizing::{Item, item};
use super::{flow_constraints, override_sizing, sizing_range};

blit::builder! {
    /// lays out at most one child
    ///
    /// percentages use the space offered to the layout, not the child's natural size
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout {
        new(),
        padding: Sides = Sides::all(0.0),
    }
}

pub fn layout() -> Layout {
    Layout::new()
}

impl<P: Platform> blit::Layout<P> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, bounds: Constraints) -> Size {
        let res = cx.resolution();
        let padding = res.sides(self.padding);
        let mut children = cx.children();
        let Some(child) = children.next() else {
            return bounds.constrain(padding.size());
        };
        assert!(
            children.next().is_none(),
            "single accepts at most one flow child"
        );
        let content = bounds.shrink(padding.size());
        let item = cx.item(child);
        let range = |axis: Axis, sizing| {
            let sizing = res.sizing(axis, sizing);
            let minimum = axis.extent(content.min);
            let maximum = axis.extent(content.max);
            if matches!(sizing, Sizing::Grow { .. }) {
                // a single child forwards the budget instead of claiming its maximum
                (
                    sizing.clamp(minimum),
                    sizing.clamp(maximum).max(sizing.clamp(minimum)),
                )
            } else {
                sizing_range(sizing, maximum)
            }
        };
        let child_bounds = flow_constraints(
            Axis::Horizontal,
            range(Axis::Horizontal, item.width),
            range(Axis::Vertical, item.height),
        );
        let size = cx.layout_child(child, child_bounds);
        cx.set_child_position(child, Point::new(padding.left, padding.top));
        bounds.constrain(Size::new(
            size.width + padding.left + padding.right,
            size.height + padding.top + padding.bottom,
        ))
    }

    fn override_size(&self, item: &mut Item, width: Option<f32>, height: Option<f32>) -> bool {
        override_sizing(&mut item.width, &mut item.height, width, height)
    }
}
