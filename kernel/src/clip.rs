use crate::{geometry::Rect, renderer::Renderer};

pub trait Clip<R: Renderer>: Copy + 'static {
    fn push(&self, renderer: &mut R, area: Rect);

    fn pop(&self, renderer: &mut R);
}
