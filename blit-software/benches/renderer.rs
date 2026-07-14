use std::hint::black_box;

use blit::{
    Color, DirtyRegions, FontId, ImageData, ImageFormat, ImagePixels, LogicalRect, PhysicalRect,
    widgets::{
        BorderRadius, BoxShadow, BoxShadowRequest, GradientStop, ImageFit, ImageRequest,
        ImageSampling, ImageTiling, LinearGradient, Rectangle,
    },
};
use blit_software::{Font, FontFace, FontSettings, Renderer, RendererConfig, Scanline, VecBuffer};
use criterion::{Criterion, criterion_group, criterion_main};

fn dirty_regions(criterion: &mut Criterion) {
    criterion.bench_function("dirty_regions/add_64", |bencher| {
        bencher.iter(|| {
            let mut dirty = DirtyRegions::default();
            for index in 0..64 {
                dirty.add(PhysicalRect {
                    x: (index * 37 % 1024) as i32,
                    y: (index * 67 % 768) as i32,
                    width: 48,
                    height: 32,
                });
            }
            black_box(dirty)
        });
    });
}

fn scanline(criterion: &mut Criterion) {
    let config = RendererConfig {
        fonts: vec![FontFace {
            id: FontId::default(),
            weight: 400,
            font: Font::from_bytes(
                include_bytes!("../../example/assets/Montserrat-Regular.ttf") as &[u8],
                FontSettings::default(),
            )
            .unwrap(),
        }],
        glyph_cache_capacity: 1,
        paragraph_cache_capacity: 1,
        shadow_cache_capacity: 1024 * 1024,
    };
    let mut renderer =
        Renderer::new(VecBuffer::<u32>::new(1024, 768), config).strategy(Scanline::default());
    let left = Rectangle::new(LogicalRect {
        x: 16.0,
        y: 0.0,
        width: 64.0,
        height: 768.0,
    })
    .background(Color::from_rgba8(20, 40, 60, 128));
    let right = Rectangle::new(LogicalRect {
        x: 944.0,
        y: 0.0,
        width: 64.0,
        height: 768.0,
    })
    .background(Color::from_rgba8(60, 40, 20, 128));
    let left_clip = [PhysicalRect {
        x: 16,
        y: 352,
        width: 64,
        height: 64,
    }];
    let right_clip = [PhysicalRect {
        x: 944,
        y: 352,
        width: 64,
        height: 64,
    }];
    let damage = [left_clip[0], right_clip[0]];
    let clip = PhysicalRect {
        x: 0,
        y: 0,
        width: 1024,
        height: 768,
    };

    criterion.bench_function("scanline/256_commands_two_horizontal_regions", |bencher| {
        bencher.iter(|| {
            renderer.begin_frame(&damage);
            for _ in 0..128 {
                renderer.draw_rectangle(black_box(&left), black_box(clip));
                renderer.draw_rectangle(black_box(&right), black_box(clip));
            }
            renderer.end_frame();
            black_box(renderer.buffer().pixels()[352 * 1024 + 16]);
        });
    });

    criterion.bench_function(
        "scanline/256_commands_two_horizontal_regions_rounded_clip",
        |bencher| {
            bencher.iter(|| {
                renderer.begin_frame(&damage);
                renderer.push_rounded_clip(
                    LogicalRect {
                        width: 1024.0,
                        height: 768.0,
                        ..LogicalRect::default()
                    },
                    BorderRadius {
                        top_left: 384.0,
                        top_right: 384.0,
                        bottom_right: 384.0,
                        bottom_left: 384.0,
                    },
                );
                for _ in 0..128 {
                    renderer.draw_rectangle(black_box(&left), black_box(clip));
                    renderer.draw_rectangle(black_box(&right), black_box(clip));
                }
                renderer.pop_rounded_clip();
                renderer.end_frame();
                black_box(renderer.buffer().pixels()[352 * 1024 + 16]);
            });
        },
    );

    let screen = Rectangle::new(LogicalRect {
        x: 0.0,
        y: 0.0,
        width: 1024.0,
        height: 768.0,
    })
    .background(Color::from_rgba8(20, 40, 60, 128));
    let clips = [
        PhysicalRect {
            x: 480,
            y: 8,
            width: 64,
            height: 8,
        },
        PhysicalRect {
            x: 480,
            y: 752,
            width: 64,
            height: 8,
        },
    ];

    criterion.bench_function("scanline/two_sparse_vertical_regions", |bencher| {
        bencher.iter(|| {
            renderer.begin_frame(&clips);
            renderer.draw_rectangle(black_box(&screen), black_box(clip));
            renderer.end_frame();
            black_box(renderer.buffer().pixels()[8 * 1024 + 480]);
        });
    });

    let shadow = BoxShadowRequest {
        area: LogicalRect {
            x: 256.0,
            y: 192.0,
            width: 512.0,
            height: 384.0,
        },
        radius: BorderRadius {
            top_left: 24.0,
            top_right: 24.0,
            bottom_right: 24.0,
            bottom_left: 24.0,
        },
        shadow: BoxShadow::new(Color::from_rgba8(0, 0, 0, 128))
            .offset(0.0, 8.0)
            .blur(16.0),
    };
    let shadow_clip = [shadow.bounds().to_physical(1.0)];
    renderer.begin_frame(&shadow_clip);
    renderer.draw_box_shadow(&shadow, shadow_clip[0]);
    renderer.end_frame();

    criterion.bench_function("shadow/cached_512x384_blur_16", |bencher| {
        bencher.iter(|| {
            renderer.begin_frame(&shadow_clip);
            renderer.draw_box_shadow(black_box(&shadow), black_box(shadow_clip[0]));
            renderer.end_frame();
            black_box(renderer.buffer().pixels()[192 * 1024 + 512]);
        });
    });

    let gradient_stops = [
        GradientStop::new(0.0, Color::from_rgba8(80, 120, 255, 255)),
        GradientStop::new(0.45, Color::from_rgba8(180, 70, 240, 220)),
        GradientStop::new(1.0, Color::from_rgba8(255, 80, 120, 255)),
    ];
    let gradient = Rectangle::new(LogicalRect {
        x: 256.0,
        y: 192.0,
        width: 512.0,
        height: 384.0,
    })
    .background(Color::from_rgba8(24, 28, 38, 255))
    .gradient_border(2.0, LinearGradient::new(&gradient_stops).angle(135.0))
    .uniform_radius(24.0);
    let gradient_clip = [gradient.area.to_physical(1.0)];

    criterion.bench_function("gradient_border/512x384_width_2_three_stops", |bencher| {
        bencher.iter(|| {
            renderer.begin_frame(&gradient_clip);
            renderer.draw_rectangle(black_box(&gradient), black_box(gradient_clip[0]));
            renderer.end_frame();
            black_box(renderer.buffer().pixels()[192 * 1024 + 512]);
        });
    });

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Owned([64, 32, 16, 128].repeat(256 * 256).into_boxed_slice()),
        ImageFormat::Rgba8Premultiplied,
        256,
        256,
    ));
    let image = ImageRequest {
        image,
        area: LogicalRect {
            x: 384.0,
            y: 256.0,
            width: 256.0,
            height: 256.0,
        },
        fit: ImageFit::Fill,
        sampling: ImageSampling::Nearest,
        opacity: 0.5,
        colorize: None,
        nine_slice: None,
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
    };
    let image_clip = [image.area.to_physical(1.0)];

    criterion.bench_function("image/premultiplied_rgba_256x256_opacity_50", |bencher| {
        bencher.iter(|| {
            renderer.begin_frame(&image_clip);
            renderer.draw_image(black_box(&image), black_box(image_clip[0]));
            renderer.end_frame();
            black_box(renderer.buffer().pixels()[256 * 1024 + 384]);
        });
    });
}

criterion_group!(benches, dirty_regions, scanline);
criterion_main!(benches);
