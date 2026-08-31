use blit::{Clip, FrameInfo, LogicalPoint, LogicalRect, Platform, Scale2, Size};
use blit_tui_render::{
    TuiRenderer,
    image::{ImageData, ImageHandle, ImagePlacement},
    surface::CellBuffer,
    text::{Span, TextLayoutRequest, TextRequest, TextRunId},
};

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

    pub fn invalidate_all(&mut self) {
        self.renderer.invalidate();
    }

    pub fn cells(&mut self, area: LogicalRect) -> CellBuffer<'_> {
        self.renderer.cells(area, self.clip)
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        self.renderer.create_image(data)
    }

    pub fn text_run(&mut self, text: &str) -> TextRunId {
        self.renderer.text_run(text)
    }

    pub fn rich_text(&mut self, spans: &[Span<'_>]) -> TextRunId {
        self.renderer.rich_text(spans)
    }

    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest,
        position: LogicalPoint,
    ) -> usize {
        self.renderer.text_offset_at_position(request, position)
    }

    pub fn text_cursor_rect(&mut self, request: &TextRequest, offset: usize) -> LogicalRect {
        self.renderer.text_cursor_rect(request, offset)
    }

    pub fn measure_text(&mut self, request: &TextLayoutRequest) -> Size {
        self.renderer.measure_text(request)
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
