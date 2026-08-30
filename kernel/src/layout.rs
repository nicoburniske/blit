pub use crate::frame::{Children, LayoutCx};

use crate::{
    geometry::{Constraints, Size},
    renderer::Renderer,
};

pub trait Layout<R: Renderer>: Copy + 'static {
    type Item: Copy + 'static;

    fn layout(&self, cx: LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size;
}
