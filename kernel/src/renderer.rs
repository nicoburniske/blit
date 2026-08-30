use crate::{
    geometry::{Rect, Size},
    layout::LayoutResolution,
};

pub trait Renderer {
    fn begin(&mut self, frame: FrameInfo);

    fn end(&mut self);

    fn interaction_area(&self, area: Rect, clip: Rect) -> Option<Rect> {
        area.intersection(clip)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameInfo {
    pub size: Size,
    pub layout_resolution: LayoutResolution,
}

impl FrameInfo {
    pub const fn new(size: Size) -> Self {
        Self {
            size,
            layout_resolution: LayoutResolution::Continuous,
        }
    }

    pub const fn layout_resolution(mut self, layout_resolution: LayoutResolution) -> Self {
        self.layout_resolution = layout_resolution;
        self
    }
}

pub trait Measure<R: Renderer> {
    type Output;

    fn measure(self, renderer: &mut R) -> Self::Output;
}

pub trait Paint<R: Renderer> {
    fn paint(self, renderer: &mut R);
}
