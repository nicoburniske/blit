use crate::{
    command_list::CommandList,
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    image::{ImageData, ImageHandle},
    layout::LayoutResolution,
    text::{TextLayoutRequest, TextRequest, TextRunId, TextStyle},
};

pub trait Renderer {
    /// updates the logical-to-physical scale used by rendering and text layout
    fn set_scale_factor(&mut self, scale_factor: f32);

    fn layout_resolution(&self) -> LayoutResolution {
        LayoutResolution::Continuous
    }

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
