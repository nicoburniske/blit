pub use crate::frame::ClipCx;

use crate::{
    geometry::{Point, Rect},
    renderer::Renderer,
};

pub trait ClipCommand<R: Renderer> {
    fn push(self, renderer: &mut R);

    fn pop(renderer: &mut R);
}

pub trait Clip<R: Renderer>: Copy + 'static {
    fn bounds(&self, area: Rect) -> Rect;

    fn contains(&self, area: Rect, point: Point) -> bool;

    fn push(&self, cx: &mut ClipCx<'_, R>, area: Rect);

    fn pop(&self, cx: &mut ClipCx<'_, R>);
}
