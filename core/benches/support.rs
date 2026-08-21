use std::hint::black_box;

use blit::{
    RepaintBuffer, Sizing, Ui,
    color::Color,
    command_list::CommandList,
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    interact::{Sense, WidgetId},
    paint::{self, TextLayoutRequest, TextRunId, TextStyle},
    platform::PlatformImpl,
    resource::{ImageData, ImageHandle, ImageId},
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

    fn create_image(&mut self, data: ImageData) -> ImageHandle {
        ImageHandle::new(ImageId(0), data.size)
    }

    fn text_run(&mut self, _: &str, _: TextStyle) -> TextRunId {
        TextRunId(1)
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
}

pub fn layout_frame(ui: &mut Ui) {
    let mut column = ui.container().col().width(Sizing::grow()).gap(2.0).open();
    for _ in 0..ROWS {
        let mut row = column
            .container()
            .row()
            .width(Sizing::grow())
            .height(Sizing::fixed(20.0))
            .gap(4.0)
            .open();
        row.add(Rectangle::new().width(Sizing::fixed(120.0)));
        row.add(Rectangle::new().width(Sizing::grow()));
        row.add(Rectangle::new().width(Sizing::fixed(80.0)));
    }
}

pub fn command_frame(ui: &mut Ui) {
    let mut column = ui.container().col().width(Sizing::grow()).gap(2.0).open();
    for row_index in 0..ROWS {
        let mut row = column
            .container()
            .row()
            .width(Sizing::grow())
            .height(Sizing::fixed(20.0))
            .gap(4.0)
            .open();
        for (cell_index, width) in [Sizing::fixed(120.0), Sizing::grow(), Sizing::fixed(80.0)]
            .into_iter()
            .enumerate()
        {
            let id = WidgetId::new(row_index * CELLS_PER_ROW + cell_index);
            row.interact(id, Sense::CLICK);
            row.add(
                Rectangle::new()
                    .width(width)
                    .background(Color::BLACK)
                    .id(id),
            );
        }
    }
}
