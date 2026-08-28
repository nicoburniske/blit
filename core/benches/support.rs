use std::{hint::black_box, time::Duration};

use blit::{
    Ui,
    animation::Transition,
    color::Color,
    command_list::CommandList,
    container::{Absolute, Anchor, Sizing, Slot},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    image::{ImageData, ImageHandle, ImageId},
    interact::{Sense, WidgetId},
    layout::{Flex, Grid, RectLayout, UnitScope, Wrap},
    renderer::Renderer,
    style::Style,
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
    let mut column = ui
        .layout(Flex::column().gap(2.0))
        .width(Sizing::grow())
        .open();
    layout_rows(&mut column, true)
}

pub fn transition_frame(ui: &mut Ui, right: bool) {
    let anchor = if right {
        Anchor::TopRight
    } else {
        Anchor::TopLeft
    };
    let mut column = ui
        .layout(Flex::column().gap(2.0))
        .width(Sizing::fixed(640.0))
        .id(WidgetId::new("benchmark transition"))
        .absolute(Absolute::attach(anchor, anchor))
        .transition(Transition::new(Duration::from_millis(100)).position())
        .open();
    layout_rows(&mut column, false)
}

fn layout_rows(column: &mut UnitScope<'_>, complex: bool) {
    for index in 0..ROWS {
        column.add(
            |ui: &mut Ui| match if complex { index % 4 } else { index % 2 } {
                0 => {
                    let mut row = ui
                        .layout(Flex::row().gap(4.0))
                        .width(Sizing::grow())
                        .height(Sizing::fixed(20.0))
                        .open();
                    row.add(Rectangle::new().slot(Slot::new().width(Sizing::fixed(120.0))));
                    row.add(Rectangle::new().slot(Slot::new().width(Sizing::grow())));
                    row.add(Rectangle::new().slot(Slot::new().width(Sizing::fixed(80.0))));
                }
                1 => {
                    let mut row = ui
                        .layout(RectLayout)
                        .width(Sizing::grow())
                        .height(Sizing::fixed(20.0))
                        .open();
                    for (x, width) in [(0.0, 120.0), (124.0, 400.0), (528.0, 80.0)] {
                        row.add(
                            LogicalRect {
                                x,
                                y: 0.0,
                                width,
                                height: 20.0,
                            },
                            Rectangle::new(),
                        );
                    }
                }
                2 => {
                    let mut row = ui
                        .layout(Grid::columns(3).spanning().gap(4.0))
                        .width(Sizing::grow())
                        .height(Sizing::fixed(20.0))
                        .open();
                    row.add_span(2, 2, Rectangle::new().slot(Slot::new().fixed(120.0, 8.0)));
                    row.add(Rectangle::new().slot(Slot::new().fixed(400.0, 8.0)));
                    row.add(Rectangle::new().slot(Slot::new().fixed(80.0, 8.0)));
                }
                _ => {
                    let mut row = ui
                        .layout(Wrap::horizontal().item_gap(4.0).run_gap(4.0))
                        .width(Sizing::grow())
                        .height(Sizing::fixed(20.0))
                        .open();
                    for width in [500.0; 3] {
                        row.add(Rectangle::new().slot(Slot::new().fixed(width, 8.0)));
                    }
                }
            },
        );
    }
}

pub fn layer_frame(ui: &mut Ui) {
    let overlay = ui.layer();
    for index in 0..ROWS {
        let mut item = ui
            .layout(Flex::column())
            .fixed(20.0, 20.0)
            .absolute(Absolute::at(index as f32, 0.0))
            .open();
        for _ in 0..CELLS_PER_ROW {
            item.add(Rectangle::new().slot(Slot::new().layer(overlay).fixed(4.0, 4.0)));
        }
    }
}

pub fn z_index_frame(ui: &mut Ui) {
    for index in 0..ROWS {
        let mut item = ui
            .layout(Flex::column())
            .fixed(20.0, 20.0)
            .z_index((index % 7) as i16 - 3)
            .absolute(Absolute::at(index as f32, 0.0))
            .open();
        for _ in 0..CELLS_PER_ROW {
            item.add(Rectangle::new().slot(Slot::new().fixed(4.0, 4.0)));
        }
    }
}

pub fn command_frame(ui: &mut Ui) {
    let mut column = ui
        .layout(Flex::column().gap(2.0))
        .width(Sizing::grow())
        .open();
    for row_index in 0..ROWS {
        column.add(|ui: &mut Ui| {
            for cell_index in 0..CELLS_PER_ROW {
                ui.interact(
                    WidgetId::new(row_index * CELLS_PER_ROW + cell_index),
                    Sense::CLICK,
                );
            }
            let mut row = ui
                .layout(Flex::row().gap(4.0))
                .width(Sizing::grow())
                .height(Sizing::fixed(20.0))
                .open();
            for (cell_index, width) in [Sizing::fixed(120.0), Sizing::grow(), Sizing::fixed(80.0)]
                .into_iter()
                .enumerate()
            {
                row.add(
                    Rectangle::new()
                        .slot(Slot::new().width(width))
                        .style(Style::new().background(Color::BLACK))
                        .id(WidgetId::new(row_index * CELLS_PER_ROW + cell_index)),
                );
            }
        });
    }
}
