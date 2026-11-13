use blit::{Constraints, Context, LayoutCx, Point, Scalar, Sides, Size};

/// insets flow children while keeping the node's atoms on its outer box
pub fn layout<C: Context, I: 'static>(
    cx: &mut LayoutCx<'_, C, I>,
    bounds: Constraints<C::Scalar>,
    padding: Sides<C::Scalar>,
    content: impl FnOnce(&mut LayoutCx<'_, C, I>, Constraints<C::Scalar>) -> Size<C::Scalar>,
) -> Size<C::Scalar> {
    if padding == Sides::all(C::Scalar::ZERO) {
        return content(cx, bounds);
    }
    for side in [padding.left, padding.right, padding.top, padding.bottom] {
        assert!(
            side.is_finite() && side >= C::Scalar::ZERO,
            "padding must be finite and nonnegative"
        );
    }
    assert!(
        padding.left.endpoint(padding.right).is_finite() && padding.top.endpoint(padding.bottom).is_finite(),
        "padding sums must be finite"
    );
    let leading = Point::new(padding.left.min(bounds.max.width), padding.top.min(bounds.max.height));
    let padding = padding.size();
    let size = content(&mut cx.offset(leading), bounds.shrink(padding));
    bounds.constrain(Size::new(
        size.width.endpoint(padding.width),
        size.height.endpoint(padding.height),
    ))
}
