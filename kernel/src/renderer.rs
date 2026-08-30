use crate::geometry::Size;

pub trait Renderer {
    fn begin(&mut self, frame: FrameInfo);

    fn end(&mut self);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameInfo {
    pub size: Size,
}

pub trait Measure<R: Renderer> {
    type Output;

    fn measure(self, renderer: &mut R) -> Self::Output;
}

pub trait Paint<R: Renderer> {
    fn paint(self, renderer: &mut R);
}
