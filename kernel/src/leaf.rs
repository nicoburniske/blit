use crate::{
    geometry::{Constraints, Rect, Size},
    renderer::Renderer,
};

pub trait Leaf<R: Renderer>: Copy + 'static {
    fn measure(&self, renderer: &mut R, constraints: Constraints) -> Size;

    fn paint(&self, renderer: &mut R, area: Rect);
}
