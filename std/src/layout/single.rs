use blit::{Axis, Constraints, Layout, LayoutCx, Platform, Point, Sides, Size, Sizing};

use super::{flow_constraints, override_sizing, sizing_range};

blit::builder! {
    /// sizing policy for the child of a single layout
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct SingleItem {
        new(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
    }
}

impl SingleItem {
    pub fn fixed(width: f32, height: f32) -> Self {
        Self::new()
            .width(Sizing::fixed(width))
            .height(Sizing::fixed(height))
    }

    pub fn grow() -> Self {
        Self::new().width(Sizing::grow()).height(Sizing::grow())
    }
}

blit::builder! {
    /// lays out at most one direct child
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Single {
        new(),
        padding: Sides = Sides::all(0.0),
    }
}

impl<P: Platform> Layout<P> for Single {
    type Item = SingleItem;

    fn size_override(&self, item: &mut Self::Item, width: Option<f32>, height: Option<f32>) {
        override_sizing(&mut item.width, &mut item.height, width, height);
    }

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        fn range(sizing: Sizing, minimum: f32, maximum: f32) -> (f32, f32) {
            if matches!(sizing, Sizing::Percent(_)) {
                // percentages fall back to intrinsic sizing until the extent is definite
                return if minimum == maximum {
                    sizing_range(sizing, maximum)
                } else {
                    (0.0, maximum)
                };
            }
            let mut range = sizing_range(sizing, maximum);
            if matches!(sizing, Sizing::Grow { .. }) {
                range.0 = sizing.clamp(minimum).min(range.1);
            }
            range
        }

        let padding = cx.resolve_sides(self.padding);
        let padding_size = padding.size();
        let mut children = cx.children();
        let Some(child) = children.next() else {
            return constraints.constrain(padding_size);
        };
        assert!(
            children.next().is_none(),
            "single layout accepts at most one direct child"
        );

        let content = constraints.shrink(padding_size);
        let item = cx.item(child);
        let width = range(
            cx.resolve_sizing(Axis::Horizontal, item.width),
            content.min.width,
            content.max.width,
        );
        let height = range(
            cx.resolve_sizing(Axis::Vertical, item.height),
            content.min.height,
            content.max.height,
        );
        let child_size = cx.layout_child(child, flow_constraints(Axis::Horizontal, width, height));
        cx.set_position(child, Point::new(padding.left, padding.top));
        constraints.constrain(Size::new(
            child_size.width + padding_size.width,
            child_size.height + padding_size.height,
        ))
    }
}
