pub use crate::frame::layout::{Children, LayoutCx};

use crate::{
    Platform,
    frame::Sizing,
    geometry::{Constraints, Sides, Size},
};

pub trait Layout<R: Platform>: 'static {
    /// per-child data interpreted by this layout
    type Item: Copy + 'static;

    /// lays out direct non-absolute children within parent constraints
    ///
    /// - use `cx` to measure, size, and position children
    /// - call [`LayoutCx::measure_base`] when this node's atoms contribute
    /// - return the desired size, which the kernel constrains
    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size;

    /// applies animated outer dimensions to a temporary child item
    ///
    /// - values are the current interpolated dimensions
    /// - `None` leaves that axis unchanged
    /// - leaving the item unchanged disables animated reflow
    fn size_override(&self, item: &mut Self::Item, width: Option<f32>, height: Option<f32>);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
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
