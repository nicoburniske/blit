use blit::{Clip, PhysicalRect};
use blit_widgets::performance;

use crate::{TuiRenderer, cell::CellBuffer, image::ImagePlacement, text::TextRequest};

/// rendering resources independent of terminal io and event loop ownership
pub struct TuiContext {
    performance: performance::State,
    renderer: TuiRenderer,
    clip: PhysicalRect,
    clips: Vec<PhysicalRect>,
    should_quit: bool,
}

impl blit::Context for TuiContext {
    type Scalar = i32;
}

impl AsMut<TuiContext> for TuiContext {
    fn as_mut(&mut self) -> &mut TuiContext {
        self
    }
}

impl AsRef<performance::State> for TuiContext {
    fn as_ref(&self) -> &performance::State {
        &self.performance
    }
}

impl TuiContext {
    pub fn new(renderer: TuiRenderer) -> Self {
        let clip = renderer.screen();
        Self {
            performance: performance::State::default(),
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

    pub fn cells(&mut self, area: PhysicalRect) -> CellBuffer<'_> {
        self.renderer.cells(area, self.clip)
    }

    pub fn paint_text(&mut self, text: TextRequest) {
        self.renderer.paint_text(text, self.clip);
    }

    pub fn place_image(&mut self, image: ImagePlacement) {
        self.renderer.place_image(image, self.clip);
    }

    pub fn performance(&self) -> &performance::State {
        &self.performance
    }

    pub fn performance_mut(&mut self) -> &mut performance::State {
        &mut self.performance
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

impl<C: crate::Context> Clip<C> for BoundsClip {
    fn push(&self, context: &mut C, area: PhysicalRect) {
        let context = context.as_mut();
        let previous = context.clip;
        context.clip = previous.intersection(area).unwrap_or_default();
        context.clips.push(previous);
    }

    fn pop(&self, context: &mut C) {
        let context = context.as_mut();
        context.clip = context.clips.pop().unwrap();
    }
}
