pub type LogicalPoint = Point<f32>;
pub type LogicalRect = Rect<f32>;
pub type LogicalSize = Size<f32>;
pub type PhysicalPoint = Point<i32>;
pub type PhysicalRect = Rect<i32>;
pub type PhysicalSize = Size<i32>;

crate::builder! {
    #[const]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Point<T: Copy> {
        new(x: T, y: T),
    }
}

crate::builder! {
    #[const]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Size<T: Copy> {
        new(width: T, height: T),
    }
}

impl<T: Copy> Size<T> {
    pub const fn uniform(size: T) -> Self {
        Self::new(size, size)
    }
}

impl<T: Coordinate> Point<T> {
    pub const ZERO: Self = Self { x: T::ZERO, y: T::ZERO };
}

impl<T: Coordinate> Size<T> {
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

impl<T: Copy + std::ops::Add<Output = T>> std::ops::Add for Size<T> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.width + other.width, self.height + other.height)
    }
}

impl<T: Copy + std::ops::Sub<Output = T>> std::ops::Sub for Size<T> {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.width - other.width, self.height - other.height)
    }
}

impl<T: Copy + std::ops::Mul<Output = T>> std::ops::Mul<T> for Size<T> {
    type Output = Self;

    fn mul(self, scale: T) -> Self {
        Self::new(self.width * scale, self.height * scale)
    }
}

pub trait Coordinate: Copy + PartialOrd + std::ops::Sub<Output = Self> {
    const ZERO: Self;

    fn endpoint(self, extent: Self) -> Self;
    fn min(self, other: Self) -> Self;
    fn max(self, other: Self) -> Self;
}

impl Coordinate for f32 {
    const ZERO: Self = 0.0;

    #[inline]
    fn endpoint(self, extent: Self) -> Self {
        self + extent
    }

    #[inline]
    fn min(self, other: Self) -> Self {
        f32::min(self, other)
    }

    #[inline]
    fn max(self, other: Self) -> Self {
        f32::max(self, other)
    }
}

impl Coordinate for i32 {
    const ZERO: Self = 0;

    #[inline]
    fn endpoint(self, extent: Self) -> Self {
        self.saturating_add(extent)
    }

    #[inline]
    fn min(self, other: Self) -> Self {
        Ord::min(self, other)
    }

    #[inline]
    fn max(self, other: Self) -> Self {
        Ord::max(self, other)
    }
}

crate::builder! {
    #[const]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Rect<T: Copy> {
        new(x: T, y: T, width: T, height: T),
    }
}

impl<T: Copy> Rect<T> {
    pub const fn size(self) -> Size<T> {
        Size::new(self.width, self.height)
    }
}

impl<T: Coordinate> Rect<T> {
    pub fn contains(self, point: Point<T>) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x.endpoint(self.width)
            && point.y < self.y.endpoint(self.height)
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
        Self {
            x,
            y,
            width: self.x.endpoint(self.width).max(other.x.endpoint(other.width)) - x,
            height: self.y.endpoint(self.height).max(other.y.endpoint(other.height)) - y,
        }
    }
}

impl Rect<f32> {
    pub fn to_physical(self, scale: Scale2) -> PhysicalRect {
        let x = (self.x * scale.x).floor() as i32;
        let y = (self.y * scale.y).floor() as i32;
        let right = ((self.x + self.width) * scale.x).ceil() as i32;
        let bottom = ((self.y + self.height) * scale.y).ceil() as i32;
        PhysicalRect {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        }
    }
}

impl Rect<i32> {
    pub const fn to_logical(self, scale: Scale2) -> LogicalRect {
        Rect {
            x: self.x as f32 / scale.x,
            y: self.y as f32 / scale.y,
            width: self.width as f32 / scale.x,
            height: self.height as f32 / scale.y,
        }
    }

    pub const fn area(self) -> i64 {
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

crate::builder! {
    #[const]
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Sides {
        new(),
        top: f32 = 0.0,
        right: f32 = 0.0,
        bottom: f32 = 0.0,
        left: f32 = 0.0,
    }
}

impl Sides {
    #[inline]
    pub const fn size(self) -> LogicalSize {
        LogicalSize::new(self.left + self.right, self.top + self.bottom)
    }

    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn xy(x: f32, y: f32) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    pub const fn x(value: f32) -> Self {
        Self::xy(value, 0.0)
    }

    pub const fn y(value: f32) -> Self {
        Self::xy(0.0, value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min: LogicalSize,
    pub max: LogicalSize,
}

impl Constraints {
    pub const fn loose(max: LogicalSize) -> Self {
        Self {
            min: LogicalSize::ZERO,
            max,
        }
    }

    pub const fn tight(size: LogicalSize) -> Self {
        Self { min: size, max: size }
    }

    #[inline]
    pub fn constrain(self, size: LogicalSize) -> LogicalSize {
        LogicalSize {
            width: size.width.clamp(self.min.width, self.max.width),
            height: size.height.clamp(self.min.height, self.max.height),
        }
    }

    #[inline]
    pub fn shrink(self, amount: LogicalSize) -> Self {
        Self {
            min: (self.min - amount).max(LogicalSize::ZERO),
            max: (self.max - amount).max(LogicalSize::ZERO),
        }
    }
}
