pub use crate::frame::{MeasureCx, PaintCx};

use crate::{
    geometry::{Constraints, Rect, Size},
    renderer::Renderer,
};

pub trait Leaf<R: Renderer>: Copy + 'static {
    fn measure(&self, cx: MeasureCx<'_, R>, constraints: Constraints) -> Size;

    fn paint(&self, cx: PaintCx<'_, R>, area: Rect);
}
