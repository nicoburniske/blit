use blit::{Clip, FrameInfo, LogicalRect, Platform, Scale2};
use blit_tui_render::{TuiRenderer, cell::CellBuffer, image::ImagePlacement, text::TextRequest};

pub struct TuiPlatform {
    renderer: TuiRenderer,
    clip: LogicalRect,
    clips: Vec<LogicalRect>,
}

impl TuiPlatform {
    pub fn new(renderer: TuiRenderer) -> Self {
        let clip = renderer.screen().to_logical(Scale2::IDENTITY);
        Self {
            renderer,
            clip,
            clips: Vec::new(),
        }
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

impl Platform for TuiPlatform {
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
