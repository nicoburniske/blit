use blit::{Atom, AtomCx, Constraints, Context, Rect, Sides, Size};

/// insets everything inserted after it on its node
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pad<T>(pub Sides<T>);

impl<T> Pad<T> {
    pub fn new(padding: impl Into<Sides<T>>) -> Self {
        Self(padding.into())
    }
}

impl<C: Context> Atom<C> for Pad<C::Scalar> {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<C::Scalar>) -> Size<C::Scalar> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, _: &mut AtomCx<'_, C>, _: Rect<C::Scalar>) {}

    fn inset(&self) -> Sides<C::Scalar> {
        self.0
    }
}
