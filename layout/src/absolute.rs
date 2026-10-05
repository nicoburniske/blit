use blit::{Axis, Constraints, LayoutCx, NodeTarget, Point, Size, Ui};

use crate::{Context, resolve_sizing, sizing_range};

pub fn place<L>(inner: L) -> Layout<L> {
    Layout::new(inner)
}

blit::builder! {
    /// positions a layout outside its parent's flow
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<L> {
        new(inner: L),
        target: NodeTarget = NodeTarget::Parent,
        target_anchor: Anchor = Anchor::TopLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        offset: Point = Point::ZERO,
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: f32, max: f32 },
    Fixed(f32),
    Percent(f32),
}

impl Sizing {
    pub const fn fit() -> Self {
        Self::fit_range(0.0, f32::INFINITY)
    }

    pub const fn fit_range(min: f32, max: f32) -> Self {
        Self::Fit { min, max }
    }

    pub const fn fixed(size: f32) -> Self {
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

impl<C: Context, L: blit::Layout<C>> blit::Layout<C> for Layout<L> {
    type Item = L::Item;

    fn on_insert<'a>(&self, ui: Ui<'a, C>) -> Ui<'a, C> {
        self.inner.on_insert(ui).relative(self.target)
    }

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> Size {
        let available = cx.size(cx.parent());
        let range = |sizing, axis, available| {
            let sizing = match sizing {
                Sizing::Fit { min, max } => crate::Sizing::Fit { min, max },
                Sizing::Fixed(size) => crate::Sizing::Fixed(size),
                Sizing::Percent(fraction) => crate::Sizing::Percent(fraction),
            };
            sizing_range::<C>(resolve_sizing(cx, cx.node(), axis, sizing), available)
        };
        let width = range(self.width, Axis::Horizontal, available.width);
        let height = range(self.height, Axis::Vertical, available.height);
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
        cx.set_position(Point::new(
            target.width * target_anchor.x - size.width * child_anchor.x + self.offset.x,
            target.height * target_anchor.y - size.height * child_anchor.y + self.offset.y,
        ));
        size
    }
}
