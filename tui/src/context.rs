use blit::{Corners, Paint, PhysicalRect};
use blit_widget::performance::FrameProfiler;

use crate::{RendererConfig, TuiRenderer, image::Image, text::FrameText};

/// rendering resources independent of terminal io and event loop ownership
pub struct TuiContext {
    pub profiler: FrameProfiler,
    pub renderer: TuiRenderer,
    pub should_quit: bool,
}

impl Default for TuiContext {
    fn default() -> Self {
        Self::new(TuiRenderer::new(RendererConfig::new()))
    }
}

impl blit::Context for TuiContext {
    type Scalar = i32;

    // cells cannot show rounded corners
    fn push_clip(&mut self, area: PhysicalRect, _: Corners<i32>) {
        self.renderer.push_clip(area);
    }

    fn pop_clip(&mut self) {
        self.renderer.pop_clip();
    }
}

impl blit_widget::Context for TuiContext {
    const SLOP: i32 = 1;
}

impl AsMut<TuiContext> for TuiContext {
    fn as_mut(&mut self) -> &mut TuiContext {
        self
    }
}

impl AsRef<FrameProfiler> for TuiContext {
    fn as_ref(&self) -> &FrameProfiler {
        &self.profiler
    }
}

impl TuiContext {
    pub fn new(renderer: TuiRenderer) -> Self {
        Self {
            profiler: FrameProfiler::default(),
            renderer,
            should_quit: false,
        }
    }
}

impl Paint<Image> for TuiContext {
    fn paint(&mut self, area: PhysicalRect, image: &Image) {
        self.renderer.paint_image(area, image);
    }
}

impl Paint<FrameText> for TuiContext {
    fn paint(&mut self, area: PhysicalRect, text: &FrameText) {
        self.renderer.paint_text(area, text);
    }
}
