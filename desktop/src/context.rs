use blit::{LogicalRect, Paint};
use blit_gui::{
    TextSystem,
    image::{ImageData, ImageHandle},
    style::BorderRadius,
};
use blit_widgets::FrameProfiler;

use crate::{GraphicsBackend, GraphicsError};

pub struct DesktopContext<G> {
    pub profiler: FrameProfiler,
    pub graphics: G,
}

impl<G: GraphicsBackend> DesktopContext<G> {
    pub fn new(graphics: G) -> Self {
        Self {
            profiler: FrameProfiler::default(),
            graphics,
        }
    }

    pub fn create_image(&mut self, data: ImageData) -> Result<ImageHandle, GraphicsError> {
        self.graphics.create_image(data)
    }
}

impl<G: GraphicsBackend> blit::Context for DesktopContext<G> {
    type Scalar = f32;
}

impl<G: GraphicsBackend> blit::HasFrameStore for DesktopContext<G> {
    fn store(&self) -> &blit::FrameStore {
        self.graphics.store()
    }

    fn store_mut(&mut self) -> &mut blit::FrameStore {
        self.graphics.store_mut()
    }
}

impl<G: GraphicsBackend> blit_widgets::Context for DesktopContext<G> {
    const SLOP: f32 = 6.0;
}

impl<G: GraphicsBackend + Paint<T>, T> Paint<T> for DesktopContext<G> {
    fn paint(&mut self, bounds: LogicalRect, command: T) {
        self.graphics.paint(bounds, command);
    }
}

impl<G: GraphicsBackend> blit_gui::Context for DesktopContext<G> {
    fn text(&mut self) -> &mut TextSystem {
        self.graphics.text()
    }

    fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.graphics.push_clip(area, radius);
    }

    fn pop_clip(&mut self) {
        self.graphics.pop_clip();
    }
}

impl<G> AsRef<FrameProfiler> for DesktopContext<G> {
    fn as_ref(&self) -> &FrameProfiler {
        &self.profiler
    }
}
