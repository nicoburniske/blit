pub type LogicalPoint = Point;
pub type LogicalRect = Rect;
pub type LogicalSize = Size;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn max(self, other: Self) -> Self {
        Self {
            width: self.width.max(other.width),
            height: self.height.max(other.height),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x + self.width
            && point.y < self.y + self.height
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > x && bottom > y).then_some(Self::new(x, y, right - x, bottom - y))
    }

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhysicalSize {
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl PhysicalRect {
    pub fn to_logical(self, scale: Scale2) -> Rect {
        Rect {
            x: self.x as f32 / scale.x,
            y: self.y as f32 / scale.y,
            width: self.width as f32 / scale.x,
            height: self.height as f32 / scale.y,
        }
    }

    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x.saturating_add(self.width)
            && y < self.y.saturating_add(self.height)
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .min(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .min(other.y.saturating_add(other.height));
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    pub fn touches(self, other: Self) -> bool {
        self.x <= other.x.saturating_add(other.width)
            && other.x <= self.x.saturating_add(self.width)
            && self.y <= other.y.saturating_add(other.height)
            && other.y <= self.y.saturating_add(self.height)
    }

    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: self
                .x
                .saturating_add(self.width)
                .max(other.x.saturating_add(other.width))
                - x,
            height: self
                .y
                .saturating_add(self.height)
                .max(other.y.saturating_add(other.height))
                - y,
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

crate::builder! {
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
    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn x(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            ..Self::all(0.0)
        }
    }

    pub const fn y(value: f32) -> Self {
        Self {
            top: value,
            bottom: value,
            ..Self::all(0.0)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min: Size,
    pub max: Size,
}

impl Constraints {
    pub const fn loose(max: Size) -> Self {
        Self {
            min: Size::ZERO,
            max,
        }
    }

    pub const fn tight(size: Size) -> Self {
        Self {
            min: size,
            max: size,
        }
    }

    pub fn constrain(self, size: Size) -> Size {
        Size {
            width: size.width.clamp(self.min.width, self.max.width),
            height: size.height.clamp(self.min.height, self.max.height),
        }
    }
}
