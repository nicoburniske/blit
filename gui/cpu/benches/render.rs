use std::{hint::black_box, time::Duration};

use blit::{FrameInfo, Kernel, LogicalSize, Scope, Ui, input::Input};
use blit_cpu::{Renderer, RendererConfig, Scanline, VecBuffer, Xrgb8888};
use blit_gui::{
    color::Color,
    rectangle::Rectangle,
    shadow::Shadow,
    style::{Border, BorderRadius},
    text::{FontId, Text, TextOptions, TextOverflow, TextStyle, TextWrap},
};
use blit_text_parley::parley::{FontFamilyName, fontique::Blob};
use blit_widget::layout::{Sizing, flex, grid};

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

const WIDTH: usize = 1280;
const HEIGHT: usize = 720;

const PARAGRAPH: &str = "blit builds the full interface tree every frame without retained widget state, \
    resolving layout and damage so only changed tiles repaint on the cpu.";

fn main() {
    divan::main()
}

/// full text heavy page redrawn every frame
#[divan::bench]
fn text_page_full(bencher: divan::Bencher) {
    let mut renderer = make_renderer(1024, 1024 * 1024);
    let mut kernel = Kernel::default();
    let lines: Vec<String> = (0..64)
        .map(|i| format!("row {i:03}  status=ok  latency=0.{:02}ms  {PARAGRAPH}", (i * 17) % 99))
        .collect();
    run_text_page(&mut kernel, &mut renderer, &lines, 0, 0);
    bencher.bench_local(|| {
        run_text_page(&mut kernel, &mut renderer, &lines, 0, 0);
        black_box(renderer.buffer().pixels().len());
    });
}

/// same text heavy page with partial damage and one changing counter
#[divan::bench]
fn text_page_partial(bencher: divan::Bencher) {
    let mut renderer = make_renderer(1024, 1024 * 1024);
    let mut kernel = Kernel::default();
    let lines: Vec<String> = (0..64)
        .map(|i| format!("row {i:03}  status=ok  latency=0.{:02}ms  {PARAGRAPH}", (i * 17) % 99))
        .collect();
    let mut frame = 0u64;
    run_text_page(&mut kernel, &mut renderer, &lines, frame, 0);
    bencher.bench_local(|| {
        frame += 1;
        run_text_page(&mut kernel, &mut renderer, &lines, frame, 1);
        black_box(renderer.buffer().pixels().len());
    });
}

/// scrolling text window under cache pressure
#[divan::bench]
fn text_scroll_churn(bencher: divan::Bencher) {
    // budget fits the 36 visible rows plus header while evicting scrolled off lines
    let mut renderer = make_renderer(48, 256 * 1024);
    let mut kernel = Kernel::default();
    let pool: Vec<String> = (0..600)
        .map(|i| {
            format!(
                "[{i:04}] commit {:08x}  author=user{:02}  message: updated tile damage pass and text cache #{i}",
                i * 2654435761u32,
                i % 24
            )
        })
        .collect();
    let mut offset = 0usize;
    let body = TextStyle::new().size(13.0).color(Color::rgb(220, 224, 232));
    let header = TextStyle::new().size(15.0).color(Color::rgb(245, 247, 252));
    let draw = |kernel: &mut Kernel<_>, renderer: &mut BenchRenderer, offset: usize, age: u8| {
        let screen = LogicalSize::new(WIDTH as f32, HEIGHT as f32);
        kernel.build(
            renderer,
            FrameInfo::new(screen),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, _>| {
                let mut root = ui.layout(flex::column().gap(4.0)).pad(12.0);
                root.insert(Rectangle::new().background(Color::rgb(18, 20, 26)));
                root.child()
                    .insert(Text::new("activity log: hot chrome header").style(header));
                for row in 0..36 {
                    let index = (offset + row) % pool.len();
                    root.child_in(Scope::ROOT, index)
                        .insert(Text::new(&pool[index]).style(body));
                }
            },
        );
        kernel.layout(renderer);
        kernel.paint(renderer);
        renderer.render(age);
    };
    draw(&mut kernel, &mut renderer, 0, 0);
    bencher.bench_local(|| {
        offset = (offset + 4) % pool.len();
        draw(&mut kernel, &mut renderer, offset, 1);
        black_box(renderer.buffer().pixels().len());
    });
}

/// grid of rounded bordered cards with shadows and ellipsized wrapped text
#[divan::bench]
fn text_cards_ellipsized(bencher: divan::Bencher) {
    let mut renderer = make_renderer(1024, 1024 * 1024);
    let mut kernel = Kernel::default();
    let titles: Vec<String> = (0..24).map(|i| format!("card {i:02}: rendering metrics")).collect();
    let title_style = TextStyle::new().size(14.0).color(Color::rgb(240, 244, 250));
    let body_style = TextStyle::new().size(12.0).color(Color::rgb(170, 178, 192));
    let options = TextOptions::new()
        .wrap(TextWrap::Word)
        .overflow(TextOverflow::Ellipsis)
        .max_lines(Some(2));
    let draw = |kernel: &mut Kernel<_>, renderer: &mut BenchRenderer| {
        let screen = LogicalSize::new(WIDTH as f32, HEIGHT as f32);
        kernel.build(
            renderer,
            FrameInfo::new(screen),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, _>| {
                let mut root = ui.layout(grid::new(4).gap(12.0)).pad(16.0);
                root.insert(Rectangle::new().background(Color::rgb(16, 18, 24)));
                for title in &titles {
                    let mut card = root.child().layout(flex::column().gap(6.0)).pad(10.0);
                    card.insert(
                        Shadow::new(Color::rgba(0, 0, 0, 90))
                            .blur(10.0)
                            .offset_y(4.0)
                            .radius(BorderRadius::all(8.0)),
                    );
                    card.insert(
                        Rectangle::new()
                            .background(Color::rgb(28, 31, 40))
                            .border(Border::solid(1.0, Color::rgb(52, 58, 72)))
                            .radius(BorderRadius::all(8.0)),
                    );
                    card.child().insert(Text::new(title).style(title_style));
                    card.child()
                        .item(flex::item().width(Sizing::grow()))
                        .insert(Text::new(PARAGRAPH).style(body_style).options(options));
                }
            },
        );
        kernel.layout(renderer);
        kernel.paint(renderer);
        renderer.render(0);
    };
    draw(&mut kernel, &mut renderer);
    bencher.bench_local(|| {
        draw(&mut kernel, &mut renderer);
        black_box(renderer.buffer().pixels().len());
    });
}

type BenchRenderer = Renderer<VecBuffer<Xrgb8888>, Scanline>;

fn run_text_page(
    kernel: &mut Kernel<BenchRenderer>,
    renderer: &mut BenchRenderer,
    lines: &[String],
    frame: u64,
    age: u8,
) {
    let screen = LogicalSize::new(WIDTH as f32, HEIGHT as f32);
    let header_style = TextStyle::new().size(16.0).color(Color::rgb(244, 246, 250));
    let body_style = TextStyle::new().size(12.0).color(Color::rgb(205, 212, 224));
    let body_options = TextOptions::new().wrap(TextWrap::Word);
    kernel.build(
        renderer,
        FrameInfo::new(screen),
        Duration::ZERO,
        Input::None,
        |ui: Ui<'_, _>| {
            let mut root = ui.layout(flex::column().gap(4.0)).pad(12.0);
            root.insert(Rectangle::new().background(Color::rgb(18, 20, 26)));

            {
                let mut top = root.child().layout(flex::row().gap(16.0));
                top.child()
                    .insert(Text::new("blit text frame benchmark").style(header_style));
                top.child()
                    .insert(Text::new(format_args!("frame {frame:06}")).style(header_style));
            }

            let mut columns = root.child().item(flex::item().grow()).layout(flex::row().gap(12.0));
            for chunk in lines.chunks(32) {
                let mut col = columns
                    .child()
                    .item(flex::item().width(Sizing::grow()))
                    .layout(flex::column().gap(2.0));
                for line in chunk {
                    col.child()
                        .item(flex::item().width(Sizing::grow()))
                        .insert(Text::new(line).style(body_style).options(body_options));
                }
            }
        },
    );
    kernel.layout(renderer);
    kernel.paint(renderer);
    renderer.render(age);
}

fn make_renderer(text_cache: usize, glyph_cache: usize) -> BenchRenderer {
    let mut text = blit_text_parley::Backend::default().cache_capacity(text_cache);
    let family = std::env::var("BLIT_TEST_FONT")
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| {
            let registered = text
                .fonts
                .collection
                .register_fonts(Blob::new(std::sync::Arc::new(bytes)), None);
            Some(FontFamilyName::Named(
                text.fonts.collection.family_name(registered.first()?.0)?.to_owned().into(),
            ))
        })
        .unwrap_or_else(|| FontFamilyName::named("monospace"));
    let text = text.font(FontId::default(), [family]);
    Renderer::new(
        VecBuffer::new(WIDTH, HEIGHT),
        Box::new(text),
        RendererConfig {
            glyph_cache_capacity: glyph_cache,
            ..RendererConfig::default()
        },
    )
    .strategy(Scanline::default())
}
