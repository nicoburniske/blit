use blit::{Axis, Constraints, LayoutCx, Point, Sides, Size};

use super::{flow_constraints, sizing_range};
use crate::{Context, Sizing, layout_child, resolve_sizing};

blit::builder! {
    /// lays out at most one child
    ///
    /// percentages use the space offered to the layout, not the child's natural size
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout {
        new(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
        padding: Sides = Sides::all(0.0),
    }
}

pub fn new() -> Layout {
    Layout::new()
}

impl Layout {
    pub fn fixed(mut self, width: f32, height: f32) -> Self {
        self.width = Sizing::fixed(width);
        self.height = Sizing::fixed(height);
        self
    }

    pub fn grow(mut self) -> Self {
        self.width = Sizing::grow();
        self.height = Sizing::grow();
        self
    }
}

impl<C: Context> blit::Layout<C> for Layout {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> Size {
        let padding = crate::round_padding::<C>(self.padding);
        let mut children = cx.children();
        let child = children.next();
        assert!(children.next().is_none(), "single accepts at most one flow child");
        let range = |axis: Axis, sizing| {
            let sizing = resolve_sizing(cx, cx.node(), axis, sizing);
            let minimum = axis.extent(bounds.min);
            let maximum = axis.extent(bounds.max);
            if let Sizing::Grow { .. } = sizing {
                // forwards the budget instead of claiming its maximum
                (sizing.clamp(minimum), sizing.clamp(maximum).max(sizing.clamp(minimum)))
            } else {
                sizing_range::<C>(sizing, maximum)
            }
        };
        let sizing = flow_constraints(
            Axis::Horizontal,
            range(Axis::Horizontal, self.width),
            range(Axis::Vertical, self.height),
        );
        let size = child.map_or(Size::ZERO, |child| {
            let size = layout_child(cx, child, sizing.shrink(padding.size()));
            cx.set_child_position(child, Point::new(padding.left, padding.top));
            size
        });
        bounds.constrain(sizing.constrain(size + padding.size()))
    }
}
