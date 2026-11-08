use std::{hint::black_box, time::Instant};

use blit::{LogicalRect, Paint};
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
            let median = time(case, |frame, age| {
                scene(&mut gui, count, frame);
                renderer.render(black_box(gui.render_input()), age);
                black_box(renderer.buffer().pixels());
                gui.finish_frame();
            });
            let label = ["full", "unchanged", "one_changed"][case];
            println!("{strategy} {count} shadows {label}: {median:.3} us");
        }
    }
}

#[cfg(feature = "paint")]
#[test]
#[ignore = "run in release mode with --ignored --nocapture"]
fn paint_cost() {
    measure::<Direct>("direct legacy");
    measure_typed::<Direct>("direct typed");
    measure::<Scanline>("scanline legacy");
    measure_typed::<Scanline>("scanline typed");
}

#[cfg(feature = "paint")]
fn measure_typed<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default>(strategy: &str) {
    crate::paint_handlers! {
        struct Effects {
            shadow: BoxShadow => super::Renderer,
        }
    }
    for count in [1, 64] {
        let renderer = Renderer::new(VecBuffer::new(1024, 768), RendererConfig::default()).strategy(S::default());
        let mut painter = crate::paint::Painter::new(
            renderer,
            TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
            Effects {
                shadow: super::Renderer::default(),
            },
        );
        for case in 0..3 {
            let median = time(case, |frame, age| {
                scene(&mut painter, count, frame);
                painter.render(age);
                black_box(painter.buffer().pixels());
            });
            let label = ["full", "unchanged", "one_changed"][case];
            println!("{strategy} {count} shadows {label}: {median:.3} us");
        }
    }
}

fn time(case: usize, mut render: impl FnMut(usize, u8)) -> f64 {
    for _ in 0..8 {
        render(0, 0);
    }
    let mut samples = [0.0; 9];
    for sample in &mut samples {
        let start = Instant::now();
        for frame in 0..100 {
            render(if case == 2 { frame % 2 } else { 0 }, if case == 0 { 0 } else { 1 });
        }
        *sample = start.elapsed().as_secs_f64() * 1_000_000.0 / 100.0;
    }
    samples.sort_by(f64::total_cmp);
    samples[4]
}

fn scene<C>(gui: &mut C, count: usize, frame: usize)
where
    C: blit::Context<Scalar = f32> + for<'a> Paint<Rectangle<'a>> + Paint<BoxShadow>,
{
    let background = LogicalRect::new(0.0, 0.0, 1024.0, 768.0);
    gui.paint(
        background,
        Rectangle::new(background).background(Color::rgb(20, 30, 40)),
    );
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
        gui.paint(shadow.bounds(), shadow);
        gui.paint(
            area,
            Rectangle::new(area)
                .background(Color::rgb(45, 65, 85))
                .radius(BorderRadius::uniform(10.0)),
        );
    }
}
