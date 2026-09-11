use std::time::Instant;

use blit::{Clip, FrameInfo, FrameStage, LogicalRect, Platform, Scale2};
use blit_std::widget::performance::{FrameProfiler, Profiled};
use blit_tui_render::{TuiRenderer, cell::CellBuffer, image::ImagePlacement, text::TextRequest};

/// rendering resources independent of terminal io and event loop ownership
pub struct TuiPlatform {
    profiler: FrameProfiler,
    clock: Instant,
    renderer: TuiRenderer,
    clip: LogicalRect,
    clips: Vec<LogicalRect>,
    should_quit: bool,
}

impl TuiPlatform {
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
}

impl Profiled for TuiPlatform {
    fn profiler(&self) -> &FrameProfiler {
        &self.profiler
    }
}

impl Platform for TuiPlatform {
    fn begin_stage(&mut self, stage: FrameStage) {
        self.profiler.begin_stage(stage, self.clock.elapsed());
    }

    fn begin(&mut self, _: FrameInfo) {
        self.renderer.begin_frame();
        self.clip = self.renderer.screen().to_logical(Scale2::IDENTITY);
        self.clips.clear();
    }

    fn end(&mut self) {
        self.renderer.end_frame();
    }

    fn interaction_area(&self, area: LogicalRect, clip: LogicalRect) -> Option<LogicalRect> {
        self.renderer.interaction_area(area, clip)
    }
}

#[derive(Clone, Copy)]
pub struct BoundsClip;

impl Clip<TuiPlatform> for BoundsClip {
    fn push(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        let previous = platform.clip;
        platform.clip = previous.intersection(area).unwrap_or_default();
        platform.clips.push(previous);
    }

    fn pop(&self, platform: &mut TuiPlatform) {
        platform.clip = platform.clips.pop().unwrap();
    }
}
