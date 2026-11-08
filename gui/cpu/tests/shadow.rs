use blit::{LogicalRect, Scale2};
use blit_cpu::{
    DamageTracker, Direct, RenderStrategy, Renderer, RendererConfig, Scanline, TileDamage, VecBuffer, Xrgb8888,
    damage::command::CommandDiff,
};
use blit_gui::{
    GuiContext, TextConfig, TextSystem,
    color::Color,
    display_list::{ClipId, Rectangle},
    shadow::BoxShadow,
    style::BorderRadius,
};

#[test]
fn shadows_match_reference_pixels_and_incremental_damage() {
    fn check<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default, D: DamageTracker>(damage: impl Fn() -> D) {
        for capacity in [0, 512 * 1024] {
            // reference pixels from the builtin shadow renderer
            for (scale, expected) in [
                (
                    1.0,
                    [
                        0x3cc8ce64bc2ef27f,
                        0x585c220b3b9621f3,
                        0x5c899d7e364b05f0,
                        0x667beaedce43f43d,
                    ],
                ),
                (
                    1.5,
                    [
                        0x8ec1a46a6a1b47b1,
                        0x4d3a7028585958e6,
                        0x8acccd060192db1a,
                        0x28570af67a8e7b79,
                    ],
                ),
                (
                    2.0,
                    [
                        0x85675ba3eee0aaa8,
                        0xdad3c0a0307b40d1,
                        0x0bb572a925bf6c31,
                        0x66aa5382073e021b,
                    ],
                ),
            ] {
                let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(128, 96), RendererConfig::default())
                    .strategy(S::default())
                    .damage_tracker(damage());
                renderer.register(blit_cpu::shadow::Renderer::new(capacity));
                let mut spare = VecBuffer::new(128, 96);
                let mut gui = GuiContext::new(
                    TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
                );
                gui.set_scale(scale);
                for (step, frame) in [0, 0, 1, 2, 3, 3, 0].into_iter().enumerate() {
                    let list = gui.render_input().display_list;
                    list.push_rectangle(
                        Rectangle::new(LogicalRect::new(0.0, 0.0, 128.0, 96.0)).background(Color::rgb(30, 45, 60)),
                        blit::PhysicalRect::new(0, 0, 128, 96),
                        ClipId::default(),
                    );
                    let outer = list.push_clip(
                        ClipId::default(),
                        LogicalRect::new(1.0, 2.0, 118.0, 88.0),
                        BorderRadius::uniform(7.0),
                    );
                    let clip = list.push_clip(
                        outer,
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
                        let shadow = BoxShadow::new(
                            area,
                            Color::rgba(150 + frame as u8 * 20, 40, 180, if index == 5 { 0 } else { 170 }),
                        )
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
                        list.push_custom(shadow.bounds().to_physical(Scale2::uniform(scale)), clip, |_| shadow);
                    }
                    renderer.render(gui.render_input(), if step < 2 { 0 } else { 2 });
                    let hash = renderer
                        .buffer()
                        .pixels()
                        .iter()
                        .fold(0xcbf29ce484222325u64, |hash, pixel| {
                            (hash ^ pixel.raw() as u64).wrapping_mul(0x100000001b3)
                        });
                    assert_eq!(hash, expected[frame], "capacity {capacity} scale {scale} frame {frame}");
                    gui.finish_frame();
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

#[test]
#[should_panic(expected = "no cpu extension registered")]
fn shadows_require_a_registered_handler() {
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(8, 8), RendererConfig::default());
    let mut gui = GuiContext::new(
        TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
    );
    let shadow = BoxShadow::new(LogicalRect::new(1.0, 1.0, 4.0, 4.0), Color::BLACK);
    gui.paint_custom(shadow.bounds(), |_| shadow);
    renderer.render(gui.render_input(), 0);
}
