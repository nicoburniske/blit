use crate::{
    Context, Scalar, Ui,
    geometry::{Constraints, Point, Size},
};
pub use crate::{
    bump::Scratch,
    frame::layout::{Children, LayoutCx},
};

pub trait Layout<C: Context>: 'static {
    /// per child data interpreted by this layout
    /// children without explicit items share this type's default value
    type Item: Default + 'static;

    /// sizes and positions children and returns a constrained size
    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<C::Scalar>) -> Size<C::Scalar>;

    /// returns declared sizing without measuring content or visiting children
    /// keep this query cheap as parents may call it multiple times
    fn sizing(&self, _axis: Axis) -> Sizing<C::Scalar> {
        Sizing::fit()
    }

    /// configures this node before the layout is stored
    fn on_insert<'a>(&self, ui: Ui<'a, C>) -> Ui<'a, C> {
        ui
    }
}

/// forwards constraints to overlapping children and measures this node's atoms
impl<C: Context> Layout<C> for () {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, C, ()>, bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        let mut size = cx.measure_atoms(bounds);
        for child in cx.children() {
            size = size.max(cx.layout_child(child, bounds));
            cx.set_child_position(child, Point::ZERO);
        }
        bounds.constrain(size)
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
    pub fn extent<T: Copy>(self, size: Size<T>) -> T {
        match self {
            Self::Horizontal => size.width,
            Self::Vertical => size.height,
        }
    }

    #[inline]
    pub fn set_extent<T>(self, size: &mut Size<T>, extent: T) {
        match self {
            Self::Horizontal => size.width = extent,
            Self::Vertical => size.height = extent,
        }
    }
}

/// one dimensional sizing policy interpreted by a layout
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing<T> {
    Fit { min: T, max: T },
    Grow { min: T, max: T },
    Fixed(T),
    Percent(f32),
}

impl<T: Scalar> Sizing<T> {
    pub const fn fit() -> Self {
        Self::fit_range(T::ZERO, T::UNBOUNDED)
    }
    pub const fn fit_range(min: T, max: T) -> Self {
        Self::Fit { min, max }
    }
    pub const fn grow() -> Self {
        Self::grow_range(T::ZERO, T::UNBOUNDED)
    }
    pub const fn grow_range(min: T, max: T) -> Self {
        Self::Grow { min, max }
    }
    pub const fn fixed(size: T) -> Self {
        Self::Fixed(size)
    }
    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }

    #[inline]
    pub fn clamp(self, size: T) -> T {
        match self {
            Self::Fit { min, max } | Self::Grow { min, max } => {
                let min = min.max(T::ZERO);
                size.clamp(min, max.max(min))
            }
            Self::Fixed(fixed) => fixed.max(T::ZERO),
            Self::Percent(_) => size.max(T::ZERO),
        }
    }
}
