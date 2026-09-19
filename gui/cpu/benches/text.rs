use std::hint::black_box;

use blit::{LogicalPoint, LogicalRect};
use blit_gui::{
    FontData, FontFamily, TextConfig, TextSystem,
    color::Color,
    text::{
        FontId, HorizontalAlign, TextLayoutRequest, TextOptions, TextRequest, TextStyle, TextWrap,
    },
};

const TEXT: &str = "Passport keeps your keys offline while making secure approvals clear and deliberate. Every transaction is reviewed on the trusted display before it is signed. Recovery information stays under your control, and the device never needs to expose private keys.";

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[divan::bench]
fn text_run_cached(bencher: divan::Bencher) {
    let mut text = text_system(1024 * 1024);
    text.text_run(TEXT, TextStyle::default());

    bencher.bench_local(|| text.text_run(black_box(TEXT), black_box(TextStyle::default())));
}

#[divan::bench]
fn layout_cached(bencher: divan::Bencher) {
    let mut text = text_system(1024 * 1024);
    let request = TextLayoutRequest {
        text: text.text_run(TEXT, TextStyle::default()),
        wrap: TextWrap::Word,
        max_width: Some(320.0),
        max_lines: None,
    };
    text.measure(&request);

    bencher.bench_local(|| text.measure(black_box(&request)));
}

#[divan::bench]
fn rewrap_uncached(bencher: divan::Bencher) {
    let mut text = text_system(0);
    let run = text.text_run(TEXT, TextStyle::default());
    let mut width = 240.0f32;

    bencher.bench_local(|| {
        width = if width < 400.0 { width + 0.125 } else { 240.0 };
        let size = text.measure(&TextLayoutRequest {
            text: run,
            wrap: TextWrap::Word,
            max_width: Some(black_box(width)),
            max_lines: None,
        });
        text.finish_frame();
        size
    });
}

#[divan::bench]
fn measure_then_paint_uncached(bencher: divan::Bencher) {
    let mut text = text_system(0);
    let run = text.text_run(TEXT, TextStyle::default());
    let measure = TextLayoutRequest {
        text: run,
        wrap: TextWrap::Word,
        max_width: Some(320.0),
        max_lines: None,
    };
    let paint = TextRequest {
        text: run,
        area: LogicalRect::new(0.0, 0.0, 320.0, 240.0),
        offset_x: 0.0,
        color: Color::BLACK,
        options: TextOptions {
            wrap: TextWrap::Word,
            ..TextOptions::default()
        },
    };

    bencher.bench_local(|| {
        let size = text.measure(black_box(&measure));
        let layout = text.paint_layout(black_box(&paint));
        black_box((size, layout.layout.size));
        text.finish_frame();
    });
}

#[divan::bench]
fn placement_changed(bencher: divan::Bencher) {
    let mut text = text_system(32 * 1024 * 1024);
    let run = text.text_run(TEXT, TextStyle::default());
    let mut height = 200.0f32;

    bencher.bench_local(|| {
        height = if height < 360.0 {
            height + 0.125
        } else {
            200.0
        };
        let request = TextRequest {
            text: run,
            area: LogicalRect::new(0.0, 0.0, 320.0, black_box(height)),
            offset_x: 0.0,
            color: Color::BLACK,
            options: TextOptions {
                wrap: TextWrap::Word,
                horizontal_align: HorizontalAlign::Center,
                ..TextOptions::default()
            },
        };
        let size = text.paint_layout(&request).layout.size;
        size
    });
}

#[divan::bench]
fn layout_without_caret_query(bencher: divan::Bencher) {
    let mut text = text_system(0);
    let run = text.text_run(TEXT, TextStyle::default());
    let request = TextLayoutRequest {
        text: run,
        wrap: TextWrap::Word,
        max_width: Some(320.0),
        max_lines: None,
    };

    bencher.bench_local(|| {
        let size = text.measure(black_box(&request));
        text.finish_frame();
        size
    });
}

#[divan::bench]
fn layout_with_caret_query(bencher: divan::Bencher) {
    let mut text = text_system(0);
    let request = TextRequest {
        text: text.text_run(TEXT, TextStyle::default()),
        area: LogicalRect::new(0.0, 0.0, 320.0, 240.0),
        offset_x: 0.0,
        color: Color::BLACK,
        options: TextOptions {
            wrap: TextWrap::Word,
            ..TextOptions::default()
        },
    };

    bencher.bench_local(|| {
        let offset = text.offset_at_position(
            black_box(&request),
            black_box(LogicalPoint { x: 140.0, y: 42.0 }),
        );
        text.finish_frame();
        offset
    });
}

fn text_system(layout_cache_capacity: usize) -> TextSystem {
    TextSystem::new(
        TextConfig {
            fonts: vec![FontFamily {
                id: FontId::default(),
                fonts: vec![FontData::Static(include_bytes!(env!("BLIT_TEST_FONT")))],
            }],
            text_cache_capacity: 1024 * 1024,
            layout_cache_capacity,
        },
        blit_text_cosmic::Backend::without_system_fonts(),
    )
    .unwrap()
}
