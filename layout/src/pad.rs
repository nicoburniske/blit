use blit::{Constraints, Context, LayoutCx, Point, Scalar, Sides, Size, Ui};

/// insets flow children while keeping this node's atoms on its outer box
///
/// absolute placement outside padding anchors the outer box
/// padding outside absolute placement anchors the inner content
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pad<L, T: Scalar> {
    inner: L,
    padding: Sides<T>,
}

pub fn pad<L, T: Scalar>(inner: L, padding: impl Into<Sides<T>>) -> Pad<L, T> {
    Pad {
        inner,
        padding: Sides::all(T::ZERO),
    }
    .pad(padding)
}

impl<L, T: Scalar> Pad<L, T> {
    pub fn pad(mut self, padding: impl Into<Sides<T>>) -> Self {
        let padding = padding.into();
        validate(padding);
        self.padding = padding;
        self
    }
}

impl<C: Context<Scalar = T>, L: blit::Layout<C>, T: Scalar> blit::Layout<C> for Pad<L, T> {
    type Item = L::Item;

    fn on_insert<'a>(&self, ui: Ui<'a, C>) -> Ui<'a, C> {
        self.inner.on_insert(ui)
    }

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<T>) -> Size<T> {
        layout(cx, bounds, self.padding, |cx, bounds| self.inner.layout(cx, bounds))
    }
}

pub fn layout<C: Context, I: 'static>(
    cx: &mut LayoutCx<'_, C, I>,
    bounds: Constraints<C::Scalar>,
    padding: Sides<C::Scalar>,
    content: impl FnOnce(&mut LayoutCx<'_, C, I>, Constraints<C::Scalar>) -> Size<C::Scalar>,
) -> Size<C::Scalar> {
    if padding == Sides::all(C::Scalar::ZERO) {
        return content(cx, bounds);
    }
    validate(padding);
    let leading = Point::new(padding.left.min(bounds.max.width), padding.top.min(bounds.max.height));
    let padding = padding.size();
    let size = content(&mut cx.offset(leading), bounds.shrink(padding));
    bounds.constrain(Size::new(
        size.width.endpoint(padding.width),
        size.height.endpoint(padding.height),
    ))
}

fn validate<T: Scalar>(padding: Sides<T>) {
    for side in [padding.left, padding.right, padding.top, padding.bottom] {
        assert!(
            side.is_finite() && side >= T::ZERO,
            "padding must be finite and nonnegative"
        );
    }
    assert!(
        padding.left.endpoint(padding.right).is_finite() && padding.top.endpoint(padding.bottom).is_finite(),
        "padding sums must be finite"
    );
}
