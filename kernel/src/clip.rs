pub use crate::frame::ClipCx;

use crate::{geometry::Rect, renderer::Renderer};

pub trait ClipCommand<R: Renderer> {
    fn push(self, renderer: &mut R);

    fn pop(renderer: &mut R);
}

pub trait Clip<R: Renderer>: Copy + 'static {
    fn push(&self, cx: ClipCx<'_, R>, area: Rect);

    fn pop(&self, cx: ClipCx<'_, R>);
}
