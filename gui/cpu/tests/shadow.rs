use blit::{LogicalRect, Paint};
use blit_cpu::{Direct, RenderStrategy, Renderer, RendererConfig, Scanline, VecBuffer, Xrgb8888, shadow};
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
                        0xcdad6a98810983ab,
                        0xf81307921dac8f87,
                        0xa76adcaea35edd0c,
                        0x667beaedce43f43d,
                    ],
                ),
                (
                    1.5,
                    [
                        0x1dbe81214cb462ff,
                        0xa816dfda8b38f31c,
                        0x9efb2de3e9cb3a8c,
                        0x28570af67a8e7b79,
                    ],
                ),
                (
                    2.0,
                    [
                        0x8240d8cfd3f20d19,
                        0x584d94dba5834684,
                        0x7d189c040bc033cc,
                        0x66aa5382073e021b,
                    ],
                ),
            ] {
                let mut renderer = Renderer::new(
                    VecBuffer::<Xrgb8888>::new(128, 96),
                    TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
                    RendererConfig::default(),
                )
                .handler(shadow::Handler::new(capacity))
                .strategy(S::default())
                .damage_tracker(damage());
                let mut spare = VecBuffer::new(128, 96);
                renderer.set_scale(scale);
                for (step, frame) in [0, 0, 1, 2, 3, 3, 0].into_iter().enumerate() {
                    renderer.paint(
                        LogicalRect::new(0.0, 0.0, 128.0 / scale, 96.0 / scale),
                        Rectangle::new().background(Color::rgb(30, 45, 60)),
                    );
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
