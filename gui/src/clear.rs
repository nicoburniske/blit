use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, Size};

#[derive(Clone, Copy, Default)]
pub struct Clear;

impl<C: Paint<Clear, Scalar = f32>> Atom<C> for Clear {
    fn measure(&self, _: &mut AtomCx<'_, C>, bounds: Constraints<f32>) -> Size<f32> {
        bounds.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        cx.context_mut().paint(area, *self);
    }
}
