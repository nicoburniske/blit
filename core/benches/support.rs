use std::hint::black_box;

use blit::{
    Container, RepaintBuffer, Sizing, Ui,
    color::Color,
    command_list::CommandList,
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    interact::{Sense, WidgetId},
    keyboard::KeyboardRequest,
    paint::{self, TextLayoutRequest},
    platform::PlatformImpl,
    resource::{ImageData, ImageId, StringData, StringId},
    widget::Rectangle,
};

pub const ROWS: usize = 256;
pub const CELLS_PER_ROW: usize = 3;
pub const ITEMS: usize = ROWS * CELLS_PER_ROW;

pub struct NoopPlatform;

impl PlatformImpl for NoopPlatform {
    fn render(&mut self, commands: &CommandList, damage: &[PhysicalRect]) {
        black_box((commands.len(), damage.len()));
    }

    fn screen(&mut self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: 1280,
            height: 8192,
        }
    }

    fn repaint_buffer(&self) -> RepaintBuffer {
        RepaintBuffer::Reused
    }

    fn create_image(&mut self, _: ImageData) -> ImageId {
        ImageId(0)
    }

    fn drop_image(&mut self, _: ImageId) {}

    fn create_string(&mut self, _: StringData) -> StringId {
        StringId(0)
    }

    fn drop_string(&mut self, _: StringId) {}

    fn string(&self, _: StringId) -> &str {
        unreachable!()
    }

    fn text_offset_at_position(&mut self, _: &paint::TextRequest, _: LogicalPoint) -> usize {
        unreachable!()
    }

    fn measure_text(&mut self, request: &TextLayoutRequest) -> LogicalSize {
        LogicalSize {
            width: request.max_width.unwrap_or(80.0).min(80.0),
            height: 16.0,
        }
    }

    fn text_cursor_rect(&mut self, _: &paint::TextRequest, _: usize) -> LogicalRect {
        unreachable!()
    }

    fn show_keyboard(&mut self, _: &KeyboardRequest<'_>) {}
}

pub fn layout_frame(ui: &mut Ui) {
    let mut column = ui.column(Container::new().width(Sizing::grow()).gap(2.0));
    for _ in 0..ROWS {
        let mut row = column.row(
            Container::new()
                .width(Sizing::grow())
                .height(Sizing::fixed(20.0))
                .gap(4.0),
        );
        row.add(Rectangle::new().width(Sizing::fixed(120.0)));
        row.add(Rectangle::new().width(Sizing::grow()));
        row.add(Rectangle::new().width(Sizing::fixed(80.0)));
    }
}

pub fn command_frame(ui: &mut Ui) {
    let mut column = ui.column(Container::new().width(Sizing::grow()).gap(2.0));
    for row_index in 0..ROWS {
        let mut row = column.row(
            Container::new()
                .width(Sizing::grow())
                .height(Sizing::fixed(20.0))
                .gap(4.0),
        );
        for (cell_index, width) in [Sizing::fixed(120.0), Sizing::grow(), Sizing::fixed(80.0)]
            .into_iter()
            .enumerate()
        {
            let id = WidgetId::new(row_index * CELLS_PER_ROW + cell_index);
            row.add(
                Rectangle::new()
                    .width(width)
                    .background(Color::BLACK)
                    .interact(id, Sense::CLICK),
            );
        }
    }
}
