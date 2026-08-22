use std::{hint::black_box, time::Duration};

use blit::{
    Ui,
    animation::Transition,
    color::Color,
    command_list::CommandList,
    container::{Absolute, Anchor, Scope, Sizing},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    image::{ImageData, ImageHandle, ImageId},
    interact::{Sense, WidgetId},
    renderer::Renderer,
    text::{self, TextLayoutRequest, TextRunId, TextStyle},
    widget::Rectangle,
};

pub const ROWS: usize = 256;
pub const CELLS_PER_ROW: usize = 3;
pub const ITEMS: usize = ROWS * CELLS_PER_ROW;

pub struct NoopRenderer;

impl Renderer for NoopRenderer {
    fn set_scale_factor(&mut self, _: f32) {}

    fn render(&mut self, commands: &CommandList, damage: &[PhysicalRect]) {
        black_box((commands.len(), damage.len()));
    }

    fn create_image(&mut self, data: ImageData) -> ImageHandle {
        ImageHandle::new(ImageId(0), data.size)
    }

    fn text_run(&mut self, _: &str, _: TextStyle) -> TextRunId {
        TextRunId(1)
    }

    fn text_offset_at_position(&mut self, _: &text::TextRequest, _: LogicalPoint) -> usize {
        unreachable!()
    }

    fn measure_text(&mut self, request: &TextLayoutRequest) -> LogicalSize {
        LogicalSize {
            width: request.max_width.unwrap_or(80.0).min(80.0),
            height: 16.0,
        }
    }

    fn text_cursor_rect(&mut self, _: &text::TextRequest, _: usize) -> LogicalRect {
        unreachable!()
    }
}

pub fn layout_frame(ui: &mut Ui) {
    let mut column = ui.container().col().width(Sizing::grow()).gap(2.0).open();
    layout_rows(&mut column)
}

pub fn transition_frame(ui: &mut Ui, right: bool) {
    let anchor = if right {
        Anchor::TopRight
    } else {
        Anchor::TopLeft
    };
    let mut column = ui
        .container()
        .col()
        .width(Sizing::fixed(640.0))
        .gap(2.0)
        .id(WidgetId::new("benchmark transition"))
        .absolute(Absolute::attach(anchor, anchor))
        .transition(Transition::new(Duration::from_millis(100)).position())
        .open();
    layout_rows(&mut column)
}

fn layout_rows(column: &mut Scope<'_>) {
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
