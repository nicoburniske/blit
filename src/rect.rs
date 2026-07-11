#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<T> {
    pub x: T,
    pub y: T,
    pub width: T,
    pub height: T,
}

pub type LogicalRect = Rect<f32>;
pub type PhysicalRect = Rect<i32>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LogicalInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl LogicalInsets {
    pub const fn uniform(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

impl LogicalRect {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    pub fn inset(self, insets: LogicalInsets) -> Self {
        Self {
            x: self.x + insets.left,
            y: self.y + insets.top,
            width: (self.width - insets.left - insets.right).max(0.0),
            height: (self.height - insets.top - insets.bottom).max(0.0),
        }
    }

    pub fn inset_top(mut self, amount: f32) -> Self {
        self.y += amount;
        self.height = (self.height - amount).max(0.0);
        self
    }

    pub fn inset_right(mut self, amount: f32) -> Self {
        self.width = (self.width - amount).max(0.0);
        self
    }

    pub fn inset_bottom(mut self, amount: f32) -> Self {
        self.height = (self.height - amount).max(0.0);
        self
    }

    pub fn inset_left(mut self, amount: f32) -> Self {
        self.x += amount;
        self.width = (self.width - amount).max(0.0);
        self
    }

    pub fn inset_x(mut self, amount: f32) -> Self {
        self.x += amount;
        self.width = (self.width - amount * 2.0).max(0.0);
        self
    }

    pub fn inset_y(mut self, amount: f32) -> Self {
        self.y += amount;
        self.height = (self.height - amount * 2.0).max(0.0);
        self
    }

    pub fn to_physical(self, scale_factor: f32) -> PhysicalRect {
        let x = (self.x * scale_factor).floor() as i32;
        let y = (self.y * scale_factor).floor() as i32;
        let right = ((self.x + self.width) * scale_factor).ceil() as i32;
        let bottom = ((self.y + self.height) * scale_factor).ceil() as i32;
        PhysicalRect {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        }
    }
}

impl PhysicalRect {
    pub fn to_logical(self, scale_factor: f32) -> LogicalRect {
        LogicalRect {
            x: self.x as f32 / scale_factor,
            y: self.y as f32 / scale_factor,
            width: self.width as f32 / scale_factor,
            height: self.height as f32 / scale_factor,
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
