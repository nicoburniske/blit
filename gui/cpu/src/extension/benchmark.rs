//! compares custom and builtin shadows using the same cached raster kernels

use std::{
    hash::{Hash, Hasher},
    hint::black_box,
    time::Instant,
};

use blit::{LogicalRect, PhysicalRect};
use blit_gui::{
    GuiContext, TextConfig, TextSystem,
    color::Color,
    display_list::{BoxShadow, Rectangle},
    style::BorderRadius,
};

use super::{DrawContext, Extension, PrepareContext};
use crate::{
    Direct, Pixel, RenderStrategy, Renderer, RendererConfig, Scanline, VecBuffer, Xrgb8888,
    render::{image, image_patch, shadow},
};

#[test]
#[ignore = "run in release mode with --ignored --nocapture"]
fn shadow_extension_cost() {
    compare::<Direct>("direct");
    compare::<Scanline>("scanline");
}

fn compare<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default>(strategy: &str) {
    for count in [1, 64] {
        let mut native = Renderer::new(VecBuffer::new(1024, 768), RendererConfig::default()).strategy(S::default());
        let mut custom = Renderer::new(VecBuffer::new(1024, 768), RendererConfig::default()).strategy(S::default());
        custom.register(ShadowRenderer {
            cache: shadow::Cache::new(512 * 1024),
        });
        let mut native_gui = GuiContext::new(
            TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
        );
        let mut custom_gui = GuiContext::new(
            TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
        );
        for case in 0..3 {
            for _ in 0..8 {
                for (extension, renderer, gui) in [
                    (false, &mut native, &mut native_gui),
                    (true, &mut custom, &mut custom_gui),
                ] {
                    scene(gui, count, extension, 0);
                    renderer.render(gui.render_input(), 0);
                    gui.finish_frame();
                }
            }
            assert_eq!(native.buffer().pixels(), custom.buffer().pixels());
            let mut samples = [[0.0; 2]; 9];
            for (sample, times) in samples.iter_mut().enumerate() {
                for route in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let (renderer, gui) = if route == 0 {
                        (&mut native, &mut native_gui)
                    } else {
                        (&mut custom, &mut custom_gui)
                    };
                    let start = Instant::now();
                    for frame in 0..100 {
                        scene(gui, count, route == 1, if case == 2 { frame % 2 } else { 0 });
                        renderer.render(black_box(gui.render_input()), if case == 0 { 0 } else { 1 });
                        black_box(renderer.buffer().pixels());
                        gui.finish_frame();
                    }
                    times[route] = start.elapsed().as_secs_f64() * 1_000_000.0 / 100.0;
                }
                assert_eq!(native.buffer().pixels(), custom.buffer().pixels());
            }
            let mut native = samples.map(|times| times[0]);
            let mut custom = samples.map(|times| times[1]);
            native.sort_by(f64::total_cmp);
            custom.sort_by(f64::total_cmp);
            let label = ["full", "unchanged", "one_changed"][case];
            println!(
                "{strategy} {count} shadows {label}: native {:.3} us custom {:.3} us ratio {:.3}",
                native[4],
                custom[4],
                custom[4] / native[4]
            );
        }
    }
}

fn scene(gui: &mut GuiContext, count: usize, custom: bool, frame: usize) {
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
        if custom {
            gui.paint_custom(shadow.bounds(), |_| CustomShadow(shadow));
        } else {
            gui.paint_shadow(shadow);
        }
        gui.paint_rectangle(
            Rectangle::new(area)
                .background(Color::rgb(45, 65, 85))
                .radius(BorderRadius::uniform(10.0)),
        );
    }
}

#[derive(PartialEq)]
struct CustomShadow(BoxShadow);

impl Hash for CustomShadow {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let shadow = &self.0;
        [
            shadow.area.x,
            shadow.area.y,
            shadow.area.width,
            shadow.area.height,
            shadow.radius.top_left,
            shadow.radius.top_right,
            shadow.radius.bottom_right,
            shadow.radius.bottom_left,
            shadow.offset_x,
            shadow.offset_y,
            shadow.blur,
            shadow.spread,
        ]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
        shadow.color.hash(state);
        shadow.inset.hash(state);
    }
}

struct ShadowRenderer {
    cache: shadow::Cache,
}

impl<P: Pixel> Extension<P> for ShadowRenderer {
    type Command = CustomShadow;
    type Prepared = [Option<(image_patch::Prepared, PhysicalRect)>; 9];

    fn prepare(&mut self, command: &CustomShadow, cx: &mut PrepareContext<'_>) -> Self::Prepared {
        let mut patches = [None; 9];
        let Some(shadow::Prepared::Image(request)) = self.cache.prepare(cx.images, &command.0, cx.scale) else {
            panic!("benchmark requires blurred shadows");
        };
        let texture = cx.images.get(request.image.into()).unwrap();
        let mut index = 0;
        image::prepare(&request, &texture.data, cx.bounds, 1.0, |image, bounds| {
            patches[index] = Some((image, bounds));
            index += 1;
        });
        patches
    }

    fn render(&mut self, patches: &Self::Prepared, cx: &mut DrawContext<'_, P>) {
        cx.for_each_span(|span| {
            let row = PhysicalRect::new(span.x, span.y, span.pixels.len() as i32, 1);
            for (image, bounds) in patches.iter().flatten() {
                let Some(clip) = bounds.intersection(row) else { continue };
                let texture = span.images.get(image.image.into()).unwrap();
                let mut image = *image;
                image.opacity = (image.opacity as u16 * span.coverage as u16 / 255) as u8;
                image.draw_line(span.pixels, &texture.data, &texture.alpha_rows, clip, span.x, span.y);
            }
        });
    }

    fn finish_frame(&mut self) {
        self.cache.finish_frame();
    }
}
