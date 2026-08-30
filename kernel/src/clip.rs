use crate::{geometry::Rect, platform::Platform};

pub trait Clip<R: Platform>: Copy + 'static {
    fn push(&self, platform: &mut R, area: Rect);

    fn pop(&self, platform: &mut R);
}
