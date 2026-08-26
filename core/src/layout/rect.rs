use crate::geometry::LogicalRect;

use super::{Axis, ItemScope, Layout, LayoutCx};

/// places direct children in supplied local rectangles
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RectLayout;

impl Layout for RectLayout {
    type Item = LogicalRect;
    type Scope<'a> = ItemScope<'a, Self>;

    fn measure(&self, cx: &LayoutCx<'_, Self::Item>, axis: Axis) -> Option<f32> {
        let mut measured: Option<f32> = None;
        for node in cx.children() {
            if cx.is_in_flow(node) {
                let rect = cx.item(node);
                let edge = match axis {
                    Axis::Horizontal => rect.x + rect.width.max(0.0),
                    Axis::Vertical => rect.y + rect.height.max(0.0),
                }
                .max(0.0);
                measured = Some(measured.map_or(edge, |measured| measured.max(edge)));
            }
        }
        measured
    }

    fn place(&self, cx: &mut LayoutCx<'_, Self::Item>, axis: Axis) {
        let parent = cx.rect();
        for node in cx.children() {
            if cx.is_in_flow(node) {
                let rect = cx.item(node);
                let (position, size) = match axis {
                    Axis::Horizontal => (parent.x + rect.x, rect.width),
                    Axis::Vertical => (parent.y + rect.y, rect.height),
                };
                cx.set_axis(node, axis, position, size.max(0.0));
            }
        }
    }
}
