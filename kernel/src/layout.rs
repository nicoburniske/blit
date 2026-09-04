pub use crate::frame::layout::{Children, LayoutCx};

use crate::{
    Platform,
    geometry::{Constraints, Sides, Size},
};

pub trait Layout<R: Platform>: 'static {
    /// per-child data interpreted by this layout
    type Item: 'static;

    /// measures this node and arranges its flow children
    ///
    /// layout may run more than once per frame, including during size
    /// transitions. every call must:
    ///
    /// - measure any contributing atoms with [`LayoutCx::measure_atoms`]
    /// - call [`LayoutCx::layout_child`] with every flow child's final constraints
    /// - call [`LayoutCx::set_child_position`] for every flow child
    /// - adapt layout-owned physical lengths through [`LayoutCx::resolution`]
    /// - return a size within `constraints`
    ///
    /// use [`LayoutCx::target_child_size`] when animated sizes must not change
    /// structural decisions such as wrapping.
    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size;

    /// applies an animated outer size to a flow child item
    ///
    /// return
    /// - `true` if layout honors every supplied extent
    /// - `false` disables the transition and must leave `item` unchanged
    fn override_size(&self, item: &mut Self::Item, width: Option<f32>, height: Option<f32>)
    -> bool;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
}

/// one-dimensional sizing policy interpreted by a layout or absolute placement
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: f32, max: f32 },
    Grow { min: f32, max: f32 },
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

    pub const fn grow() -> Self {
        Self::grow_range(0.0, f32::INFINITY)
    }

    pub const fn grow_range(min: f32, max: f32) -> Self {
        Self::Grow { min, max }
    }

    pub const fn fixed(size: f32) -> Self {
        Self::Fixed(size)
    }

    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }

    #[inline]
    pub fn clamp(self, size: f32) -> f32 {
        match self {
            Self::Fit { min, max } | Self::Grow { min, max } => {
                size.clamp(min.max(0.0), max.max(min).max(0.0))
            }
            Self::Fixed(fixed) => fixed.max(0.0),
            Self::Percent(_) => size.max(0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LayoutResolution {
    #[default]
    Continuous,
    Discrete {
        step: Size,
    },
}

impl LayoutResolution {
    /// adapts an extent to this resolution
    ///
    /// continuous values are unchanged. discrete values round up by `step`.
    #[inline]
    pub fn extent(self, axis: Axis, value: f32) -> f32 {
        let Self::Discrete { step } = self else {
            return value;
        };
        if value <= 0.0 || !value.is_finite() {
            return value;
        }
        let step = match axis {
            Axis::Horizontal => step.width,
            Axis::Vertical => step.height,
        };
        assert!(step.is_finite() && step > 0.0);
        (value / step).ceil() * step
    }

    /// adapts absolute extents in a sizing policy to this resolution
    ///
    /// percentage policies are unchanged.
    #[inline]
    pub fn sizing(self, axis: Axis, sizing: Sizing) -> Sizing {
        match sizing {
            Sizing::Fit { min, max } => Sizing::Fit {
                min: self.extent(axis, min),
                max: self.extent(axis, max),
            },
            Sizing::Grow { min, max } => Sizing::Grow {
                min: self.extent(axis, min),
                max: self.extent(axis, max),
            },
            Sizing::Fixed(size) => Sizing::Fixed(self.extent(axis, size)),
            Sizing::Percent(fraction) => Sizing::Percent(fraction),
        }
    }

    /// adapts horizontal and vertical sides on their respective axes
    #[inline]
    pub fn sides(self, sides: Sides) -> Sides {
        Sides {
            top: self.extent(Axis::Vertical, sides.top),
            right: self.extent(Axis::Horizontal, sides.right),
            bottom: self.extent(Axis::Vertical, sides.bottom),
            left: self.extent(Axis::Horizontal, sides.left),
        }
    }
}
