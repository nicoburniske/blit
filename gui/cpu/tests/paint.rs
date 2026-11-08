#![cfg(feature = "paint")]

use std::time::Duration;

use blit::{Atom, AtomCx, Clip, Constraints, Frame, FrameInfo, Input, LogicalRect, LogicalSize, Paint, Size};
use blit_cpu::{
    DamageTracker, Direct, Pixel, PremultipliedRgbaColor, RenderStrategy, Renderer, RendererConfig, Scanline,
    TileDamage, VecBuffer, Xrgb8888,
    damage::command::CommandDiff,
    extension::{DrawContext, Extension, PrepareContext},
    paint::Painter,
};
use blit_gui::{
    BoundsClip, GuiContext, PaintCustom, TextConfig, TextSystem, atom,
    color::Color,
    display_list::{
        CustomCommand, Rectangle,
        arena::{Arena, Slice},
    },
    shadow::BoxShadow,
    style::{BorderRadius, GradientStop, LinearGradient},
};

blit_cpu::paint_handlers! {
    #[derive(Default)]
    struct Effects {
        shadow: BoxShadow => blit_cpu::shadow::Renderer,
        stripes: Stripes => StripeRenderer,
    }
}

#[test]
fn typed_atoms_match_full_redraws_with_reused_buffers() {
    fn check<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default, D: DamageTracker>(damage: impl Fn() -> D) {
        for scale in [1.0, 1.5, 2.0] {
            let renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(96, 64), RendererConfig::default())
                .strategy(S::default())
                .damage_tracker(damage());
            let mut painter = Painter::new(
                renderer,
                TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
                Effects::default(),
            );
            painter.set_scale(scale);
            let mut spare = VecBuffer::new(96, 64);
            let mut gui = GuiContext::new(
                TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
            );
            gui.set_scale(scale);
            let mut reference = Renderer::new(VecBuffer::<Xrgb8888>::new(96, 64), RendererConfig::default());
            reference.register(blit_cpu::shadow::Renderer::default());
            let mut frame = Frame::default();
            let mut reference_frame = Frame::default();
            let info = FrameInfo::new(LogicalSize::new(96.0 / scale, 64.0 / scale));

            for (step, content) in [0, 0, 1, 2, 2, 0].into_iter().enumerate() {
                frame.build(
                    &mut painter,
                    info,
                    Duration::ZERO,
                    Input::None,
                    |ui: blit::Ui<'_, _>| scene(ui, content),
                );
                frame.layout(&mut painter);
                frame.paint(&mut painter);
                painter.render(if step < 2 { 0 } else { 2 });

                reference_frame.build(&mut gui, info, Duration::ZERO, Input::None, |ui: blit::Ui<'_, _>| {
                    scene(ui, content)
                });
                reference_frame.layout(&mut gui);
                reference_frame.paint(&mut gui);
                reference.render(gui.render_input(), 0);
                gui.finish_frame();

                assert_eq!(
                    painter.buffer().pixels(),
                    reference.buffer().pixels(),
                    "scale {scale} step {step}"
                );
                std::mem::swap(painter.buffer_mut(), &mut spare);
            }
        }
    }
    check::<Direct, _>(|| TileDamage::new(8));
    check::<Scanline, _>(|| TileDamage::new(8));
    check::<Direct, _>(CommandDiff::default);
    check::<Scanline, _>(CommandDiff::default);
}

fn scene<C>(ui: blit::Ui<'_, C>, content: u8)
where
    C: blit::Context<Scalar = f32> + for<'a> Paint<Rectangle<'a>> + Paint<BoxShadow>,
    BoundsClip: Clip<C>,
{
    let mut panel = ui.layout(()).clip(BoundsClip);
    panel.insert(atom::Rectangle::new().background(Color::rgb(40, 70, 100)));
    if content != 2 {
        panel.insert(
            atom::Shadow::new(Color::rgba(0, 0, 0, 180))
                .radius(BorderRadius::uniform(7.0))
                .offset(content as f32 * 3.0, -1.0)
                .blur(5.0)
                .spread(2.0)
                .inset(true),
        );
    }
}

#[test]
fn borrowed_and_arena_commands_share_the_typed_painter() {
    let mut painter = Painter::new(
        Renderer::new(VecBuffer::<Xrgb8888>::new(32, 24), RendererConfig::default()),
        TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
        Effects::default(),
    );
    let mut ui_frame = Frame::default();
    for frame in [0, 0, 1, 1, 0] {
        let stops = [
            GradientStop::new(0.0, Color::BLACK),
            GradientStop::new(1.0, Color::WHITE),
        ];
        let area = LogicalRect::new(0.0, 0.0, 32.0, 24.0);
        painter.paint(area, Rectangle::new(area).background(LinearGradient::new(&stops)));
        let shadow = BoxShadow::new(LogicalRect::new(4.0, 4.0, 24.0, 16.0), Color::BLACK).blur(2.0);
        painter.paint(shadow.bounds(), shadow);
        painter.push_clip(LogicalRect::new(8.0, 8.0, 16.0, 8.0), BorderRadius::uniform(2.0));
        let colors = [Color::rgb(200, frame * 100, 40), Color::rgb(50, 100, 200)];
        ui_frame.build(
            &mut painter,
            FrameInfo::new(LogicalSize::new(32.0, 24.0)),
            Duration::ZERO,
            Input::None,
            |mut ui: blit::Ui<'_, _>| ui.insert(StripeAtom(colors)),
        );
        ui_frame.layout(&mut painter);
        ui_frame.paint(&mut painter);
        painter.pop_clip();
        painter.render(1);
        assert_eq!(
            painter.buffer().pixels()[12 * 32 + 12].raw(),
            0xc80028 | ((u32::from(frame) * 100) << 8)
        );
        assert_eq!(painter.buffer().pixels()[12 * 32 + 13].raw(), 0x3264c8);
        assert_ne!(
            painter.buffer().pixels()[12 * 32 + 6].raw(),
            0xc80028 | ((u32::from(frame) * 100) << 8)
        );
    }
}

struct StripeAtom([Color; 2]);

impl<C: PaintCustom<Stripes>> Atom<C> for StripeAtom {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        cx.context_mut().paint_with(area, |writer| Stripes {
            colors: writer.extend(self.0.iter().copied()),
        });
    }
}

struct Stripes {
    colors: Slice<Color>,
}

impl CustomCommand for Stripes {
    type View<'a> = &'a [Color];

    fn view<'a>(&'a self, arena: &'a Arena) -> Self::View<'a> {
        arena.slice(self.colors)
    }
}

#[derive(Default)]
struct StripeRenderer;

impl<P: Pixel> Extension<P> for StripeRenderer {
    type Command = Stripes;
    type Prepared = Slice<Color>;

    fn prepare(&mut self, colors: &[Color], cx: &mut PrepareContext<'_>) -> Self::Prepared {
        cx.data.extend(colors.iter().copied())
    }

    fn render(&mut self, colors: &Self::Prepared, cx: &mut DrawContext<'_, P>) {
        cx.for_each_span(|span| {
            let colors = span.data.slice(*colors);
            for (offset, pixel) in span.pixels.iter_mut().enumerate() {
                let color = colors[(span.x as usize + offset) % colors.len()];
                pixel.blend(PremultipliedRgbaColor::new(color, span.coverage));
            }
        });
    }
}
