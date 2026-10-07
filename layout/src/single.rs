use blit::{Axis, Constraints, Context, LayoutCx, Point, Scalar, Sides, Size};

use super::{Sizing, flow_constraints, resolve_sizing, sizing_range};

blit::builder! {
    /// lays out at most one child
    ///
    /// percentages use the space offered to the layout, not the child's natural size
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<T: Scalar> {
        new(),
        width: Sizing<T> = Sizing::fit(),
        height: Sizing<T> = Sizing::fit(),
        padding: Sides<T> = Sides::all(T::ZERO),
    }
}

pub fn new<T: Scalar>() -> Layout<T> {
    Layout::new()
}

impl<T: Scalar> Layout<T> {
    pub fn fixed(mut self, width: T, height: T) -> Self {
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

impl<C: Context<Scalar = T>, T: Scalar> blit::Layout<C> for Layout<T> {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<T>) -> Size<T> {
        let padding = self.padding;
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
                sizing_range(sizing, maximum)
            }
        };
        let sizing = flow_constraints(
            Axis::Horizontal,
            range(Axis::Horizontal, self.width),
            range(Axis::Vertical, self.height),
        );
        let size = child.map_or(Size::ZERO, |child| {
            let size = cx.layout_child(child, sizing.shrink(padding.size()));
            cx.set_child_position(child, Point::new(padding.left, padding.top));
            size
        });
        bounds.constrain(sizing.constrain(size + padding.size()))
    }
}
