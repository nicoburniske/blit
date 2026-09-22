use blit::{Constraints, LayoutCx, Point, Rect, Size};

/// places children in exact supplied local rectangles
///
/// rectangles bypass layout resolution and must match the frame coordinate grid
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout;

pub fn layout() -> Layout {
    Layout
}

impl<C> blit::Layout<C> for Layout {
    type Item = Rect;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> Size {
        let mut natural = Size::ZERO;
        for child in cx.children() {
            let rect = cx.item(child);
            let size = cx.layout_child(child, Constraints::tight(rect.size().max(Size::ZERO)));
            natural.width = natural.width.max((rect.x + size.width).max(0.0));
            natural.height = natural.height.max((rect.y + size.height).max(0.0));
            cx.set_child_position(child, Point::new(rect.x, rect.y));
        }
        bounds.constrain(natural)
    }
}
