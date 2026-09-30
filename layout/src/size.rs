use blit::{Axis, Constraints, LogicalSize};

/// physical layout units and their allocation precision
pub trait Unit: Copy + std::fmt::Debug + PartialEq + 'static {
    type Offset: Unit;
    const ZERO: Self;
    const ONE: Self;

    fn into_float(self) -> f32;
    fn from_float(value: f32) -> Self;
    fn round(value: f32) -> f32;

    #[inline]
    fn distribute(cursor: &mut f32, share: f32) -> f32 {
        let previous = Self::round(*cursor);
        *cursor += share;
        Self::round(*cursor) - previous
    }
}

macro_rules! unit {
    ($(($ty:ty, $offset:ty, $round:path $(, $extra:item)*)),+ $(,)?) => {
        $(impl Unit for $ty {
            type Offset = $offset;
            const ZERO: Self = 0 as Self;
            const ONE: Self = 1 as Self;

            #[inline]
            fn into_float(self) -> f32 {
                self as f32
            }
            #[inline]
            fn from_float(value: f32) -> Self {
                Self::round(value) as Self
            }
            #[inline]
            fn round(value: f32) -> f32 {
                $round(value)
            }
            $($extra)*
        })+
    };
}

unit! {
    (f32, f32, std::convert::identity,
        #[inline]
        fn distribute(_: &mut f32, share: f32) -> f32 {
            share
        }
    ),
    (u16, i32, f32::round),
    (i32, i32, f32::round),
}

blit::builder! {
    #[const]
    /// layout-owned inset lengths
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Padding<U: Unit> {
        new(),
        top: U = U::ZERO,
        right: U = U::ZERO,
        bottom: U = U::ZERO,
        left: U = U::ZERO,
    }
}

impl<U: Unit> Padding<U> {
    pub const fn all(value: U) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

impl<U: Unit> From<Padding<U>> for blit::Sides {
    fn from(value: Padding<U>) -> Self {
        Self {
            top: value.top.into_float(),
            right: value.right.into_float(),
            bottom: value.bottom.into_float(),
            left: value.left.into_float(),
        }
    }
}

/// physical extents use the selected unit and percentages remain ratios
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing<U> {
    Fit { min: U, max: Option<U> },
    Grow { min: U, max: Option<U> },
    Fixed(U),
    Percent(f32),
}

impl<U: Unit> Sizing<U> {
    pub const fn fit() -> Self {
        Self::Fit {
            min: U::ZERO,
            max: None,
        }
    }

    pub const fn fit_range(min: U, max: U) -> Self {
        Self::Fit { min, max: Some(max) }
    }

    pub const fn grow() -> Self {
        Self::Grow {
            min: U::ZERO,
            max: None,
        }
    }

    pub const fn grow_min(min: U) -> Self {
        Self::Grow { min, max: None }
    }

    pub const fn grow_range(min: U, max: U) -> Self {
        Self::Grow { min, max: Some(max) }
    }

    pub const fn fixed(size: U) -> Self {
        Self::Fixed(size)
    }

    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }

    /// transforms lengths while leaving percentages unchanged
    pub fn map<V>(self, f: impl Fn(U) -> V) -> Sizing<V> {
        match self {
            Self::Fit { min, max } => Sizing::Fit {
                min: f(min),
                max: max.map(f),
            },
            Self::Grow { min, max } => Sizing::Grow {
                min: f(min),
                max: max.map(f),
            },
            Self::Fixed(size) => Sizing::Fixed(f(size)),
            Self::Percent(fraction) => Sizing::Percent(fraction),
        }
    }

    pub fn into_float(self) -> Sizing<f32> {
        self.map(U::into_float)
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
    #[const]
    /// sizing policy for a flow layout child
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item<U: Unit> {
        new(),
        width: Sizing<U> = Sizing::fit(),
        height: Sizing<U> = Sizing::fit(),
    }
}

impl<U: Unit> Item<U> {
    pub const fn fixed(mut self, width: U, height: U) -> Self {
        self.width = Sizing::fixed(width);
        self.height = Sizing::fixed(height);
        self
    }

    pub const fn grow(mut self) -> Self {
        self.width = Sizing::grow();
        self.height = Sizing::grow();
        self
    }
}

pub fn flow_sizing<U: Unit>(
    axis: Axis,
    width: Sizing<U>,
    height: Sizing<U>,
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
pub fn sizing_range<U: Unit>(sizing: Sizing<f32>, available: f32) -> (f32, f32) {
    match sizing.map(U::round) {
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

pub fn flow_constraints<U: Unit>(axis: Axis, main: (f32, f32), cross: (f32, f32)) -> Constraints {
    let (width, height) = match axis {
        Axis::Horizontal => (main, cross),
        Axis::Vertical => (cross, main),
    };
    Constraints {
        min: LogicalSize::new(U::round(width.0), U::round(height.0)),
        max: LogicalSize::new(U::round(width.1), U::round(height.1)),
    }
}
