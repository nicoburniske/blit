use crate::{
    command_list::CommandList,
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect, Scale2},
    image::{ImageData, ImageHandle},
    layout::LayoutResolution,
    text::{TextLayoutRequest, TextRequest, TextRunId, TextStyle},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderGeometry {
    pub physical_bounds: PhysicalRect,
    pub physical_per_logical: Scale2,
    pub layout_resolution: LayoutResolution,
    pub supports_zoom: bool,
}

pub trait Renderer {
    fn geometry(&self) -> RenderGeometry;

    /// updates the scale of a renderer that supports zoom
    fn set_scale(&mut self, _: Scale2) {}

    /// damage rectangles may overlap and are interpreted as their union
    fn render(&mut self, commands: &CommandList, damage: &[PhysicalRect]);

    /// projects logical interaction geometry into the rendered coordinate space
    fn interaction_area(&self, area: LogicalRect, clip: LogicalRect) -> Option<LogicalRect> {
        area.intersection(clip)
    }

    /// stores image data and returns its renderer-backed handle
    fn create_image(&mut self, data: ImageData) -> ImageHandle;

    /// stores or reuses a styled text run
    fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId;

    /// returns the nearest valid byte offset at a logical position
    fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize;

    /// returns the typographic size without rasterizing
    fn measure_text(&mut self, request: &TextLayoutRequest) -> LogicalSize;

    /// returns the caret position, line height, and minimum representable width
    /// for the nearest valid byte offset
    fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect;
}
