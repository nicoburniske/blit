use blit::{Clip, ClipShape, LogicalRect};

use crate::{Context, style::BorderRadius};

#[derive(Clone, Copy)]
pub struct BoundsClip;

impl<C: Context> Clip<C> for BoundsClip {
    fn shape(&self, area: LogicalRect) -> ClipShape<f32> {
        ClipShape::Rect(area)
    }

    fn push(&self, context: &mut C, area: LogicalRect) {
        context.push_clip(area, BorderRadius::default());
    }

    fn pop(&self, context: &mut C) {
        context.pop_clip();
    }
}
