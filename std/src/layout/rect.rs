use blit::{Constraints, Layout, LayoutCx, Platform, Point, Rect, Size};

/// places direct children in supplied local rectangles
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RectLayout;

impl<P: Platform> Layout<P> for RectLayout {
    type Item = Rect;

    fn size_override(&self, item: &mut Self::Item, width: Option<f32>, height: Option<f32>) {
        if let Some(width) = width {
            item.width = width;
        }
        if let Some(height) = height {
            item.height = height;
        }
    }

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        let mut natural = Size::default();
        for node in cx.children() {
            let rect = cx.item(node);
            natural.width = natural.width.max((rect.x + rect.width.max(0.0)).max(0.0));
            natural.height = natural.height.max((rect.y + rect.height.max(0.0)).max(0.0));
            cx.layout_child(
                node,
                Constraints::tight(Size {
                    width: rect.width.max(0.0),
                    height: rect.height.max(0.0),
                }),
            );
            cx.set_position(
                node,
                Point {
                    x: rect.x,
                    y: rect.y,
                },
            );
        }
        constraints.constrain(natural)
    }
}
