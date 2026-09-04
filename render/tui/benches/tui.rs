use std::hint::black_box;

use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_tui_render::{
    RendererConfig, TuiRenderer,
    cell::{Cell, CellStyle},
    color::Color,
    text::TextRequest,
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

#[divan::bench(args = [(96, 32), (240, 80), (400, 200)])]
fn render_screen(bencher: divan::Bencher, (columns, rows): (u16, u16)) {
    let mut renderer = TuiRenderer::new(RendererConfig::new().columns(columns).rows(rows));
    let screen = renderer.screen().to_logical(Scale2::IDENTITY);
    bencher
        .counter(ItemsCount::new(usize::from(columns) * usize::from(rows)))
        .bench_local(|| {
            renderer.begin_frame();
            renderer
                .cells(screen, screen)
                .clear(Cell::default().style(CellStyle::new().background(Color::Rgb(20, 30, 40))));
            renderer.end_frame();
            black_box(renderer.output());
        });
}

#[divan::bench]
fn render_scene(bencher: divan::Bencher) {
    let mut renderer = renderer();
    bencher.bench_local(|| {
        scene(&mut renderer, false);
        black_box(renderer.output());
    });
}

#[divan::bench]
fn update_one_cell(bencher: divan::Bencher) {
    let mut renderer = renderer();
    scene(&mut renderer, false);
    let mut changed = true;
    bencher.bench_local(|| {
        scene(&mut renderer, changed);
        black_box(renderer.output());
        changed = !changed;
    });
}

#[divan::bench]
fn update_one_cell_in_large_text(bencher: divan::Bencher) {
    let mut renderer = TuiRenderer::new(RendererConfig::new().columns(200).rows(50));
    let screen = renderer.screen();
    let frame = vec!["A".repeat(screen.width as usize); screen.height as usize].join("\n");
    let mut changed_frame = frame.clone();
    let changed_byte =
        CHANGED_CELL.y as usize * (screen.width as usize + 1) + CHANGED_CELL.x as usize;
    changed_frame.replace_range(changed_byte..changed_byte + 1, "B");
    let area = screen.to_logical(Scale2::IDENTITY);
    let old = renderer.text_run(&frame);
    let new = renderer.text_run(&changed_frame);
    renderer.begin_frame();
    renderer.paint_text(TextRequest::new(old, area), area);
    renderer.end_frame();
    let mut changed = true;
    bencher.counter(ItemsCount::new(1usize)).bench_local(|| {
        renderer.begin_frame();
        renderer.paint_text(
            TextRequest::new(if changed { new } else { old }, area),
            area,
        );
        renderer.end_frame();
        black_box(renderer.output());
        changed = !changed;
    });
}

fn scene(renderer: &mut TuiRenderer, changed: bool) {
    let screen = renderer.screen().to_logical(Scale2::IDENTITY);
    renderer.begin_frame();
    for row in 0..8 {
        for column in 0..12 {
            let area = LogicalRect {
                x: column as f32 * 8.0,
                y: row as f32 * 4.0,
                width: 8.0,
                height: 4.0,
            };
            renderer
                .cells(area, screen)
                .clear(
                    Cell::default().style(CellStyle::new().background(Color::Rgb(
                        (column * 19) as u8,
                        (row * 31) as u8,
                        ((row + column) * 13) as u8,
                    ))),
                );
        }
    }
    if changed {
        renderer
            .cells(CHANGED_CELL.to_logical(Scale2::IDENTITY), screen)
            .set_cell(
                0,
                0,
                Cell::new('x').style(CellStyle::new().foreground(Color::WHITE)),
            );
    }
    renderer.end_frame();
}

fn renderer() -> TuiRenderer {
    TuiRenderer::new(RendererConfig::new().columns(96).rows(32))
}
