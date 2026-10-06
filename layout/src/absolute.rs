use blit::{Constraints, Context, LayoutCx, NodeTarget, Point, Scalar, Size, Ui};

use super::sizing_range;

pub fn place<L, T: Scalar>(inner: L) -> Layout<L, T> {
    Layout::new(inner)
}

blit::builder! {
    /// positions a layout outside its parent's flow with x and y offsets from its anchors
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<L, T: Scalar> {
        new(inner: L),
        #[into]
        target: NodeTarget = NodeTarget::Parent,
        target_anchor: Anchor = Anchor::TopLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        x: T = T::ZERO,
        y: T = T::ZERO,
        width: Sizing<T> = Sizing::fit(),
        height: Sizing<T> = Sizing::fit(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing<T> {
    Fit { min: T, max: T },
    Fixed(T),
    Percent(f32),
}

impl<T: Scalar> Sizing<T> {
    pub const fn fit() -> Self {
        Self::fit_range(T::ZERO, T::UNBOUNDED)
    }

    pub const fn fit_range(min: T, max: T) -> Self {
        Self::Fit { min, max }
    }

    pub const fn fixed(size: T) -> Self {
        Self::Fixed(size)
    }

    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }

    pub const fn full() -> Self {
        Self::percent(1.0)
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

impl<C: Context<Scalar = T>, L: blit::Layout<C>, T: Scalar> blit::Layout<C> for Layout<L, T> {
    type Item = L::Item;

    fn on_insert<'a>(&self, ui: Ui<'a, C>) -> Ui<'a, C> {
        self.inner.on_insert(ui).relative(self.target)
    }

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<T>) -> Size<T> {
        let available = cx.size(cx.parent());
        let range = |sizing, available| {
            let sizing = match sizing {
                Sizing::Fit { min, max } => super::Sizing::Fit { min, max },
                Sizing::Fixed(size) => super::Sizing::Fixed(size),
                Sizing::Percent(fraction) => super::Sizing::Percent(fraction),
            };
            sizing_range(sizing, available)
        };
        let width = range(self.width, available.width);
        let height = range(self.height, available.height);
        let size = self.inner.layout(
            cx,
            Constraints {
                min: bounds.constrain(Size::new(width.0, height.0)),
                max: bounds.constrain(Size::new(width.1, height.1)),
            },
        );
        let anchor = |anchor| match anchor {
            Anchor::TopLeft => Point::new(0.0, 0.0),
            Anchor::Top => Point::new(0.5, 0.0),
            Anchor::TopRight => Point::new(1.0, 0.0),
            Anchor::Left => Point::new(0.0, 0.5),
            Anchor::Center => Point::new(0.5, 0.5),
            Anchor::Right => Point::new(1.0, 0.5),
            Anchor::BottomLeft => Point::new(0.0, 1.0),
            Anchor::Bottom => Point::new(0.5, 1.0),
            Anchor::BottomRight => Point::new(1.0, 1.0),
        };
        let target = cx.size(cx.relative());
        let target_anchor = anchor(self.target_anchor);
        let child_anchor = anchor(self.child_anchor);
        let offset = |target, child, target_anchor, child_anchor| {
            let start =
                if target_anchor == 1.0 { target } else { T::ZERO } - if child_anchor == 1.0 { child } else { T::ZERO };
            let end =
                if target_anchor == 0.0 { T::ZERO } else { target } - if child_anchor == 0.0 { T::ZERO } else { child };
            start.lerp(end, 0.5)
        };
        cx.set_position(Point::new(
            offset(target.width, size.width, target_anchor.x, child_anchor.x) + self.x,
            offset(target.height, size.height, target_anchor.y, child_anchor.y) + self.y,
        ));
        size
    }
}
