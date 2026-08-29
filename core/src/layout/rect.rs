use crate::geometry::{LogicalPoint, LogicalRect, LogicalSize};

use super::{Constraints, ItemScope, Layout, LayoutCx};

/// places direct children in supplied local rectangles
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RectLayout;

impl Layout for RectLayout {
    type Item = LogicalRect;
    type Scope<'a> = ItemScope<'a, Self>;

    fn layout(&self, cx: &mut LayoutCx<'_, Self::Item>, constraints: Constraints) -> LogicalSize {
        let mut natural = LogicalSize::default();
        for node in cx.children() {
            let rect = cx.item(node);
            natural.width = natural.width.max((rect.x + rect.width.max(0.0)).max(0.0));
            natural.height = natural.height.max((rect.y + rect.height.max(0.0)).max(0.0));
            cx.layout_child(
                node,
                Constraints::tight(LogicalSize {
                    width: rect.width.max(0.0),
                    height: rect.height.max(0.0),
                }),
            );
            cx.set_position(
                node,
                LogicalPoint {
                    x: rect.x,
                    y: rect.y,
                },
            );
        }
        constraints.constrain(natural)
    }
}
