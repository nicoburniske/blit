use blit::{Clip, ClipShape, LogicalRect};

use crate::{
    TextSystem,
    display_list::{CustomCommand, arena::Writer},
    style::BorderRadius,
};

/// records custom command data built in the frame arena
pub trait PaintCustom<T: CustomCommand>: blit::Context<Scalar = f32> {
    fn paint_with(&mut self, bounds: LogicalRect, build: impl FnOnce(&mut Writer<'_>) -> T);
}

pub trait TextContext: blit::Context<Scalar = f32> {
    fn text(&mut self) -> &mut TextSystem;
}

pub trait ClipContext: blit::Context<Scalar = f32> {
    fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius);
    fn pop_clip(&mut self);
}

#[derive(Clone, Copy)]
pub struct BoundsClip;

impl<C: ClipContext> Clip<C> for BoundsClip {
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
