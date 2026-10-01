pub mod absolute;
pub mod cache;
pub mod flex;
pub mod grid;
pub mod single;
pub mod wrap;

mod size;

pub use size::{Padding, Sizing, Unit};
use size::{flow_constraints, flow_size, flow_sizing, intrinsic_child, intrinsic_cross, intrinsic_range, sizing_range};

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

fn justify_offset(justify: Justify, remaining: f32, count: usize) -> (f32, f32) {
    match justify {
        Justify::Start => (0.0, 0.0),
        Justify::Center => (remaining / 2.0, 0.0),
        Justify::End => (remaining, 0.0),
        Justify::SpaceBetween if count > 1 => (0.0, remaining / (count - 1) as f32),
        Justify::SpaceAround if count != 0 => {
            let space = remaining / count as f32;
            (space / 2.0, space)
        }
        Justify::SpaceEvenly if count != 0 => {
            let space = remaining / (count + 1) as f32;
            (space, space)
        }
        _ => (0.0, 0.0),
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! layout_ext {
    ($unit:ty) => {
        pub trait LayoutExt: ::blit::Layout + Sized {
            /// positions this layout relative to `Ui::relative`
            fn absolute(self) -> $crate::absolute::Layout<Self, $unit> {
                $crate::absolute::Layout::new(self)
            }

            /// caches this layout's intrinsic answers
            fn cached(self) -> $crate::cache::Cached<Self> {
                $crate::cache::cached(self)
            }
        }

        impl<L: ::blit::Layout> LayoutExt for L {}
    };
}

#[cfg(test)]
mod test;
