use blit::{Axis, Constraints, Size};

#[cfg(feature = "tui")]
mod platform {
    pub type Length = u16;
    pub type Offset = i32;

    #[inline]
    pub fn round(value: f32) -> f32 {
        value.round()
    }

    pub fn distribute(cursor: &mut f32, share: f32) -> f32 {
        let previous = cursor.round();
        *cursor += share;
        cursor.round() - previous
    }
}

#[cfg(not(feature = "tui"))]
mod platform {
    pub type Length = f32;
    pub type Offset = f32;

    #[inline]
    pub fn round(value: f32) -> f32 {
        value
    }

    pub fn distribute(_: &mut f32, share: f32) -> f32 {
        share
    }
}

pub use platform::*;

blit::builder! {
    /// layout-owned inset lengths
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Padding {
        new(),
        top: Length = 0 as Length,
        right: Length = 0 as Length,
        bottom: Length = 0 as Length,
        left: Length = 0 as Length,
    }
}

impl Padding {
    pub const fn all(value: Length) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

impl From<Padding> for blit::Sides {
    fn from(value: Padding) -> Self {
        Self {
            top: value.top as f32,
            right: value.right as f32,
            bottom: value.bottom as f32,
            left: value.left as f32,
        }
    }
}

/// physical extents use the platform layout unit; percentages remain ratios
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: Length, max: Option<Length> },
    Grow { min: Length, max: Option<Length> },
    Fixed(Length),
    Percent(f32),
}

impl Sizing {
    pub const fn fit() -> Self {
        Self::Fit {
            min: 0 as Length,
            max: None,
        }
    }

    pub const fn fit_range(min: Length, max: Length) -> Self {
        Self::Fit { min, max: Some(max) }
    }

    pub const fn grow() -> Self {
        Self::Grow {
            min: 0 as Length,
            max: None,
        }
    }

    pub const fn grow_min(min: Length) -> Self {
        Self::Grow { min, max: None }
    }

    pub const fn grow_range(min: Length, max: Length) -> Self {
        Self::Grow { min, max: Some(max) }
    }

    pub const fn fixed(size: Length) -> Self {
        Self::Fixed(size)
    }

    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }
}

impl From<Sizing> for blit::Sizing {
    fn from(value: Sizing) -> Self {
        match value {
            Sizing::Fit { min, max } => Self::fit_range(min as f32, max.map_or(f32::INFINITY, |max| max as f32)),
            Sizing::Grow { min, max } => Self::grow_range(min as f32, max.map_or(f32::INFINITY, |max| max as f32)),
            Sizing::Fixed(size) => Self::fixed(size as f32),
            Sizing::Percent(fraction) => Self::percent(fraction),
        }
    }
}

blit::builder! {
    /// sizing policy for a flow layout child
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item {
        new(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
    }
}

impl Item {
    pub fn fixed(mut self, width: Length, height: Length) -> Self {
        self.width = Sizing::fixed(width);
        self.height = Sizing::fixed(height);
        self
    }

    pub fn grow(mut self) -> Self {
        self.width = Sizing::grow();
        self.height = Sizing::grow();
        self
    }

    pub fn sizing(&self, axis: Axis) -> Sizing {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }
}

pub fn item() -> Item {
    Item::new()
}

pub fn round_sizing(sizing: blit::Sizing) -> blit::Sizing {
    match sizing {
        blit::Sizing::Fit { min, max } => blit::Sizing::Fit {
            min: round(min),
            max: round(max),
        },
        blit::Sizing::Grow { min, max } => blit::Sizing::Grow {
            min: round(min),
            max: round(max),
        },
        blit::Sizing::Fixed(size) => blit::Sizing::Fixed(round(size)),
        blit::Sizing::Percent(fraction) => blit::Sizing::Percent(fraction),
    }
}

pub fn flow_size(main: f32, cross: f32, axis: Axis) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, cross),
        Axis::Vertical => Size::new(cross, main),
    }
}

#[inline]
pub fn sizing_range(sizing: blit::Sizing, available: f32) -> (f32, f32) {
    match round_sizing(sizing) {
        blit::Sizing::Fit { min, max } | blit::Sizing::Grow { min, max } => {
            let min = min.max(0.0);
            (min, max.max(min).min(available).max(min))
        }
        blit::Sizing::Fixed(size) => {
            let size = size.max(0.0);
            (size, size)
        }
        blit::Sizing::Percent(fraction) => {
            assert!((0.0..=1.0).contains(&fraction));
            let size = if available.is_finite() {
                available * fraction
            } else {
                0.0
            };
            (size, size)
        }
    }
}

pub fn flow_constraints(axis: Axis, main: (f32, f32), cross: (f32, f32)) -> Constraints {
    let (width, height) = match axis {
        Axis::Horizontal => (main, cross),
        Axis::Vertical => (cross, main),
    };
    Constraints {
        min: Size::new(round(width.0), round(height.0)),
        max: Size::new(round(width.1), round(height.1)),
    }
}
