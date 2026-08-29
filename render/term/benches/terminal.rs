use std::{hint::black_box, io, time::Duration};

use blit::{
    UiState,
    color::Color,
    command_list::{ClipId, CommandList, Rectangle},
    geometry::{LogicalPoint, LogicalRect, PhysicalRect},
    input::{Input, Modifiers},
    renderer::Renderer as _,
    repaint::{IncrementalRepaint, MyersTracker},
};
use blit_showcase::{Page, Showcase};
use blit_term::{CELL_HEIGHT, CELL_WIDTH, TerminalRenderer};
use divan::counter::ItemsCount;

const MICRO_COLUMNS: u16 = 96;
const MICRO_ROWS: u16 = 32;
const SHOWCASE_COLUMNS: u16 = 140;
const SHOWCASE_ROWS: u16 = 50;

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

#[derive(Clone, Copy, Debug)]
enum ShowcaseUpdate {
    Stable,
    Pointer,
}

fn main() {
    divan::main()
}

#[divan::bench]
fn render_full(bencher: divan::Bencher) {
    let commands = commands(false);
    let damage = [screen(MICRO_COLUMNS, MICRO_ROWS)];
    let mut renderer = TerminalRenderer::new(MICRO_COLUMNS, MICRO_ROWS);
    bencher
        .counter(ItemsCount::new(
            usize::from(MICRO_COLUMNS) * usize::from(MICRO_ROWS),
        ))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

#[divan::bench]
fn render_one_cell(bencher: divan::Bencher) {
    let commands = commands(true);
    let damage = [PhysicalRect {
        x: 40 * CELL_WIDTH as i32,
        y: 12 * CELL_HEIGHT as i32,
        width: CELL_WIDTH as i32,
        height: CELL_HEIGHT as i32,
    }];
    let mut renderer = TerminalRenderer::new(MICRO_COLUMNS, MICRO_ROWS);
    renderer.render(&commands, &[screen(MICRO_COLUMNS, MICRO_ROWS)]);
    bencher
        .counter(ItemsCount::new(1usize))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

#[divan::bench]
fn present_unchanged(bencher: divan::Bencher) {
    let commands = commands(false);
    let mut renderer = TerminalRenderer::new(MICRO_COLUMNS, MICRO_ROWS);
    renderer.render(&commands, &[screen(MICRO_COLUMNS, MICRO_ROWS)]);
    renderer.present(&mut io::sink()).unwrap();
    bencher.bench_local(|| renderer.present(black_box(&mut io::sink())).unwrap());
}

#[divan::bench]
fn update_one_cell(bencher: divan::Bencher) {
    let old = commands(false);
    let new = commands(true);
    let damage = [PhysicalRect {
        x: 40 * CELL_WIDTH as i32,
        y: 12 * CELL_HEIGHT as i32,
        width: CELL_WIDTH as i32,
        height: CELL_HEIGHT as i32,
    }];
    let mut renderer = TerminalRenderer::new(MICRO_COLUMNS, MICRO_ROWS);
    renderer.render(&old, &[screen(MICRO_COLUMNS, MICRO_ROWS)]);
    renderer.present(&mut io::sink()).unwrap();
    let mut changed = true;
    bencher.bench_local(|| {
        renderer.render(
            black_box(if changed { &new } else { &old }),
            black_box(&damage),
        );
        renderer.present(black_box(&mut io::sink())).unwrap();
        changed = !changed;
    });
}

#[divan::bench(args = [
    ShowcaseUpdate::Stable,
    ShowcaseUpdate::Pointer,
])]
fn showcase_incremental(bencher: divan::Bencher, update: ShowcaseUpdate) {
    let mut renderer = TerminalRenderer::new(SHOWCASE_COLUMNS, SHOWCASE_ROWS);
    let mut state = UiState::new(renderer.screen(), 1.0);
    let mut repaint = IncrementalRepaint::new(MyersTracker::default(), false);
    let mut showcase = Showcase::default();
    blit::render(
        &mut renderer,
        &mut state,
        &mut repaint,
        Duration::ZERO,
        [],
        |ui| showcase.render(ui),
    );
    renderer.present(&mut io::sink()).unwrap();

    let mut alternate = false;
    bencher.bench_local(|| {
        let input = match update {
            ShowcaseUpdate::Stable => Input::None,
            ShowcaseUpdate::Pointer => Input::PointerMove {
                position: LogicalPoint {
                    x: if alternate { 620.0 } else { 760.0 },
                    y: 52.0,
                },
                modifiers: Modifiers::NONE,
            },
        };
        blit::render(
            black_box(&mut renderer),
            black_box(&mut state),
            black_box(&mut repaint),
            Duration::ZERO,
            [black_box(input)],
            |ui| showcase.render(ui),
        );
        renderer.present(black_box(&mut io::sink())).unwrap();
        alternate = !alternate;
    });
}

#[divan::bench(args = [
    Page::Layout,
    Page::Scrolling,
    Page::Input,
    Page::Images,
    Page::Animation,
])]
fn showcase_full(bencher: divan::Bencher, page: Page) {
    let mut renderer = TerminalRenderer::new(SHOWCASE_COLUMNS, SHOWCASE_ROWS);
    let mut state = UiState::new(renderer.screen(), 1.0);
    let mut repaint = IncrementalRepaint::new(MyersTracker::default(), false);
    let mut showcase = Showcase::default();
    showcase.set_page(page);
    blit::render(
        &mut renderer,
        &mut state,
        &mut repaint,
        Duration::ZERO,
        [],
        |ui| showcase.render(ui),
    );
    renderer.present(&mut io::sink()).unwrap();

    bencher.bench_local(|| {
        state.invalidate_all();
        blit::render(
            black_box(&mut renderer),
            black_box(&mut state),
            black_box(&mut repaint),
            Duration::ZERO,
            [Input::None],
            |ui| showcase.render(ui),
        );
        renderer.present(black_box(&mut io::sink())).unwrap();
    });
}

fn commands(changed: bool) -> CommandList {
    let mut commands = CommandList::default();
    commands.push_clear(screen(MICRO_COLUMNS, MICRO_ROWS));
    for row in 0..8 {
        for column in 0..12 {
            let area = LogicalRect {
                x: column as f32 * CELL_WIDTH * 8.0,
                y: row as f32 * CELL_HEIGHT * 4.0,
                width: CELL_WIDTH * 8.0,
                height: CELL_HEIGHT * 4.0,
            };
            commands.push_rectangle(
                Rectangle::new(area).background(Color::from_rgba8(
                    (column * 19) as u8,
                    (row * 31) as u8,
                    ((row + column) * 13) as u8,
                    255,
                )),
                area.to_physical(1.0),
                ClipId::default(),
            );
        }
    }
    if changed {
        let area = LogicalRect {
            x: 40.0 * CELL_WIDTH,
            y: 12.0 * CELL_HEIGHT,
            width: CELL_WIDTH,
            height: CELL_HEIGHT,
        };
        commands.push_rectangle(
            Rectangle::new(area).background(Color::from_rgba8(86, 211, 194, 255)),
            area.to_physical(1.0),
            ClipId::default(),
        );
    }
    commands
}

fn screen(columns: u16, rows: u16) -> PhysicalRect {
    PhysicalRect {
        x: 0,
        y: 0,
        width: i32::from(columns) * CELL_WIDTH as i32,
        height: i32::from(rows) * CELL_HEIGHT as i32,
    }
}
