use std::{hint::black_box, time::Duration};

use blit::{
    UiState,
    color::Color,
    command_list::{ClipId, CommandList, Rectangle},
    geometry::{LogicalPoint, LogicalRect, PhysicalRect, Scale2},
    input::{Input, Modifiers},
    renderer::Renderer as _,
    repaint::{IncrementalRepaint, MyersTracker},
};
use blit_showcase::{Page, Showcase};
use blit_term::{PIXEL_LIKE, RendererConfig, TerminalRenderer};
use divan::counter::ItemsCount;

const CHANGED_CELL: PhysicalRect = PhysicalRect {
    x: 40,
    y: 12,
    width: 1,
    height: 1,
};

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
    let mut renderer = micro_renderer();
    let screen = renderer.screen();
    let commands = commands(screen, false);
    let damage = [screen];
    bencher
        .counter(ItemsCount::new((screen.width * screen.height) as usize))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

#[divan::bench]
fn render_one_cell(bencher: divan::Bencher) {
    let mut renderer = micro_renderer();
    let screen = renderer.screen();
    let commands = commands(screen, true);
    let damage = [CHANGED_CELL];
    renderer.render(&commands, &[screen]);
    bencher
        .counter(ItemsCount::new(1usize))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

#[divan::bench]
fn update_one_cell(bencher: divan::Bencher) {
    let mut renderer = micro_renderer();
    let screen = renderer.screen();
    let old = commands(screen, false);
    let new = commands(screen, true);
    let damage = [CHANGED_CELL];
    renderer.render(&old, &[screen]);
    let mut changed = true;
    bencher.bench_local(|| {
        renderer.render(
            black_box(if changed { &new } else { &old }),
            black_box(&damage),
        );
        black_box(renderer.output());
        changed = !changed;
    });
}

#[divan::bench(args = [
    ShowcaseUpdate::Stable,
    ShowcaseUpdate::Pointer,
])]
fn showcase_incremental(bencher: divan::Bencher, update: ShowcaseUpdate) {
    let mut renderer = showcase_renderer();
    let mut state = UiState::default();
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
    black_box(renderer.output());

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
        black_box(renderer.output());
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
    let mut renderer = showcase_renderer();
    let mut state = UiState::default();
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
    black_box(renderer.output());

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
        black_box(renderer.output());
    });
}

fn commands(screen: PhysicalRect, changed: bool) -> CommandList {
    let mut commands = CommandList::default();
    commands.push_clear(screen);
    for row in 0..8 {
        for column in 0..12 {
            let area = LogicalRect {
                x: column as f32 * 8.0,
                y: row as f32 * 4.0,
                width: 8.0,
                height: 4.0,
            };
            commands.push_rectangle(
                Rectangle::new(area).background(Color::from_rgba8(
                    (column * 19) as u8,
                    (row * 31) as u8,
                    ((row + column) * 13) as u8,
                    255,
                )),
                area.to_physical(Scale2::IDENTITY),
                ClipId::default(),
            );
        }
    }
    if changed {
        let area = CHANGED_CELL.to_logical(Scale2::IDENTITY);
        commands.push_rectangle(
            Rectangle::new(area).background(Color::from_rgba8(86, 211, 194, 255)),
            area.to_physical(Scale2::IDENTITY),
            ClipId::default(),
        );
    }
    commands
}

fn micro_renderer() -> TerminalRenderer {
    TerminalRenderer::new(RendererConfig::new().columns(96).rows(32))
}

fn showcase_renderer() -> TerminalRenderer {
    TerminalRenderer::new(
        RendererConfig::new()
            .columns(140)
            .rows(50)
            .cell_size(PIXEL_LIKE),
    )
}
