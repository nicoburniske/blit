use std::time::Instant;

use crate::{TuiRenderer, cell::CellBuffer, image::ImagePlacement, text::TextRequest};
use blit::{Clip, Context, FrameStage, LogicalRect, Scale2};
use blit_widgets::performance::FrameProfiler;

/// rendering resources independent of terminal io and event loop ownership
pub struct TuiContext {
    profiler: FrameProfiler,
    clock: Instant,
    renderer: TuiRenderer,
    clip: LogicalRect,
    clips: Vec<LogicalRect>,
    should_quit: bool,
}

impl TuiContext {
    pub fn new(renderer: TuiRenderer) -> Self {
        let clip = renderer.screen().to_logical(Scale2::IDENTITY);
        Self {
            profiler: FrameProfiler::default(),
            clock: Instant::now(),
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
        self.renderer.cells(area, self.clip)
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
}

impl Context for TuiContext {
    fn frame_stage(&mut self, stage: FrameStage) {
        match stage {
            FrameStage::Paint => {
                self.renderer.begin_frame();
                self.clip = self.renderer.screen().to_logical(Scale2::IDENTITY);
                self.clips.clear();
            }
            FrameStage::Complete => self.renderer.end_frame(),
            FrameStage::Build | FrameStage::Layout => {}
        }
        self.profiler.begin_stage(stage, self.clock.elapsed());
    }
}

#[derive(Clone, Copy)]
pub struct BoundsClip;

impl Clip<TuiContext> for BoundsClip {
    fn push(&self, context: &mut TuiContext, area: LogicalRect) {
        let previous = context.clip;
        context.clip = previous.intersection(area).unwrap_or_default();
        context.clips.push(previous);
    }

    fn pop(&self, context: &mut TuiContext) {
        context.clip = context.clips.pop().unwrap();
    }
}
