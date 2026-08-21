use crate::{
    RepaintBuffer,
    command_list::CommandList,
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    paint::{TextLayoutRequest, TextRequest, TextRunId, TextStyle},
    resource::{ImageData, ImageHandle},
};

pub trait PlatformImpl {
    /// damage may overlap and each covered pixel must be rendered once
    fn render(&mut self, commands: &CommandList, damage: &[PhysicalRect]);
    fn screen(&mut self) -> PhysicalRect;
    fn scale_factor(&mut self) -> f32 {
        1.0
    }
    fn repaint_buffer(&self) -> RepaintBuffer {
        RepaintBuffer::Reused
    }

    fn create_image(&mut self, data: ImageData) -> ImageHandle;

    fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId;
    fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize;
    /// returns the typographic size without rasterizing
    fn measure_text(&mut self, request: &TextLayoutRequest) -> LogicalSize;
    /// returns the cursor position and line height for the nearest valid byte offset
    fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect;
}
