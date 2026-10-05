use crate::{
    Context,
    geometry::{Constraints, Point, Size},
};
pub use crate::{
    arena::Scratch,
    frame::layout::{Children, LayoutCx},
};

pub trait Layout<C: Context>: 'static {
    /// per child data interpreted by this layout
    /// children without explicit items share this type's default value
    type Item: Default + 'static;

    /// configures this node before the layout is stored
    fn on_insert<'a>(&self, ui: crate::Ui<'a, C>) -> crate::Ui<'a, C> {
        ui
    }

    /// sizes and positions children and returns a constrained size
    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<C::Scalar>) -> Size<C::Scalar>;
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
