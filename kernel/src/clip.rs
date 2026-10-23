use crate::{Context, Point, Rect, Scalar};

/// declares clip geometry and applies the matching backend clip
pub trait Clip<C: Context>: 'static {
    /// returns geometry for culling and hit testing
    fn shape(&self, area: Rect<C::Scalar>) -> ClipShape<C::Scalar>;

    fn push(&self, context: &mut C, area: Rect<C::Scalar>);

    fn pop(&self, context: &mut C);
}

/// resolved clip geometry in frame coordinates
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipShape<T> {
    /// inherits ancestor clips without invoking backend callbacks
    Unbounded,
    Rect(Rect<T>),
    /// radius is clamped to half the shorter side
    RoundedRect {
        rect: Rect<T>,
        radius: T,
    },
    /// nonpositive radii are empty
    Circle {
        center: Point<T>,
        radius: T,
    },
}

impl<C: Context> Clip<C> for () {
    fn shape(&self, _: Rect<C::Scalar>) -> ClipShape<C::Scalar> {
        ClipShape::Unbounded
    }

    fn push(&self, _: &mut C, _: Rect<C::Scalar>) {}

    fn pop(&self, _: &mut C) {}
}

impl<T: Scalar> ClipShape<T> {
    pub fn contains(self, point: Point<T>) -> bool {
        match self {
            Self::Unbounded => true,
            Self::Rect(rect) => rect.contains(point),
            Self::RoundedRect { rect, radius } => {
                if !rect.contains(point) {
                    return false;
                }
                let radius = radius
                    .max(T::ZERO)
                    .to_f32()
                    .min(rect.width.min(rect.height).to_f32() * 0.5);
                let x = (point.x - rect.x).min(rect.x.endpoint(rect.width) - point.x).to_f32();
                let y = (point.y - rect.y).min(rect.y.endpoint(rect.height) - point.y).to_f32();
                let x = (radius - x).max(0.0);
                let y = (radius - y).max(0.0);
                x * x + y * y <= radius * radius
            }
            Self::Circle { center, radius } => {
                if radius <= T::ZERO
                    || point.x < center.x.endpoint(-radius)
                    || point.y < center.y.endpoint(-radius)
                    || point.x >= center.x.endpoint(radius)
                    || point.y >= center.y.endpoint(radius)
                {
                    return false;
                }
                let x = (point.x - center.x).to_f32();
                let y = (point.y - center.y).to_f32();
                let radius = radius.to_f32();
                x * x + y * y <= radius * radius
            }
        }
    }
}
