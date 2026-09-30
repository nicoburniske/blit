use blit::{Constraints, LogicalPoint, LogicalSize};

use crate::{Sizing, Unit, sizing_range};

blit::builder! {
    /// positions a layout relative to the node selected by `Ui::relative`
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<L, U: Unit> {
        new(inner: L),
        x: U::Offset = U::Offset::ZERO,
        y: U::Offset = U::Offset::ZERO,
        target_anchor: Anchor = Anchor::TopLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        width: Sizing<U> = Sizing::fit(),
        height: Sizing<U> = Sizing::fit(),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl<C, L: blit::Layout<C>, U: Unit> blit::Layout<C> for Layout<L, U> {
    type Item = L::Item;

    fn intrinsic(
        &self,
        cx: &mut blit::MeasureCx<'_, C, Self::Item>,
        query: blit::IntrinsicQuery,
    ) -> blit::IntrinsicSize {
        let (main, cross) = crate::flow_sizing(query.axis, self.width, self.height, (None, None));
        let query = blit::IntrinsicQuery {
            axis: query.axis,
            cross: crate::intrinsic_cross(cross, query.cross),
        };
        let size = if let Sizing::Fixed(value) = main {
            blit::IntrinsicSize {
                min: value.max(0.0),
                preferred: value.max(0.0),
            }
        } else {
            self.inner.intrinsic(cx, query)
        };
        crate::intrinsic_range(main, size)
    }

    fn layout(&self, cx: &mut blit::LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> LogicalSize {
        let containing = cx.size(cx.parent());
        let range = |sizing: Sizing<U>, available: f32| {
            let sizing = sizing.into_float();
            let available = U::round(available);
            if matches!(sizing, Sizing::Grow { .. }) {
                let size = sizing.clamp(available);
                (size, size)
            } else {
                sizing_range::<U>(sizing, available)
            }
        };
        let width = range(self.width, containing.width);
        let height = range(self.height, containing.height);
        let size = self.inner.layout(
            cx,
            Constraints {
                min: bounds.constrain(LogicalSize::new(U::round(width.0), U::round(height.0))),
                max: bounds.constrain(LogicalSize::new(U::round(width.1), U::round(height.1))),
            },
        );
        let target = cx.size(cx.relative());
        let anchor = |anchor| match anchor {
            Anchor::TopLeft => LogicalPoint::new(0.0, 0.0),
            Anchor::Top => LogicalPoint::new(0.5, 0.0),
            Anchor::TopRight => LogicalPoint::new(1.0, 0.0),
            Anchor::Left => LogicalPoint::new(0.0, 0.5),
            Anchor::Center => LogicalPoint::new(0.5, 0.5),
            Anchor::Right => LogicalPoint::new(1.0, 0.5),
            Anchor::BottomLeft => LogicalPoint::new(0.0, 1.0),
            Anchor::Bottom => LogicalPoint::new(0.5, 1.0),
            Anchor::BottomRight => LogicalPoint::new(1.0, 1.0),
        };
        let target_anchor = anchor(self.target_anchor);
        let child_anchor = anchor(self.child_anchor);
        cx.set_position(
            cx.node(),
            LogicalPoint::new(
                U::round(
                    U::round(target.width) * target_anchor.x - U::round(size.width) * child_anchor.x
                        + self.x.into_float(),
                ),
                U::round(
                    U::round(target.height) * target_anchor.y - U::round(size.height) * child_anchor.y
                        + self.y.into_float(),
                ),
            ),
        );
        bounds.constrain(size)
    }
}
