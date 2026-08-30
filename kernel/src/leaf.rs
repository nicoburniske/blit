use crate::{
    geometry::{Constraints, Rect, Size},
    platform::Platform,
};

pub trait Leaf<R: Platform>: Copy + 'static {
    fn measure(&self, platform: &mut R, constraints: Constraints) -> Size;

    fn paint(&self, platform: &mut R, area: Rect);
}
