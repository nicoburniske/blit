use std::hint::black_box;

use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_tui_render::{
    RendererConfig, TuiRenderer,
    color::Color,
    command_list::{Block, ClipId, CommandList},
};
use divan::counter::ItemsCount;

const CHANGED_CELL: PhysicalRect = PhysicalRect {
    x: 40,
    y: 12,
    width: 1,
    height: 1,
};

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[divan::bench(args = [(96, 32), (240, 80), (400, 200)])]
fn renderer_creation(bencher: divan::Bencher, (columns, rows): (u16, u16)) {
    bencher.bench(|| {
        TuiRenderer::new(
            RendererConfig::new()
                .columns(black_box(columns))
                .rows(black_box(rows)),
        )
    });
}

#[divan::bench]
fn render_full(bencher: divan::Bencher) {
    let mut renderer = renderer();
    let screen = renderer.screen();
    let commands = commands(screen, false);
    let damage = [screen];
    bencher
        .counter(ItemsCount::new((screen.width * screen.height) as usize))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

#[divan::bench]
fn render_one_cell(bencher: divan::Bencher) {
    let mut renderer = renderer();
    let screen = renderer.screen();
    let commands = commands(screen, true);
    renderer.render(&commands, &[screen]);
    bencher
        .counter(ItemsCount::new(1usize))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&[CHANGED_CELL])));
}

#[divan::bench]
fn update_one_cell(bencher: divan::Bencher) {
    let mut renderer = renderer();
    let screen = renderer.screen();
    let old = commands(screen, false);
    let new = commands(screen, true);
    renderer.render(&old, &[screen]);
    let mut changed = true;
    bencher.bench_local(|| {
        renderer.render(
            black_box(if changed { &new } else { &old }),
            black_box(&[CHANGED_CELL]),
        );
        black_box(renderer.output());
        changed = !changed;
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
            commands.push_block(
                Block::new(area).background(Color::Rgb(
                    (column * 19) as u8,
                    (row * 31) as u8,
                    ((row + column) * 13) as u8,
                )),
                area.to_physical(Scale2::IDENTITY),
                ClipId::default(),
            );
        }
    }
    if changed {
        let area = CHANGED_CELL.to_logical(Scale2::IDENTITY);
        commands.push_block(
            Block::new(area).background(Color::Rgb(86, 211, 194)),
            area.to_physical(Scale2::IDENTITY),
            ClipId::default(),
        );
    }
    commands
}

fn renderer() -> TuiRenderer {
    TuiRenderer::new(RendererConfig::new().columns(96).rows(32))
}
