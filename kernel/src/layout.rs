pub use crate::frame::layout::{Children, LayoutCx};
use crate::geometry::{Constraints, Size};

pub trait Layout<C>: 'static {
    /// per-child data interpreted by this layout
    ///
    /// children without explicit items store no `Item` of their own
    /// all instances of this layout type share one default value
    /// this avoids storing one item per child or layout instance
    /// so it is more efficient to rely on the default value where possible
    type Item: Default + 'static;

    /// measures this node and arranges its flow children
    ///
    /// layout may run more than once per frame, including during size
    /// transitions. every call must:
    ///
    /// - call [`LayoutCx::layout_child`] with every flow child's final constraints
    /// - call [`LayoutCx::set_position`] for every flow child
    /// - return a size within `constraints`
    ///
    /// [`LayoutCx::layout_child`] applies animated size overrides
    ///
    /// - use [`LayoutCx::resolve_sizing`] when sizing affects allocation before laying out the child
    /// - use [`LayoutCx::target_size`] when animated size must not change structure such as wrapping
    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> Size;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
}

impl Axis {
    #[inline]
    pub const fn other(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }

    #[inline]
    pub const fn extent(self, size: Size) -> f32 {
        match self {
            Self::Horizontal => size.width,
            Self::Vertical => size.height,
        }
    }

    #[inline]
    pub fn set_extent(self, size: &mut Size, extent: f32) {
        match self {
            Self::Horizontal => size.width = extent,
            Self::Vertical => size.height = extent,
        }
    }
}

/// one-dimensional sizing policy interpreted by a layout
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
            Self::Fit { min, max } | Self::Grow { min, max } => size.clamp(min.max(0.0), max.max(min).max(0.0)),
            Self::Fixed(fixed) => fixed.max(0.0),
            Self::Percent(_) => size.max(0.0),
        }
    }
}
