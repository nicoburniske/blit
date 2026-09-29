use blit::{Axis, Constraints, LogicalSize};

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

/// physical extents use the platform layout unit by default; percentages remain ratios
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing<Unit = Length> {
    Fit { min: Unit, max: Option<Unit> },
    Grow { min: Unit, max: Option<Unit> },
    Fixed(Unit),
    Percent(f32),
}

impl Sizing<Length> {
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

    pub fn into_float(self) -> Sizing<f32> {
        match self {
            Self::Fit { min, max } => Sizing::Fit {
                min: min as f32,
                max: max.map(|max| max as f32),
            },
            Self::Grow { min, max } => Sizing::Grow {
                min: min as f32,
                max: max.map(|max| max as f32),
            },
            Self::Fixed(size) => Sizing::Fixed(size as f32),
            Self::Percent(fraction) => Sizing::Percent(fraction),
        }
    }

    pub fn with_override(self, animated: Option<f32>) -> Sizing<f32> {
        animated.map_or_else(|| self.into_float(), Sizing::Fixed)
    }
}

impl Sizing<f32> {
    #[inline]
    pub fn clamp(self, size: f32) -> f32 {
        match self {
            Self::Fit { min, max } | Self::Grow { min, max } => {
                size.clamp(min.max(0.0), max.unwrap_or(f32::INFINITY).max(min).max(0.0))
            }
            Self::Fixed(fixed) => fixed.max(0.0),
            Self::Percent(_) => size.max(0.0),
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
}

pub fn item() -> Item {
    Item::new()
}

pub fn round_sizing(sizing: Sizing<f32>) -> Sizing<f32> {
    match sizing {
        Sizing::Fit { min, max } => Sizing::Fit {
            min: round(min),
            max: max.map(round),
        },
        Sizing::Grow { min, max } => Sizing::Grow {
            min: round(min),
            max: max.map(round),
        },
        Sizing::Fixed(size) => Sizing::Fixed(round(size)),
        Sizing::Percent(fraction) => Sizing::Percent(fraction),
    }
}

pub fn flow_sizing(
    axis: Axis,
    width: Sizing,
    height: Sizing,
    (width_override, height_override): (Option<f32>, Option<f32>),
) -> (Sizing<f32>, Sizing<f32>) {
    let width = width.with_override(width_override);
    let height = height.with_override(height_override);
    match axis {
        Axis::Horizontal => (width, height),
        Axis::Vertical => (height, width),
    }
}

pub fn flow_size(main: f32, cross: f32, axis: Axis) -> LogicalSize {
    match axis {
        Axis::Horizontal => LogicalSize::new(main, cross),
        Axis::Vertical => LogicalSize::new(cross, main),
    }
}

#[inline]
pub fn sizing_range(sizing: Sizing<f32>, available: f32) -> (f32, f32) {
    match round_sizing(sizing) {
        Sizing::Fit { min, max } | Sizing::Grow { min, max } => {
            let min = min.max(0.0);
            let max = max.unwrap_or(f32::INFINITY);
            (min, max.max(min).min(available).max(min))
        }
        Sizing::Fixed(size) => {
            let size = size.max(0.0);
            (size, size)
        }
        Sizing::Percent(fraction) => {
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
        min: LogicalSize::new(round(width.0), round(height.0)),
        max: LogicalSize::new(round(width.1), round(height.1)),
    }
}
