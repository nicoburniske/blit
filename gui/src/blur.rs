use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, Size};

blit::builder! {
    /// blurs previously painted content behind the drawing area
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct BackdropBlur {
        new(radius: f32),
    }
}

impl<C> Atom<C> for BackdropBlur
where
    C: Paint<Self, Scalar = f32>,
{
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        cx.context_mut().paint(area, *self);
    }
}
