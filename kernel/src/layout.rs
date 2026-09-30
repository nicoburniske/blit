pub use crate::frame::layout::{Child, Children, LayoutCx, MeasureCx, ScratchSlice};
use crate::geometry::{Constraints, LogicalSize};

pub trait Layout<C>: 'static {
    /// per-child data interpreted by this layout
    ///
    /// children without explicit items store no `Item` of their own
    /// all instances of this layout type share one default value
    /// this avoids storing one item per child or layout instance
    /// so it is more efficient to rely on the default value where possible
    type Item: Default + 'static;

    /// reports natural extents without changing geometry
    /// repeated queries must agree while context resources remain unchanged
    fn intrinsic(&self, cx: &mut MeasureCx<'_, C, Self::Item>, query: IntrinsicQuery) -> IntrinsicSize;

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
    /// - use [`LayoutCx::size_overrides`] when sizing affects allocation before laying out the child
    /// - use [`LayoutCx::target_size`] when animated size must not change structure such as wrapping
    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> LogicalSize;
}

/// an axis query with an optional exact extent in the other axis
/// unspecified cross size asks for the layout's unconstrained natural contribution
/// a supplied cross extent must be finite and nonnegative
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntrinsicQuery {
    pub axis: Axis,
    pub cross: Option<f32>,
}

impl IntrinsicQuery {
    pub const fn new(axis: Axis) -> Self {
        Self { axis, cross: None }
    }

    pub const fn cross(mut self, extent: f32) -> Self {
        self.cross = Some(extent);
        self
    }
}

/// natural extents before external constraints are applied
///
/// min is the content's minimum contribution and preferred is its natural extent
/// both must be finite and nonnegative with preferred at least min
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IntrinsicSize {
    pub min: f32,
    pub preferred: f32,
}

impl IntrinsicSize {
    pub const fn new(min: f32, preferred: f32) -> Self {
        Self { min, preferred }
    }

    pub const fn uniform(extent: f32) -> Self {
        Self::new(extent, extent)
    }
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
    pub const fn extent(self, size: LogicalSize) -> f32 {
        match self {
            Self::Horizontal => size.width,
            Self::Vertical => size.height,
        }
    }

    #[inline]
    pub fn set_extent(self, size: &mut LogicalSize, extent: f32) {
        match self {
            Self::Horizontal => size.width = extent,
            Self::Vertical => size.height = extent,
        }
    }
}
