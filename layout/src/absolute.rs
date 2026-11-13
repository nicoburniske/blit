use blit::{Axis, Constraints, Context, PlaceCx, Point, Rect, Scalar, Size};

use super::sizing_range;

pub fn place<T: Scalar>() -> Place<T> {
    Place::new()
}

blit::builder! {
    /// sizes against the visual parent and positions with x and y offsets from anchors on the target
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Place<T: Scalar> {
        new(),
        target_anchor: Anchor = Anchor::TopLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        x: T = T::ZERO,
        y: T = T::ZERO,
        width: Sizing<T> = Sizing::fit(),
        height: Sizing<T> = Sizing::fit(),
        /// mirrors anchors and offset on an axis that overflows the screen when the mirror fits
        flip: bool = false,
        /// shifts into the screen after flipping
        clamp: bool = false,
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

impl<C: Context<Scalar = T>, T: Scalar> blit::Place<C> for Place<T> {
    fn place(&self, cx: &mut PlaceCx<'_, C>, target: Rect<T>) -> Point<T> {
        let range = |sizing, available| {
            let sizing = match sizing {
                Sizing::Fit { min, max } => crate::Sizing::Fit { min, max },
                Sizing::Fixed(size) => crate::Sizing::Fixed(size),
                Sizing::Percent(fraction) => crate::Sizing::Percent(fraction),
            };
            sizing_range(sizing, available)
        };
        let parent = cx.parent();
        let width = range(self.width, parent.width);
        let height = range(self.height, parent.height);
        let size = cx.resolve(Constraints {
            min: Size::new(width.0, height.0),
            max: Size::new(width.1, height.1),
        });
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
        let target_anchor = anchor(self.target_anchor);
        let child_anchor = anchor(self.child_anchor);
        let screen = cx.screen();
        let axis = |axis: Axis, offset: T, target_anchor: f32, child_anchor: f32| {
            let (start, screen_start) = match axis {
                Axis::Horizontal => (target.x, screen.x),
                Axis::Vertical => (target.y, screen.y),
            };
            let extent = axis.extent(target.size());
            let screen_end = screen_start.endpoint(axis.extent(screen.size()));
            let position = |size: T, target_anchor: f32, child_anchor: f32, offset: T| {
                let from = if target_anchor == 1.0 { extent } else { T::ZERO }
                    - if child_anchor == 1.0 { size } else { T::ZERO };
                let to = if target_anchor == 0.0 { T::ZERO } else { extent }
                    - if child_anchor == 0.0 { T::ZERO } else { size };
                start + from.lerp(to, 0.5) + offset
            };
            let size = axis.extent(size);
            let overflows = |target_anchor, child_anchor, offset| {
                let position = position(size, target_anchor, child_anchor, offset);
                position < screen_start || position.endpoint(size) > screen_end
            };
            let mut result = position(size, target_anchor, child_anchor, offset);
            if self.flip
                && overflows(target_anchor, child_anchor, offset)
                && !overflows(1.0 - target_anchor, 1.0 - child_anchor, T::ZERO - offset)
            {
                result = position(size, 1.0 - target_anchor, 1.0 - child_anchor, T::ZERO - offset);
            }
            if self.clamp {
                result = result.min(screen_end - size).max(screen_start);
            }
            result
        };
        Point::new(
            axis(Axis::Horizontal, self.x, target_anchor.x, child_anchor.x),
            axis(Axis::Vertical, self.y, target_anchor.y, child_anchor.y),
        )
    }
}
