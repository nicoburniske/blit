use crate::{
    geometry::{Rect, Size},
    layout::LayoutResolution,
};

pub trait Platform {
    fn begin(&mut self, frame: FrameInfo);

    fn end(&mut self);

    fn interaction_area(&self, area: Rect, clip: Rect) -> Option<Rect> {
        area.intersection(clip)
    }
}

crate::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FrameInfo {
        new(size: Size),
        layout_resolution: LayoutResolution = LayoutResolution::Continuous,
    }
}

pub trait Measure<R: Platform> {
    type Output;

    fn measure(self, platform: &mut R) -> Self::Output;
}

pub trait Paint<R: Platform> {
    fn paint(self, platform: &mut R);
}
