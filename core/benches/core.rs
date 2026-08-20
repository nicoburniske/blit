use std::{hint::black_box, time::Duration};

use blit::{
    Element, Layout, RepaintBuffer, Runtime, Sizing, Ui,
    color::Color,
    command_list::{ClipId, CommandList, CommandListDiffer},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    input::Input,
    interact::{Sense, WidgetId},
    keyboard::KeyboardRequest,
    paint::{self, Rectangle},
    platform::PlatformImpl,
    resource::{ImageData, ImageId, StringData, StringId},
};
use divan::counter::ItemsCount;

const ROWS: usize = 256;
const CELLS_PER_ROW: usize = 3;
const ITEMS: usize = ROWS * CELLS_PER_ROW;

fn main() {
    divan::main()
}

#[divan::bench]
fn element_layout(bencher: divan::Bencher) {
    benchmark_frame(bencher, layout_frame)
}

#[divan::bench]
fn element_layout_and_commands(bencher: divan::Bencher) {
    benchmark_frame(bencher, command_frame)
}

#[divan::bench(args = [
    DiffCase::Unchanged,
    DiffCase::OneChange,
    DiffCase::SparseChanges,
    DiffCase::AllChanged,
    DiffCase::InsertRemove,
])]
fn command_diff(bencher: divan::Bencher, case: DiffCase) {
    let old = command_list(case, false);
    let new = command_list(case, true);
    let mut differ = CommandListDiffer::default();
    black_box(differ.diff(&old, &new));

    bencher.counter(ItemsCount::new(ITEMS)).bench_local(|| {
        black_box(differ.diff(&old, &new));
    });
}

#[derive(Clone, Copy, Debug)]
enum DiffCase {
    Unchanged,
    OneChange,
    SparseChanges,
    AllChanged,
    InsertRemove,
}

struct NoopPlatform;

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

    fn measure_text(&mut self, _: &paint::TextLayoutRequest) -> LogicalSize {
        unreachable!()
    }

    fn text_cursor_rect(&mut self, _: &paint::TextRequest, _: usize) -> LogicalRect {
        unreachable!()
    }

    fn show_keyboard(&mut self, _: &KeyboardRequest<'_>) {}
}

fn benchmark_frame(bencher: divan::Bencher, frame: fn(&mut Ui)) {
    let mut runtime = Runtime::new(NoopPlatform);
    runtime.render(Duration::ZERO, Input::None, frame);
    bencher
        .counter(ItemsCount::new(ITEMS))
        .bench_local(|| runtime.render(Duration::ZERO, Input::None, frame));
}

fn layout_frame(ui: &mut Ui) {
    let mut column = ui.element(Element::new(
        Layout::vertical().width(Sizing::grow()).gap(2.0),
    ));
    for _ in 0..ROWS {
        let mut row = column.element(Element::new(
            Layout::horizontal()
                .width(Sizing::grow())
                .height(Sizing::fixed(20.0))
                .gap(4.0),
        ));
        drop(row.element(Element::new(Layout::vertical().width(Sizing::fixed(120.0)))));
        drop(row.element(Element::new(Layout::vertical().width(Sizing::grow()))));
        drop(row.element(Element::new(Layout::vertical().width(Sizing::fixed(80.0)))));
    }
}

fn command_frame(ui: &mut Ui) {
    let mut column = ui.element(Element::new(
        Layout::vertical().width(Sizing::grow()).gap(2.0),
    ));
    for row_index in 0..ROWS {
        let mut row = column.element(Element::new(
            Layout::horizontal()
                .width(Sizing::grow())
                .height(Sizing::fixed(20.0))
                .gap(4.0),
        ));
        for (cell_index, width) in [Sizing::fixed(120.0), Sizing::grow(), Sizing::fixed(80.0)]
            .into_iter()
            .enumerate()
        {
            let id = WidgetId::new(row_index * CELLS_PER_ROW + cell_index);
            black_box(drop(
                row.element(
                    Element::new(Layout::vertical().width(width))
                        .background(Color::BLACK)
                        .interact(id, Sense::CLICK),
                ),
            ));
        }
    }
}

fn command_list(case: DiffCase, new: bool) -> CommandList {
    let mut commands = CommandList::default();
    for position in 0..ITEMS {
        let index = if new && matches!(case, DiffCase::InsertRemove) {
            let from = ITEMS / 4;
            let to = ITEMS * 3 / 4;
            match position {
                position if position < from => position,
                position if position < to => position + 1,
                position if position == to => from,
                position => position,
            }
        } else {
            position
        };
        let area = LogicalRect {
            x: (index % CELLS_PER_ROW) as f32 * 84.0,
            y: (index / CELLS_PER_ROW) as f32 * 22.0,
            width: 80.0,
            height: 20.0,
        };
        let changed = new
            && match case {
                DiffCase::Unchanged | DiffCase::InsertRemove => false,
                DiffCase::OneChange => index == ITEMS / 2,
                DiffCase::SparseChanges => index % 64 == 32,
                DiffCase::AllChanged => true,
            };
        commands.push_rectangle(
            Rectangle::new(area).background(if changed { Color::WHITE } else { Color::BLACK }),
            area.to_physical(1.0),
            ClipId::default(),
        );
    }
    commands
}
