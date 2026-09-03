use std::hint::black_box;

use blit::{LogicalPoint, PhysicalRect, Scale2};
use blit_cpu::{
    Direct, FontData, FontFace, Pixel, PremultipliedRgbaColor, RenderStrategy, Renderer,
    RendererConfig, Scanline, TextLayoutEngine, VecBuffer, Xrgb8888,
    color::Color,
    command_list::{ClipId, CommandList, Polyline},
    text_types::FontId,
};
use divan::counter::ItemsCount;
use zeno::{Cap, Command as ZenoCommand, Join, Mask, PathBuilder, Scratch, Stroke};

const WIDTH: usize = 480;
const HEIGHT: usize = 256;
const SCALE: Scale2 = Scale2::IDENTITY;

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[divan::bench(args = [1.0, 4.0])]
fn single_line_direct(bencher: divan::Bencher, width: f32) {
    benchmark_polyline(bencher, Direct::default(), line_points(), width)
}

#[divan::bench(args = [1.0, 4.0])]
fn single_line_scanline(bencher: divan::Bencher, width: f32) {
    benchmark_polyline(bencher, Scanline::default(), line_points(), width)
}

#[divan::bench(args = [1.0, 4.0])]
fn single_line_zeno(bencher: divan::Bencher, width: f32) {
    benchmark_zeno(bencher, line_points(), width)
}

#[divan::bench(args = [1.0, 4.0])]
fn chart_direct(bencher: divan::Bencher, width: f32) {
    benchmark_polyline(bencher, Direct::default(), chart_points(), width)
}

#[divan::bench(args = [1.0, 4.0])]
fn chart_scanline(bencher: divan::Bencher, width: f32) {
    benchmark_polyline(bencher, Scanline::default(), chart_points(), width)
}

#[divan::bench(args = [1.0, 4.0])]
fn chart_segments_direct(bencher: divan::Bencher, width: f32) {
    benchmark_segments(bencher, Direct::default(), chart_points(), width)
}

#[divan::bench(args = [1.0, 4.0])]
fn chart_segments_scanline(bencher: divan::Bencher, width: f32) {
    benchmark_segments(bencher, Scanline::default(), chart_points(), width)
}

#[divan::bench(args = [1.0, 4.0])]
fn chart_zeno(bencher: divan::Bencher, width: f32) {
    benchmark_zeno(bencher, chart_points(), width)
}

fn line_points() -> Vec<LogicalPoint> {
    vec![
        LogicalPoint::new(12.0, 12.0),
        LogicalPoint::new(468.0, 244.0),
    ]
}

fn chart_points() -> Vec<LogicalPoint> {
    (0..256)
        .map(|index| {
            let x = 4.0 + index as f32 * 472.0 / 255.0;
            let y =
                128.0 + (index as f32 * 0.19).sin() * 72.0 + (index as f32 * 0.071).sin() * 28.0;
            LogicalPoint::new(x, y)
        })
        .collect()
}

fn benchmark_polyline<S>(
    bencher: divan::Bencher,
    strategy: S,
    points: Vec<LogicalPoint>,
    width: f32,
) where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    let polyline = Polyline::new(&points, Color::from_rgba8(38, 160, 240, 255)).width(width);
    let bounds = polyline.bounds().unwrap().to_physical(SCALE);
    let mut commands = CommandList::default();
    commands.push_polyline(polyline, bounds, ClipId::default());
    let damage = [PhysicalRect {
        width: WIDTH as i32,
        height: HEIGHT as i32,
        ..PhysicalRect::default()
    }];
    let mut renderer = renderer(strategy);
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(points.len() - 1))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn benchmark_zeno(bencher: divan::Bencher, points: Vec<LogicalPoint>, width: f32) {
    let mut path = Vec::<ZenoCommand>::with_capacity(points.len());
    path.move_to((points[0].x, points[0].y));
    for point in &points[1..] {
        path.line_to((point.x, point.y));
    }
    let mut stroke = Stroke::new(width);
    stroke.join(Join::Round).cap(Cap::Round);
    let mut scratch = Scratch::new();
    let mut mask = vec![0; WIDTH * HEIGHT];
    let mut pixels = vec![Xrgb8888::default(); WIDTH * HEIGHT];
    let color = PremultipliedRgbaColor::new(Color::from_rgba8(38, 160, 240, 255), 255);

    bencher
        .counter(ItemsCount::new(points.len() - 1))
        .bench_local(|| {
            Mask::with_scratch(black_box(&path), &mut scratch)
                .style(stroke)
                .size(WIDTH as u32, HEIGHT as u32)
                .render_into(black_box(&mut mask), None);
            for (pixel, coverage) in pixels.iter_mut().zip(&mask) {
                if *coverage != 0 {
                    pixel.blend(color.coverage(*coverage as u32));
                }
            }
            black_box((&mut scratch, &mut mask, &mut pixels));
        });
}

fn benchmark_segments<S>(
    bencher: divan::Bencher,
    strategy: S,
    points: Vec<LogicalPoint>,
    width: f32,
) where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    let mut commands = CommandList::default();
    for points in points.windows(2) {
        let polyline = Polyline::new(points, Color::from_rgba8(38, 160, 240, 255)).width(width);
        commands.push_polyline(
            polyline,
            polyline.bounds().unwrap().to_physical(SCALE),
            ClipId::default(),
        );
    }
    let damage = [PhysicalRect {
        width: WIDTH as i32,
        height: HEIGHT as i32,
        ..PhysicalRect::default()
    }];
    let mut renderer = renderer(strategy);
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(points.len() - 1))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn renderer<S: RenderStrategy<VecBuffer<Xrgb8888>>>(
    strategy: S,
) -> Renderer<VecBuffer<Xrgb8888>, S> {
    let mut text: Box<dyn TextLayoutEngine> =
        Box::new(blit_text_cosmic::Backend::without_system_fonts());
    let face = text
        .register_font(FontData::Static(include_bytes!(env!("BLIT_TEST_FONT"))), 0)
        .unwrap();
    Renderer::new(
        VecBuffer::new(WIDTH, HEIGHT),
        RendererConfig {
            fonts: vec![FontFace {
                id: FontId::default(),
                weight: 400,
                stretch: 100,
                style: Default::default(),
                face,
            }],
            text_cache_capacity: 0,
            layout_cache_capacity: 0,
            glyph_cache_capacity: 0,
            shadow_cache_capacity: 0,
        },
        text,
    )
    .strategy(strategy)
}
