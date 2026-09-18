use blit::{Constraints, LayoutCx, Platform, Point, Rect, Size};

/// places children in exact supplied local rectangles
///
/// rectangles bypass layout resolution and must match the frame coordinate grid
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout;

pub fn layout() -> Layout {
    Layout
}

impl<P: Platform> blit::Layout<P> for Layout {
    type Item = Rect;

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, bounds: Constraints) -> Size {
        let mut natural = Size::ZERO;
        for child in cx.children() {
            let rect = cx.item(child);
            natural.width = natural.width.max((rect.x + rect.width.max(0.0)).max(0.0));
            natural.height = natural.height.max((rect.y + rect.height.max(0.0)).max(0.0));
            cx.layout_child(child, Constraints::tight(rect.size().max(Size::ZERO)));
            cx.set_child_position(child, Point::new(rect.x, rect.y));
        }
        bounds.constrain(natural)
    }

    fn override_size(&self, item: &mut Rect, width: Option<f32>, height: Option<f32>) -> bool {
        if let Some(width) = width {
            item.width = width;
        }
        if let Some(height) = height {
            item.height = height;
        }
        true
    }
}
