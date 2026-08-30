use std::hint::black_box;

use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_cpu::{
    Direct, Font, FontFace, Pixel, PremultipliedRgbaColor, RenderStrategy, Renderer,
    RendererConfig, Scanline, VecBuffer, Xrgb8888,
    color::Color,
    command_list::{BoxShadow, ClipId, CommandList, Rectangle},
    image::{
        ImageData, ImageFit, ImageFormat, ImagePixels, ImageRequest, ImageSampling, ImageTiling,
    },
    style::{GradientStop, LinearGradient},
    text_types::{FontId, TextOptions, TextRequest, TextStyle, TextWrap},
};
use divan::counter::ItemsCount;

const WIDTH: usize = 480;
const HEIGHT: usize = 800;
const PIXELS: usize = WIDTH * HEIGHT;
const SCALE: Scale2 = Scale2::IDENTITY;

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[divan::bench(args = [255, 128])]
fn blend_premultiplied_rgba(bencher: divan::Bencher, opacity: u8) {
    let source = (0..WIDTH)
        .map(|index| {
            let alpha = (index * 37 % 255 + 1) as u8;
            PremultipliedRgbaColor {
                red: (192 * alpha as u16 / 255) as u8,
                green: (128 * alpha as u16 / 255) as u8,
                blue: (64 * alpha as u16 / 255) as u8,
                alpha,
            }
        })
        .collect::<Vec<_>>();
    let mut destination = vec![Xrgb8888::from_raw(0x183048); WIDTH];

    bencher.counter(ItemsCount::new(WIDTH)).bench_local(|| {
        Xrgb8888::blend_texture_slice_rgba(
            black_box(&mut destination),
            black_box(&source),
            opacity,
        );
    });
}

#[divan::bench]
fn render_rectangles(bencher: divan::Bencher) {
    let mut commands = CommandList::default();
    for index in 0..100 {
        let area = LogicalRect {
            x: (index % 10) as f32 * 48.0,
            y: (index / 10) as f32 * 80.0,
            width: 48.0,
            height: 80.0,
        };
        commands.push_rectangle(
            Rectangle::new(area).background(Color::from_rgba8(
                (index * 31) as u8,
                (index * 47) as u8,
                (index * 61) as u8,
                255,
            )),
            area.to_physical(SCALE),
            ClipId::default(),
        );
    }
    let damage = [PhysicalRect {
        x: 0,
        y: 0,
        width: WIDTH as i32,
        height: HEIGHT as i32,
    }];
    let mut renderer = renderer(WIDTH, HEIGHT, Scanline::default());
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(PIXELS))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

#[divan::bench]
fn tiny_rectangles_direct(bencher: divan::Bencher) {
    benchmark_tiny_rectangles(bencher, Direct::default())
}

#[divan::bench]
fn tiny_rectangles_scanline(bencher: divan::Bencher) {
    benchmark_tiny_rectangles(bencher, Scanline::default())
}

#[divan::bench]
fn sparse_tiles_direct(bencher: divan::Bencher) {
    benchmark_sparse_tiles(bencher, Direct::default())
}

#[divan::bench]
fn sparse_tiles_scanline(bencher: divan::Bencher) {
    benchmark_sparse_tiles(bencher, Scanline::default())
}

#[divan::bench]
fn overlapping_rectangles_direct(bencher: divan::Bencher) {
    benchmark_overlapping_rectangles(bencher, Direct::default())
}

#[divan::bench]
fn overlapping_rectangles_scanline(bencher: divan::Bencher) {
    benchmark_overlapping_rectangles(bencher, Scanline::default())
}

#[divan::bench]
fn small_images_direct(bencher: divan::Bencher) {
    benchmark_small_images(bencher, Direct::default(), ImageFormat::Rgb8)
}

#[divan::bench]
fn small_images_luma_direct(bencher: divan::Bencher) {
    benchmark_small_images(bencher, Direct::default(), ImageFormat::Luma8)
}

#[divan::bench]
fn small_images_rgba_direct(bencher: divan::Bencher) {
    benchmark_small_images(bencher, Direct::default(), ImageFormat::Rgba8)
}

#[divan::bench]
fn small_images_premultiplied_rgba_direct(bencher: divan::Bencher) {
    benchmark_small_images(bencher, Direct::default(), ImageFormat::Rgba8Premultiplied)
}

#[divan::bench]
fn small_images_alpha_direct(bencher: divan::Bencher) {
    benchmark_small_images(
        bencher,
        Direct::default(),
        ImageFormat::Alpha8(Color::from_rgba8(38, 96, 176, 255)),
    )
}

#[divan::bench]
fn small_images_scanline(bencher: divan::Bencher) {
    benchmark_small_images(bencher, Scanline::default(), ImageFormat::Rgb8)
}

#[divan::bench]
fn text_labels_direct(bencher: divan::Bencher) {
    benchmark_text_labels(bencher, Direct::default())
}

#[divan::bench]
fn text_labels_scanline(bencher: divan::Bencher) {
    benchmark_text_labels(bencher, Scanline::default())
}

#[divan::bench]
fn text_heavy_direct(bencher: divan::Bencher) {
    benchmark_text_heavy(bencher, Direct::default())
}

#[divan::bench]
fn text_heavy_scanline(bencher: divan::Bencher) {
    benchmark_text_heavy(bencher, Scanline::default())
}

#[divan::bench]
fn wrapped_paragraph_direct(bencher: divan::Bencher) {
    benchmark_wrapped_paragraph(bencher, Direct::default())
}

#[divan::bench]
fn wrapped_paragraph_scanline(bencher: divan::Bencher) {
    benchmark_wrapped_paragraph(bencher, Scanline::default())
}

#[divan::bench]
fn rounded_clip_direct(bencher: divan::Bencher) {
    benchmark_rounded_clip(bencher, Direct::default())
}

#[divan::bench]
fn rounded_clip_scanline(bencher: divan::Bencher) {
    benchmark_rounded_clip(bencher, Scanline::default())
}

#[divan::bench]
fn gradient_border(bencher: divan::Bencher) {
    let mut commands = CommandList::default();
    let area = LogicalRect {
        x: 48.0,
        y: 160.0,
        width: 384.0,
        height: 480.0,
    };
    let stops = [
        GradientStop::new(0.0, Color::from_rgba8(32, 96, 192, 255)),
        GradientStop::new(0.5, Color::from_rgba8(224, 64, 96, 255)),
        GradientStop::new(1.0, Color::from_rgba8(240, 192, 48, 255)),
    ];
    commands.push_rectangle(
        Rectangle::new(area)
            .background(Color::from_rgba8(16, 24, 40, 255))
            .gradient_border(2.0, LinearGradient::new(&stops).angle(35.0))
            .uniform_radius(24.0),
        area.to_physical(SCALE),
        ClipId::default(),
    );
    let damage = [area.to_physical(SCALE)];
    let mut renderer = renderer(WIDTH, HEIGHT, Scanline::default());
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(area.to_physical(SCALE).height as usize))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

#[divan::bench(args = [true, false])]
fn shadow(bencher: divan::Bencher, cached: bool) {
    let area = LogicalRect {
        x: 64.0,
        y: 176.0,
        width: 352.0,
        height: 448.0,
    };
    let shadow = BoxShadow::new(area, Color::from_rgba8(0, 0, 0, 128))
        .uniform_radius(24.0)
        .blur(16.0);
    let mut commands = CommandList::default();
    commands.push_box_shadow(
        shadow,
        shadow.bounds().to_physical(SCALE),
        ClipId::default(),
    );
    let damage = [shadow.bounds().to_physical(SCALE)];
    let mut renderer = renderer_with_shadow_cache(
        WIDTH,
        HEIGHT,
        Scanline::default(),
        if cached { 512 * 1024 } else { 0 },
    );
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(damage[0].height as usize))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn benchmark_tiny_rectangles<S>(bencher: divan::Bencher, strategy: S)
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    const SIDE: usize = 32;
    let mut commands = CommandList::default();
    for index in 0..SIDE * SIDE {
        let area = LogicalRect {
            x: (index % SIDE) as f32,
            y: (index / SIDE) as f32,
            width: 1.0,
            height: 1.0,
        };
        commands.push_rectangle(
            Rectangle::new(area).background(Color::from_rgba8(
                index as u8,
                (index * 31) as u8,
                (index * 67) as u8,
                255,
            )),
            area.to_physical(SCALE),
            ClipId::default(),
        );
    }
    let damage = [PhysicalRect {
        x: 0,
        y: 0,
        width: SIDE as i32,
        height: SIDE as i32,
    }];
    let mut renderer = renderer(SIDE, SIDE, strategy);
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(SIDE * SIDE))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn benchmark_sparse_tiles<S>(bencher: divan::Bencher, strategy: S)
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    const DAMAGED: [usize; 16] = [0, 2, 5, 7, 9, 12, 18, 23, 24, 31, 33, 38, 45, 50, 56, 63];
    let mut commands = CommandList::default();
    let mut damage = Vec::with_capacity(DAMAGED.len());
    for index in 0..64 {
        let area = LogicalRect {
            x: 36.0 + (index % 8) as f32 * 52.0,
            y: 144.0 + (index / 8) as f32 * 64.0,
            width: 44.0,
            height: 44.0,
        };
        commands.push_rectangle(
            Rectangle::new(area)
                .background(Color::from_rgba8(40, 72, 112, 255))
                .uniform_radius(8.0),
            area.to_physical(SCALE),
            ClipId::default(),
        );
        if DAMAGED.contains(&index) {
            damage.push(area.to_physical(SCALE));
        }
    }
    let mut renderer = renderer(WIDTH, HEIGHT, strategy);
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(DAMAGED.len()))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn benchmark_overlapping_rectangles<S>(bencher: divan::Bencher, strategy: S)
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    const COMMANDS: usize = 128;
    const SIZE: usize = 96;
    let area = LogicalRect {
        width: SIZE as f32,
        height: SIZE as f32,
        ..LogicalRect::default()
    };
    let mut commands = CommandList::default();
    for index in 0..COMMANDS {
        commands.push_rectangle(
            Rectangle::new(area).background(Color::from_rgba8(
                (index * 13) as u8,
                (index * 29) as u8,
                (index * 47) as u8,
                if index + 1 == COMMANDS { 255 } else { 64 },
            )),
            area.to_physical(SCALE),
            ClipId::default(),
        );
    }
    let damage = [area.to_physical(SCALE)];
    let mut renderer = renderer(SIZE, SIZE, strategy);
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(COMMANDS))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn benchmark_small_images<S>(bencher: divan::Bencher, strategy: S, format: ImageFormat)
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    const COMMANDS: usize = 240;
    const IMAGE_SIZE: usize = 8;
    let pixels = match format {
        ImageFormat::Rgb8 => [38, 96, 176].repeat(IMAGE_SIZE * IMAGE_SIZE),
        ImageFormat::Luma8 => [96].repeat(IMAGE_SIZE * IMAGE_SIZE),
        ImageFormat::Rgba8 => [38, 96, 176, 192].repeat(IMAGE_SIZE * IMAGE_SIZE),
        ImageFormat::Rgba8Premultiplied => [29, 72, 132, 192].repeat(IMAGE_SIZE * IMAGE_SIZE),
        ImageFormat::Alpha8(_) => [192].repeat(IMAGE_SIZE * IMAGE_SIZE),
    };
    let mut renderer = renderer(WIDTH, HEIGHT, strategy);
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Owned(pixels.into_boxed_slice()),
        format,
        IMAGE_SIZE,
        IMAGE_SIZE,
    ));
    let mut commands = CommandList::default();
    for index in 0..COMMANDS {
        let area = LogicalRect {
            x: 8.0 + (index % 12) as f32 * 38.0,
            y: 10.0 + (index / 12) as f32 * 38.0,
            width: 28.0,
            height: 28.0,
        };
        commands.push_image(
            ImageRequest {
                image: image.id(),
                area,
                fit: ImageFit::Fill,
                sampling: ImageSampling::Nearest,
                opacity: 1.0,
                colorize: None,
                nine_slice: None,
                horizontal_tiling: ImageTiling::None,
                vertical_tiling: ImageTiling::None,
            },
            area.to_physical(SCALE),
            ClipId::default(),
        );
    }
    let damage = [PhysicalRect {
        x: 0,
        y: 0,
        width: WIDTH as i32,
        height: HEIGHT as i32,
    }];
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(COMMANDS))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn benchmark_text_labels<S>(bencher: divan::Bencher, strategy: S)
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    const COMMANDS: usize = 48;
    let mut renderer = renderer(WIDTH, HEIGHT, strategy);
    let text = renderer.text_run("secure approval", TextStyle::default());
    let mut commands = CommandList::default();
    for index in 0..COMMANDS {
        let area = LogicalRect {
            x: 12.0 + (index % 3) as f32 * 156.0,
            y: 12.0 + (index / 3) as f32 * 42.0,
            width: 144.0,
            height: 28.0,
        };
        commands.push_text(
            TextRequest {
                text,
                area,
                offset_x: 0.0,
                color: Color::from_rgba8(224, 232, 240, 255),
                style: TextStyle::default(),
                options: TextOptions::default(),
            },
            area.to_physical(SCALE),
            ClipId::default(),
        );
    }
    let damage = [PhysicalRect {
        x: 0,
        y: 0,
        width: WIDTH as i32,
        height: HEIGHT as i32,
    }];
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(COMMANDS))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn benchmark_text_heavy<S>(bencher: divan::Bencher, strategy: S)
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    const LINES: usize = 40;
    let mut renderer = renderer(WIDTH, HEIGHT, strategy);
    let strings = (0..LINES)
        .map(|line| {
            renderer.text_run(
                &format!(
                    "transaction {line:02}: verify recipient 0x7c91…{line:04x}, amount 12.345 ETH, and network fee 0.0042 ETH"
                ),
                TextStyle::default(),
            )
        })
        .collect::<Vec<_>>();
    let mut commands = CommandList::default();
    for (line, text) in strings.into_iter().enumerate() {
        let area = LogicalRect {
            x: 8.0,
            y: line as f32 * 20.0,
            width: 464.0,
            height: 20.0,
        };
        commands.push_text(
            TextRequest {
                text,
                area,
                offset_x: 0.0,
                color: Color::from_rgba8(224, 232, 240, 255),
                style: TextStyle::default(),
                options: TextOptions::default(),
            },
            area.to_physical(SCALE),
            ClipId::default(),
        );
    }
    let damage = [PhysicalRect {
        x: 0,
        y: 0,
        width: WIDTH as i32,
        height: HEIGHT as i32,
    }];
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(LINES))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn benchmark_wrapped_paragraph<S>(bencher: divan::Bencher, strategy: S)
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    let area = LogicalRect {
        x: 40.0,
        y: 180.0,
        width: 400.0,
        height: 320.0,
    };
    let style = TextStyle {
        size: 20.0,
        ..TextStyle::default()
    };
    let mut renderer = renderer(WIDTH, HEIGHT, strategy);
    let text = renderer.text_run(
        "Passport keeps your keys offline while making secure approvals clear and deliberate. Every transaction is reviewed on the trusted display before it is signed. Recovery information stays under your control, and the device never needs to expose private keys.",
        style,
    );
    let mut commands = CommandList::default();
    commands.push_text(
        TextRequest {
            text,
            area,
            offset_x: 0.0,
            color: Color::from_rgba8(224, 232, 240, 255),
            style,
            options: TextOptions {
                wrap: TextWrap::Word,
                ..TextOptions::default()
            },
        },
        area.to_physical(SCALE),
        ClipId::default(),
    );
    let damage = [area.to_physical(SCALE)];
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(1usize))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn benchmark_rounded_clip<S>(bencher: divan::Bencher, strategy: S)
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    const COMMANDS: usize = 128;
    const SIZE: usize = 128;
    let area = LogicalRect {
        width: SIZE as f32,
        height: SIZE as f32,
        ..LogicalRect::default()
    };
    let mut commands = CommandList::default();
    let clip = commands.push_clip(
        ClipId::default(),
        area,
        blit_cpu::style::BorderRadius {
            top_left: 48.0,
            top_right: 48.0,
            bottom_right: 48.0,
            bottom_left: 48.0,
        },
    );
    for index in 0..COMMANDS {
        commands.push_rectangle(
            Rectangle::new(area).background(Color::from_rgba8(
                (index * 17) as u8,
                (index * 31) as u8,
                (index * 53) as u8,
                16,
            )),
            area.to_physical(SCALE),
            clip,
        );
    }
    let damage = [area.to_physical(SCALE)];
    let mut renderer = renderer(SIZE, SIZE, strategy);
    renderer.render(&commands, &damage);

    bencher
        .counter(ItemsCount::new(COMMANDS))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

fn renderer<S>(width: usize, height: usize, strategy: S) -> Renderer<VecBuffer<Xrgb8888>, S>
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    renderer_with_shadow_cache(width, height, strategy, 512 * 1024)
}

fn renderer_with_shadow_cache<S>(
    width: usize,
    height: usize,
    strategy: S,
    shadow_cache_capacity: usize,
) -> Renderer<VecBuffer<Xrgb8888>, S>
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
{
    Renderer::new(
        VecBuffer::new(width, height),
        RendererConfig {
            fonts: vec![FontFace {
                id: FontId::default(),
                weight: 400,
                font: Font::from_static(include_bytes!(env!("BLIT_TEST_FONT"))).unwrap(),
            }],
            font_metric_cache_capacity: 512,
            glyph_cache_capacity: 512 * 1024,
            paragraph_cache_capacity: 512 * 1024,
            shadow_cache_capacity,
        },
    )
    .strategy(strategy)
}
