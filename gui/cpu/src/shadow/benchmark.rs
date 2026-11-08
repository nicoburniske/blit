use std::{hint::black_box, time::Instant};

use blit::LogicalRect;
use blit_gui::{
    GuiContext, TextConfig, TextSystem, color::Color, display_list::Rectangle, shadow::BoxShadow, style::BorderRadius,
};

use crate::{Direct, RenderStrategy, Renderer, RendererConfig, Scanline, VecBuffer, Xrgb8888};

#[test]
#[ignore = "run in release mode with --ignored --nocapture"]
fn shadow_cost() {
    measure::<Direct>("direct");
    measure::<Scanline>("scanline");
}

fn measure<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default>(strategy: &str) {
    for count in [1, 64] {
        let mut renderer = Renderer::new(VecBuffer::new(1024, 768), RendererConfig::default()).strategy(S::default());
        renderer.register(super::Renderer::default());
        let mut gui = GuiContext::new(
            TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
        );
        for case in 0..3 {
            for _ in 0..8 {
                scene(&mut gui, count, 0);
                renderer.render(gui.render_input(), 0);
                gui.finish_frame();
            }
            let mut samples = [0.0; 9];
            for sample in &mut samples {
                let start = Instant::now();
                for frame in 0..100 {
                    scene(&mut gui, count, if case == 2 { frame % 2 } else { 0 });
                    renderer.render(black_box(gui.render_input()), if case == 0 { 0 } else { 1 });
                    black_box(renderer.buffer().pixels());
                    gui.finish_frame();
                }
                *sample = start.elapsed().as_secs_f64() * 1_000_000.0 / 100.0;
            }
            samples.sort_by(f64::total_cmp);
            let label = ["full", "unchanged", "one_changed"][case];
            println!("{strategy} {count} shadows {label}: {:.3} us", samples[4]);
        }
    }
}

fn scene(gui: &mut GuiContext, count: usize, frame: usize) {
    gui.paint_rectangle(Rectangle::new(LogicalRect::new(0.0, 0.0, 1024.0, 768.0)).background(Color::rgb(20, 30, 40)));
    for index in 0..count {
        let area = if count == 1 {
            LogicalRect::new(120.0, 100.0, 760.0, 540.0)
        } else {
            LogicalRect::new(
                (index % 8) as f32 * 124.0 + 30.0,
                (index / 8) as f32 * 92.0 + 24.0,
                76.0,
                48.0,
            )
        };
        let shadow = BoxShadow::new(
            area,
            if index == 0 && frame != 0 {
                Color::rgba(40, 0, 0, 120)
            } else {
                Color::rgba(0, 0, 0, 120)
            },
        )
        .blur(8.0)
        .offset(2.0, 4.0)
        .radius(BorderRadius::uniform(10.0));
        gui.paint_custom(shadow.bounds(), |_| shadow);
        gui.paint_rectangle(
            Rectangle::new(area)
                .background(Color::rgb(45, 65, 85))
                .radius(BorderRadius::uniform(10.0)),
        );
    }
}
