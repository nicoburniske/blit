use core::{
    fmt::Debug,
    ops::{Add, AddAssign, Mul, Neg, Sub},
};

use crate::math;

/// coordinate arithmetic and snapping used by [`Context::Scalar`](crate::Context::Scalar)
///
/// - floats use continuous coordinates and preserve fractions
/// - integers use discrete coordinates and snap to whole units
pub trait Scalar:
    Copy + Default + Debug + PartialOrd + Add<Output = Self> + AddAssign + Sub<Output = Self> + Neg<Output = Self> + 'static
{
    /// the additive identity
    const ZERO: Self;
    /// represents no upper size limit
    /// must compare greater than every finite extent
    const UNBOUNDED: Self;

    /// converts a float using this coordinate system's snapping rule
    /// discrete coordinates round to their grid and saturate at their limits
    fn from_f32(value: f32) -> Self;
    /// converts to a float which may lose precision
    fn to_f32(self) -> f32;
    /// signed distance to another coordinate without snapping either endpoint
    fn delta(self, end: Self) -> f32 {
        end.to_f32() - self.to_f32()
    }
    /// applies a continuous offset before snapping the result
    fn offset(self, amount: f32) -> Self {
        Self::from_f32(self.to_f32() + amount)
    }
    /// resolves shares using one cursor initialized to zero for the whole group
    /// discrete coordinates advance the cursor and subtract consecutive snapped boundaries
    /// e.g. three equal shares of ten cells snap to `3, 4, 3`
    /// continuous coordinates return `share` directly to avoid cancellation
    fn allocate(cursor: &mut f32, share: f32) -> Self;
    /// multiplies by a count with saturation for bounded integer coordinates
    fn repeat(self, count: usize) -> Self;
    /// evaluates `self + (end - self) * amount` with coordinate snapping
    /// `amount` is not clamped so values outside zero to one extrapolate
    #[inline]
    fn lerp(self, end: Self, amount: f32) -> Self {
        self.offset(self.delta(end) * amount)
    }
    /// returns the lesser coordinate
    #[inline]
    fn min(self, other: Self) -> Self {
        if self < other { self } else { other }
    }
    /// returns the greater coordinate
    #[inline]
    fn max(self, other: Self) -> Self {
        if self > other { self } else { other }
    }
    /// clamps to an inclusive range and panics if `min <= max` is false
    #[inline]
    fn clamp(self, min: Self, max: Self) -> Self {
        assert!(min <= max, "min > max");
        self.max(min).min(max)
    }
    /// adds an extent with saturation for bounded integer coordinates
    fn endpoint(self, extent: Self) -> Self;

    /// whether this is finite and differs from the unbounded sentinel
    fn is_finite(self) -> bool;
}

impl Scalar for f32 {
    const ZERO: Self = 0.0;
    const UNBOUNDED: Self = f32::INFINITY;

    #[inline]
    fn from_f32(value: f32) -> Self {
        value
    }
    #[inline]
    fn to_f32(self) -> f32 {
        self
    }
    #[inline]
    fn allocate(_: &mut f32, share: f32) -> Self {
        share
    }
    #[inline]
    fn repeat(self, count: usize) -> Self {
        self * count as Self
    }
    #[inline]
    fn endpoint(self, extent: Self) -> Self {
        self + extent
    }
    #[inline]
    fn is_finite(self) -> bool {
        f32::is_finite(self)
    }
}

impl Scalar for i32 {
    const ZERO: Self = 0;
    const UNBOUNDED: Self = i32::MAX;

    #[inline]
    fn from_f32(value: f32) -> Self {
        math::round(value as f64)
    }
    #[inline]
    fn to_f32(self) -> f32 {
        self as f32
    }
    #[inline]
    fn delta(self, end: Self) -> f32 {
        (end as f64 - self as f64) as f32
    }
    #[inline]
    fn offset(self, amount: f32) -> Self {
        math::round(self as f64 + amount as f64)
    }
    #[inline]
    fn allocate(cursor: &mut f32, share: f32) -> Self {
        let start = Self::from_f32(*cursor);
        *cursor += share;
        Self::from_f32(*cursor) - start
    }
    #[inline]
    fn repeat(self, count: usize) -> Self {
        let count = count.min(u32::MAX as usize) as i64;
        (self as i64 * count).clamp(Self::MIN as i64, Self::MAX as i64) as Self
    }
    #[inline]
    fn endpoint(self, extent: Self) -> Self {
        self.saturating_add(extent)
    }
    #[inline]
    fn is_finite(self) -> bool {
        self != Self::UNBOUNDED
    }
}

pub type LogicalPoint = Point<f32>;
pub type LogicalRect = Rect<f32>;
pub type LogicalSize = Size<f32>;
pub type PhysicalPoint = Point<i32>;
pub type PhysicalRect = Rect<i32>;
pub type PhysicalSize = Size<i32>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

impl<T> Point<T> {
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl<T: Scalar> Point<T> {
    pub const ZERO: Self = Self { x: T::ZERO, y: T::ZERO };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}

impl<T> Size<T> {
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl<T: Copy> Size<T> {
    pub const fn uniform(size: T) -> Self {
        Self::new(size, size)
    }
}

impl<T: Scalar> Size<T> {
    pub const ZERO: Self = Self {
        width: T::ZERO,
        height: T::ZERO,
    };

    pub fn max(self, other: Self) -> Self {
        Self {
            width: self.width.max(other.width),
            height: self.height.max(other.height),
        }
    }
}

impl<T: Add<Output = T>> Add for Size<T> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.width + other.width, self.height + other.height)
    }
}

impl<T: Sub<Output = T>> Sub for Size<T> {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.width - other.width, self.height - other.height)
    }
}

impl<T: Mul<U, Output = T>, U: Copy> Mul<U> for Size<T> {
    type Output = Self;

    fn mul(self, scale: U) -> Self {
        Self::new(self.width * scale, self.height * scale)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect<T> {
    pub x: T,
    pub y: T,
    pub width: T,
    pub height: T,
}

impl<T> Rect<T> {
    pub const fn new(x: T, y: T, width: T, height: T) -> Self {
        Self { x, y, width, height }
    }
}

impl<T: Copy> Rect<T> {
    pub const fn size(self) -> Size<T> {
        Size::new(self.width, self.height)
    }
}

impl<T: Scalar> Rect<T> {
    pub fn contains(self, point: Point<T>) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x.endpoint(self.width)
            && point.y < self.y.endpoint(self.height)
    }

    pub fn outset(self, sides: Sides<T>) -> Self {
        Self::new(
            self.x - sides.left,
            self.y - sides.top,
            self.width + sides.left + sides.right,
            self.height + sides.top + sides.bottom,
        )
    }

    pub fn inset(self, padding: Sides<T>) -> Self {
        Self::new(
            self.x + padding.left,
            self.y + padding.top,
            (self.width - padding.left - padding.right).max(T::ZERO),
            (self.height - padding.top - padding.bottom).max(T::ZERO),
        )
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.x.endpoint(self.width).min(other.x.endpoint(other.width));
        let bottom = self.y.endpoint(self.height).min(other.y.endpoint(other.height));
        (right > x && bottom > y).then_some(Self::new(x, y, right - x, bottom - y))
    }

    pub fn touches(self, other: Self) -> bool {
        self.x <= other.x.endpoint(other.width)
            && other.x <= self.x.endpoint(self.width)
            && self.y <= other.y.endpoint(other.height)
            && other.y <= self.y.endpoint(self.height)
    }

    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(
            x,
            y,
            self.x.endpoint(self.width).max(other.x.endpoint(other.width)) - x,
            self.y.endpoint(self.height).max(other.y.endpoint(other.height)) - y,
        )
    }
}

impl Rect<f32> {
    pub fn to_physical(self, scale: Scale2) -> PhysicalRect {
        let x = math::floor(self.x * scale.x);
        let y = math::floor(self.y * scale.y);
        let right = math::ceil((self.x + self.width) * scale.x);
        let bottom = math::ceil((self.y + self.height) * scale.y);
        PhysicalRect {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        }
    }
}

impl Rect<i32> {
    pub fn to_logical(self, scale: Scale2) -> Rect<f32> {
        Rect {
            x: self.x as f32 / scale.x,
            y: self.y as f32 / scale.y,
            width: self.width as f32 / scale.x,
            height: self.height as f32 / scale.y,
        }
    }

    pub fn area(self) -> i64 {
        self.width as i64 * self.height as i64
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale2 {
    pub x: f32,
    pub y: f32,
}

impl Scale2 {
    pub const IDENTITY: Self = Self { x: 1.0, y: 1.0 };

    pub const fn uniform(scale: f32) -> Self {
        Self { x: scale, y: scale }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Sides<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Copy + Default> Sides<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn x(value: T) -> Self {
        Self::xy(value, T::default())
    }

    pub fn y(value: T) -> Self {
        Self::xy(T::default(), value)
    }
}

impl<T: Copy> Sides<T> {
    pub const fn all(value: T) -> Self {
        Self::xy(value, value)
    }

    pub const fn xy(x: T, y: T) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    pub const fn top(mut self, value: T) -> Self {
        self.top = value;
        self
    }

    pub const fn right(mut self, value: T) -> Self {
        self.right = value;
        self
    }

    pub const fn bottom(mut self, value: T) -> Self {
        self.bottom = value;
        self
    }

    pub const fn left(mut self, value: T) -> Self {
        self.left = value;
        self
    }

    pub fn map<U>(self, mut map: impl FnMut(T) -> U) -> Sides<U> {
        Sides {
            top: map(self.top),
            right: map(self.right),
            bottom: map(self.bottom),
            left: map(self.left),
        }
    }
}

impl<T: Scalar> From<T> for Sides<T> {
    fn from(value: T) -> Self {
        Self::all(value)
    }
}

impl<T: Add<Output = T>> Add for Sides<T> {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self {
            top: self.top + other.top,
            right: self.right + other.right,
            bottom: self.bottom + other.bottom,
            left: self.left + other.left,
        }
    }
}

impl<T: Add<Output = T>> Sides<T> {
    #[inline]
    pub fn size(self) -> Size<T> {
        Size::new(self.left + self.right, self.top + self.bottom)
    }
}

crate::builder! {
    /// radii of a rounded rectangle
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Corners<T: Scalar> {
        new(),
        top_left: T = T::ZERO,
        top_right: T = T::ZERO,
        bottom_right: T = T::ZERO,
        bottom_left: T = T::ZERO,
    }
}

impl<T: Scalar> Corners<T> {
    pub const fn all(value: T) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }
}

impl<T: Scalar> From<T> for Corners<T> {
    fn from(value: T) -> Self {
        Self::all(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints<T> {
    pub min: Size<T>,
    pub max: Size<T>,
}

impl<T: Scalar> Constraints<T> {
    pub const fn loose(max: Size<T>) -> Self {
        Self { min: Size::ZERO, max }
    }

    pub const fn tight(size: Size<T>) -> Self {
        Self { min: size, max: size }
    }

    #[inline]
    pub fn constrain(self, size: Size<T>) -> Size<T> {
        Size {
            width: size.width.clamp(self.min.width, self.max.width),
            height: size.height.clamp(self.min.height, self.max.height),
        }
    }

    #[inline]
    pub fn shrink(self, amount: Size<T>) -> Self {
        Self {
            min: (self.min - amount).max(Size::ZERO),
            max: Size::new(
                if self.max.width == T::UNBOUNDED {
                    T::UNBOUNDED
                } else {
                    (self.max.width - amount.width).max(T::ZERO)
                },
                if self.max.height == T::UNBOUNDED {
                    T::UNBOUNDED
                } else {
                    (self.max.height - amount.height).max(T::ZERO)
                },
            ),
        }
    }
}
