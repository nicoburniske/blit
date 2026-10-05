use blit::{Constraints, LayoutCx, Point, Rect, Size};

use crate::{Context, layout_child};

/// places children in exact supplied local rectangles
///
/// supplied rectangles bypass the layout coordinate policy
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout;

pub fn new() -> Layout {
    Layout
}

impl<C: Context> blit::Layout<C> for Layout {
    type Item = Rect;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> Size {
        let mut natural = Size::ZERO;
        for child in cx.children() {
            let rect = cx.item(child);
            let size = layout_child(cx, child, Constraints::tight(rect.size().max(Size::ZERO)));
            cx.set_child_position(child, Point::new(rect.x, rect.y));
            natural.width = natural.width.max((rect.x + size.width).max(0.0));
            natural.height = natural.height.max((rect.y + size.height).max(0.0));
        }
        bounds.constrain(natural)
    }
}
