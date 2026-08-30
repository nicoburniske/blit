pub use crate::frame::{MeasureCx, PaintCx};

use crate::{
    geometry::{Constraints, Rect, Size},
    renderer::Renderer,
};

pub trait Leaf<R: Renderer>: Copy + 'static {
    fn measure(&self, cx: &mut MeasureCx<'_, R>, constraints: Constraints) -> Size;

    fn paint(&self, cx: &mut PaintCx<'_, R>, area: Rect);
}
