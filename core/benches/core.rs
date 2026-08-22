use std::{hint::black_box, time::Duration};

use blit::{
    Ui, UiState,
    color::Color,
    command_list::{ClipId, CommandList, CommandListDiffer, Rectangle},
    geometry::{LogicalRect, PhysicalRect},
    input::Input,
    render,
};
use divan::counter::ItemsCount;

use support::{CELLS_PER_ROW, ITEMS, NoopRenderer, command_frame, layout_frame, transition_frame};

mod support;

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[divan::bench]
fn layout(bencher: divan::Bencher) {
    benchmark_frame(bencher, layout_frame)
}

#[divan::bench]
fn layout_with_position_transition(bencher: divan::Bencher) {
    let mut renderer = NoopRenderer;
    let mut state = benchmark_state();
    let mut time = Duration::ZERO;
    let mut right = false;
    render(&mut renderer, &mut state, time, [Input::None], |ui| {
        transition_frame(ui, right)
    });
    bencher.counter(ItemsCount::new(ITEMS)).bench_local(|| {
        time += Duration::from_millis(16);
        right = !right;
        render(&mut renderer, &mut state, time, [Input::None], |ui| {
            transition_frame(ui, right)
        })
    });
}

#[divan::bench]
fn layout_and_commands(bencher: divan::Bencher) {
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

fn benchmark_frame(bencher: divan::Bencher, frame: fn(&mut Ui)) {
    let mut renderer = NoopRenderer;
    let mut state = benchmark_state();
    render(
        &mut renderer,
        &mut state,
        Duration::ZERO,
        [Input::None],
        frame,
    );
    render(
        &mut renderer,
        &mut state,
        Duration::ZERO,
        [Input::None],
        frame,
    );
    bencher.counter(ItemsCount::new(ITEMS)).bench_local(|| {
        render(
            &mut renderer,
            &mut state,
            Duration::ZERO,
            [Input::None],
            frame,
        )
    });
}

fn benchmark_state() -> UiState {
    UiState::new(
        PhysicalRect {
            x: 0,
            y: 0,
            width: 1280,
            height: 8192,
        },
        blit::RepaintBuffer::Reused,
        1.0,
    )
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
