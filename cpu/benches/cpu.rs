use std::hint::black_box;

use blit::{
    color::Color,
    command_list::{ClipId, CommandList},
    geometry::{LogicalRect, PhysicalRect},
    paint::{FontId, Rectangle},
};
use blit_cpu::{
    Font, FontFace, Pixel, PremultipliedRgbaColor, Renderer, RendererConfig, Scanline, VecBuffer,
    Xrgb8888,
};
use divan::counter::ItemsCount;

const WIDTH: usize = 480;
const HEIGHT: usize = 800;
const PIXELS: usize = WIDTH * HEIGHT;

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
            area.to_physical(1.0),
            ClipId::default(),
        );
    }
    let damage = [PhysicalRect {
        x: 0,
        y: 0,
        width: WIDTH as i32,
        height: HEIGHT as i32,
    }];
    let mut renderer = Renderer::new(
        VecBuffer::<Xrgb8888>::new(WIDTH, HEIGHT),
        RendererConfig {
            fonts: vec![FontFace {
                id: FontId::default(),
                weight: 400,
                font: Font::from_static(include_bytes!(env!("BLIT_TEST_FONT"))).unwrap(),
            }],
            font_metric_cache_capacity: 1,
            glyph_cache_capacity: 1,
            paragraph_cache_capacity: 1,
            shadow_cache_capacity: 0,
        },
    )
    .strategy(Scanline::default());

    bencher
        .counter(ItemsCount::new(PIXELS))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}
