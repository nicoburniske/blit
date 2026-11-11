use blit::{LogicalRect, Paint};
use blit_cpu::{Direct, RenderStrategy, Renderer, RendererConfig, Scanline, VecBuffer, Xrgb8888, render::shadow};
use blit_gui::{
    Context, Rectangle, Shadow, TextConfig, TextSystem,
    color::Color,
    damage::{DamageTracker, command::CommandDiff, tile::TileDamage},
    style::BorderRadius,
};

#[test]
fn shadows_match_reference_pixels_and_incremental_damage() {
    fn check<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default, D: DamageTracker>(damage: impl Fn() -> D) {
        for capacity in [0, 512 * 1024] {
            // reference pixels from the shadow mask path
            for (scale, expected) in [
                (
                    1.0,
                    [
                        0x3a34150e97d2c469,
                        0xb6a4f69cf9ae57b9,
                        0x53f3bf33d82ad519,
                        0xbdfbd0e01d4ed00b,
                    ],
                ),
                (
                    1.5,
                    [
                        0xe3d686db3ca331a9,
                        0xc43a0c7e5df4a749,
                        0x89004504fb98162b,
                        0x385c9cf4edaae4e5,
                    ],
                ),
                (
                    2.0,
                    [
                        0x982b19b54c52318b,
                        0x32093364bcefb167,
                        0xee8c71de1b005c2e,
                        0x9e30e3975b1f2253,
                    ],
                ),
            ] {
                let mut renderer = Renderer::new(
                    VecBuffer::<Xrgb8888>::new(128, 96),
                    TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
                    RendererConfig::default(),
                )
                .register(shadow::Renderer::new(capacity))
                .strategy(S::default())
                .damage_tracker(damage());
                let mut spare = VecBuffer::new(128, 96);
                renderer.set_scale(scale);
                for (step, frame) in [0, 0, 1, 2, 3, 3, 0].into_iter().enumerate() {
                    Rectangle::new()
                        .background(Color::rgb(30, 45, 60))
                        .paint(LogicalRect::new(0.0, 0.0, 128.0 / scale, 96.0 / scale), &mut renderer);
                    renderer.push_clip(LogicalRect::new(1.0, 2.0, 118.0, 88.0), BorderRadius::uniform(7.0));
                    renderer.push_clip(
                        LogicalRect::new(3.0, 1.0, if frame == 2 { 70.0 } else { 110.0 }, 90.0),
                        BorderRadius::uniform(11.0),
                    );
                    for index in 0..7 {
                        if frame == 3 && index % 2 == 0 {
                            continue;
                        }
                        let area = LogicalRect::new(
                            5.25 + (index % 3) as f32 * 25.0 + frame as f32,
                            5.5 + (index / 3) as f32 * 25.0,
                            20.0,
                            17.0,
                        );
                        let shadow = Shadow::new(Color::rgba(
                            150 + frame as u8 * 20,
                            40,
                            180,
                            if index == 5 { 0 } else { 170 },
                        ))
                        .radius(BorderRadius {
                            top_left: 3.0,
                            top_right: 8.0,
                            bottom_right: 2.0,
                            bottom_left: 5.0,
                        })
                        .offset(2.5, -1.5)
                        .blur(if index % 3 == 0 { 0.0 } else { 3.25 })
                        .spread(if index == 6 { -20.0 } else { index as f32 - 2.0 })
                        .inset(index % 2 != 0);
                        renderer.paint(area, shadow);
                    }
                    renderer.pop_clip();
                    renderer.pop_clip();
                    renderer.render(if step < 2 { 0 } else { 2 });
                    let hash = renderer
                        .buffer()
                        .pixels()
                        .iter()
                        .fold(0xcbf29ce484222325u64, |hash, pixel| {
                            (hash ^ pixel.raw() as u64).wrapping_mul(0x100000001b3)
                        });
                    assert_eq!(hash, expected[frame], "capacity {capacity} scale {scale} frame {frame}");
                    std::mem::swap(renderer.buffer_mut(), &mut spare);
                }
            }
        }
    }
    check::<Direct, _>(|| TileDamage::new(8));
    check::<Scanline, _>(|| TileDamage::new(8));
    check::<Direct, _>(CommandDiff::default);
    check::<Scanline, _>(CommandDiff::default);
}
