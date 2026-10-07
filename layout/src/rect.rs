use blit::{Constraints, Context, LayoutCx, Point, Rect, Size};

/// places children in exact supplied local rectangles
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout;

pub fn new() -> Layout {
    Layout
}

impl<C: Context> blit::Layout<C> for Layout {
    type Item = Rect<C::Scalar>;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        let mut natural = Size::ZERO;
        for child in cx.children() {
            let rect = *cx.item(child);
            let size = rect.size().max(Size::ZERO);
            let size = cx.layout_child(child, Constraints::tight(size));
            let position = Point::new(rect.x, rect.y);
            cx.set_child_position(child, position);
            natural = natural.max(Size::new(position.x + size.width, position.y + size.height));
        }
        bounds.constrain(natural)
    }
}
