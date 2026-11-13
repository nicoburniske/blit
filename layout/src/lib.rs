pub mod absolute;
pub mod flex;
pub mod grid;
mod pad;
pub mod rect;
pub mod wrap;

use blit::{Axis, Constraints, Context, LayoutCx, NodeId, Scalar, Size};

/// allocation requested from a parent layout on one axis
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// resolves sizing before allocation including animated overrides
#[inline]
pub fn resolve_sizing<C: Context, I: 'static>(
    cx: &LayoutCx<'_, C, I>,
    child: NodeId,
    axis: Axis,
    sizing: Sizing<C::Scalar>,
) -> Sizing<C::Scalar> {
    let (width, height) = cx.size_overrides(child);
    let extent = match axis {
        Axis::Horizontal => width,
        Axis::Vertical => height,
    };
    extent.map_or(sizing, Sizing::Fixed)
}

fn flow_size<T>(main: T, cross: T, axis: Axis) -> Size<T> {
    match axis {
        Axis::Horizontal => Size::new(main, cross),
        Axis::Vertical => Size::new(cross, main),
    }
}

#[inline]
fn sizing_range<T: Scalar>(sizing: Sizing<T>, available: T) -> (T, T) {
    allocated_range(sizing, available, &mut 0.0)
}

#[inline]
fn allocated_range<T: Scalar>(sizing: Sizing<T>, available: T, cursor: &mut f32) -> (T, T) {
    match sizing {
        Sizing::Fit { min, max } | Sizing::Grow { min, max } => {
            let min = min.max(T::ZERO);
            (min, max.max(min).min(available).max(min))
        }
        Sizing::Fixed(size) => {
            let size = size.max(T::ZERO);
            (size, size)
        }
        Sizing::Percent(fraction) => {
            assert!((0.0..=1.0).contains(&fraction));
            let share = if available.is_finite() {
                available.to_f32() * fraction
            } else {
                0.0
            };
            let size = T::allocate(cursor, share);
            (size, size)
        }
    }
}

fn flow_constraints<T>(axis: Axis, main: (T, T), cross: (T, T)) -> Constraints<T> {
    Constraints {
        min: flow_size(main.0, cross.0, axis),
        max: flow_size(main.1, cross.1, axis),
    }
}

fn justify_offset<T: Scalar>(justify: Justify, remaining: T, count: usize) -> (T, f32, f32) {
    match justify {
        Justify::Start => (T::ZERO, 0.0, 0.0),
        Justify::Center => (T::ZERO.lerp(remaining, 0.5), 0.0, 0.0),
        Justify::End => (remaining, 0.0, 0.0),
        Justify::SpaceBetween if count > 1 => (T::ZERO, 0.0, remaining.to_f32() / (count - 1) as f32),
        Justify::SpaceAround if count != 0 => {
            let space = remaining.to_f32() / count as f32;
            (T::ZERO, space / 2.0, space)
        }
        Justify::SpaceEvenly if count != 0 => {
            let space = remaining.to_f32() / (count + 1) as f32;
            (T::ZERO, space, space)
        }
        _ => (T::ZERO, 0.0, 0.0),
    }
}

#[macro_export]
macro_rules! export {
    ($coord:ty) => {
        pub use $crate::{Align, Justify, rect};
        pub type Sizing = $crate::Sizing<$coord>;

        pub mod absolute {
            pub use $crate::absolute::Anchor;
            pub type Place = $crate::absolute::Place<$coord>;
            pub type Sizing = $crate::absolute::Sizing<$coord>;
            pub fn place() -> Place {
                $crate::absolute::place()
            }
        }
        $crate::export!(@ $coord, flex, [Layout, Item], [
            layout(axis: ::blit::Axis) -> Layout,
            row() -> Layout,
            column() -> Layout,
            item() -> Item
        ]);
        $crate::export!(@ $coord, grid, [Layout, Item], [
            new(columns: u32) -> Layout,
            item() -> Item
        ]);
        $crate::export!(@ $coord, wrap, [Layout, Item], [
            new(axis: ::blit::Axis) -> Layout,
            horizontal() -> Layout,
            vertical() -> Layout,
            item() -> Item
        ]);
    };
    (@ $coord:ty, $module:ident, [$($ty:ident),+], [$(
        $name:ident($($arg:ident: $arg_ty:ty),*) -> $result:ident
    ),+]) => {
        pub mod $module {
            $(pub type $ty = $crate::$module::$ty<$coord>;)+
            $(pub fn $name($($arg: $arg_ty),*) -> $result {
                $crate::$module::$name($($arg),*)
            })+
        }
    };
}
