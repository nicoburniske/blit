use blit::{Axis, Constraints, LayoutCx, Platform, Point, Sides, Size, Sizing};

pub use super::sizing::{Item, item};
use super::{flow_constraints, override_sizing, percentage, sizing_range};

blit::builder! {
    /// lays out at most one direct child
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

        let res = cx.resolution();
        let padding = res.sides(self.padding);
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
        let width_sizing = res.sizing(Axis::Horizontal, item.width);
        let height_sizing = res.sizing(Axis::Vertical, item.height);
        let width = range(width_sizing, content.min.width, content.max.width);
        let height = range(height_sizing, content.min.height, content.max.height);
        let child_size = cx.layout_child(child, flow_constraints(Axis::Horizontal, width, height));
        let size = constraints.constrain(Size::new(
            child_size.width + padding_size.width,
            child_size.height + padding_size.height,
        ));
        let available = (size - padding_size).max(Size::ZERO);
        let final_size = Size::new(
            match width_sizing {
                Sizing::Percent(fraction) => percentage(fraction, available.width),
                _ => child_size.width,
            },
            match height_sizing {
                Sizing::Percent(fraction) => percentage(fraction, available.height),
                _ => child_size.height,
            },
        );
        if final_size != child_size {
            cx.layout_child(child, Constraints::tight(final_size));
        }
        cx.set_child_position(child, Point::new(padding.left, padding.top));
        size
    }

    fn override_size(
        &self,
        item: &mut Self::Item,
        width: Option<f32>,
        height: Option<f32>,
    ) -> bool {
        override_sizing(&mut item.width, &mut item.height, width, height)
    }
}
