use crate::{
    Context, Scalar,
    geometry::{Constraints, Point, Rect, Size},
};
pub use crate::{
    Scratch,
    frame::layout::{Children, LayoutCx, PlaceCx},
};

pub trait Layout<C: Context>: 'static {
    /// per child data interpreted by this layout
    /// children without explicit items share this type's default value
    type Item: Default + 'static;

    /// sizes and positions children and returns a constrained size
    /// the kernel resets flow child positions before invoking this layout
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

/// positions a node outside its parent's flow
pub trait Place<C: Context>: 'static {
    /// returns this node's frame position
    fn place(&self, cx: &mut PlaceCx<'_, C>, target: Rect<C::Scalar>) -> Point<C::Scalar>;
}

/// places at the target's origin under unbounded constraints
impl<C: Context> Place<C> for () {
    fn place(&self, cx: &mut PlaceCx<'_, C>, target: Rect<C::Scalar>) -> Point<C::Scalar> {
        cx.resolve(Constraints::loose(Size::uniform(C::Scalar::UNBOUNDED)));
        Point::new(target.x, target.y)
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
