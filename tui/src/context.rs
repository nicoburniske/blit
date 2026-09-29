use blit::{Clip, LogicalRect, PhysicalRect};
use blit_widgets::performance::FrameProfiler;

use crate::{TuiRenderer, cell::CellBuffer, geometry::cell_rect, image::ImagePlacement, text::TextRequest};

/// rendering resources independent of terminal io and event loop ownership
pub struct TuiContext {
    profiler: FrameProfiler,
    renderer: TuiRenderer,
    clip: PhysicalRect,
    clips: Vec<PhysicalRect>,
    should_quit: bool,
}

impl TuiContext {
    pub fn new(renderer: TuiRenderer) -> Self {
        let clip = renderer.screen();
        Self {
            profiler: FrameProfiler::default(),
            renderer,
            clip,
            clips: Vec::new(),
            should_quit: false,
        }
    }

    /// asks the active runner to exit after this frame
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// returns whether the active runner should exit
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn renderer(&self) -> &TuiRenderer {
        &self.renderer
    }

    pub fn renderer_mut(&mut self) -> &mut TuiRenderer {
        &mut self.renderer
    }

    pub fn cells(&mut self, area: LogicalRect) -> CellBuffer<'_> {
        self.renderer.cells(cell_rect(area), self.clip)
    }

    pub fn paint_text(&mut self, text: TextRequest) {
        self.renderer.paint_text(text, self.clip);
    }

    pub fn place_image(&mut self, image: ImagePlacement) {
        self.renderer.place_image(image, self.clip);
    }

    pub fn profiler(&self) -> &FrameProfiler {
        &self.profiler
    }

    pub fn profiler_mut(&mut self) -> &mut FrameProfiler {
        &mut self.profiler
    }

    pub fn begin_paint(&mut self) {
        self.renderer.begin_frame();
        self.clip = self.renderer.screen();
        self.clips.clear();
    }

    pub fn finish_paint(&mut self) {
        self.renderer.end_frame();
    }
}

#[derive(Clone, Copy)]
pub struct BoundsClip;

impl Clip<TuiContext> for BoundsClip {
    fn push(&self, context: &mut TuiContext, area: LogicalRect) {
        let previous = context.clip;
        context.clip = previous.intersection(cell_rect(area)).unwrap_or_default();
        context.clips.push(previous);
    }

    fn pop(&self, context: &mut TuiContext) {
        context.clip = context.clips.pop().unwrap();
    }
}
