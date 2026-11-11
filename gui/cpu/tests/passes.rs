use blit::{LogicalRect, Paint, PhysicalRect};
use blit_cpu::{
    Direct, Pixel, PixelBuffer, PremultipliedRgbaColor, Render, RenderContext, RenderStrategy, Renderer,
    RendererConfig, Scanline, VecBuffer, Xrgb8888,
};
use blit_gui::{
    BackdropBlur, Context, Rectangle, TextConfig, TextSystem,
    color::Color,
    damage::{DamageTracker, command::CommandDiff, tile::TileDamage},
    style::BorderRadius,
};

#[test]
fn regional_passes_match_full_rendering_across_buffer_ages_and_scene_changes() {
    fn check<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default, D: DamageTracker>(damage: D) {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(96, 64), RendererConfig::default())
            .strategy(S::default())
            .damage_tracker(damage);
        let mut reference = new_renderer(VecBuffer::<Xrgb8888>::new(96, 64), RendererConfig::default());
        let mut spare = VecBuffer::new(96, 64);
        for (index, frame) in [0, 0, 1, 2, 3, 3, 0].into_iter().enumerate() {
            scene(&mut renderer, frame);
            scene(&mut reference, frame);
            renderer.render(if index < 2 { 0 } else { 2 });
            reference.render(0);
            assert_eq!(renderer.buffer().pixels(), reference.buffer().pixels(), "frame {index}");
            std::mem::swap(renderer.buffer_mut(), &mut spare);
        }
    }
    check::<Direct, _>(TileDamage::new(8));
    check::<Scanline, _>(TileDamage::new(8));
    check::<Direct, _>(CommandDiff::default());
    check::<Scanline, _>(CommandDiff::default());
}

#[test]
fn pass_barriers_read_current_frame_and_respect_nested_clips() {
    #[derive(Clone, Copy, Default, Hash, PartialEq)]
    struct Shift;
    impl Render<Xrgb8888> for Shift {
        type Input = Shift;
        type Prepared = Shift;
        fn prepare(&mut self, command: &Shift, _: &mut RenderContext<'_, Xrgb8888>) -> Shift {
            *command
        }
        fn render(&mut self, _: &Shift, cx: &mut RenderContext<'_, Xrgb8888>) {
            let source = cx.source().unwrap();
            cx.for_each_span(|span| {
                for (offset, pixel) in span.pixels.iter_mut().enumerate() {
                    let x = span.x as usize + offset;
                    let sample = source.row(span.y.saturating_sub(1) as usize, x..x + 1)[0].raw();
                    let color = Color::rgb((sample >> 16) as u8, (sample >> 8) as u8, sample as u8);
                    pixel.blend(PremultipliedRgbaColor::new(color, span.coverage));
                }
            });
        }

        fn reads(&self, _: &Shift, output: PhysicalRect) -> PhysicalRect {
            PhysicalRect::new(output.x, output.y.saturating_sub(1).max(0), output.width, output.height)
        }
    }
    impl<B, S, D> Paint<Shift> for Renderer<B, S, D>
    where
        B: PixelBuffer<Pixel = Xrgb8888>,
        S: RenderStrategy<B>,
        D: DamageTracker,
    {
        fn paint(&mut self, bounds: LogicalRect, command: Shift) {
            self.record(bounds).push_pass::<Shift>(command);
        }
    }
    let mut renderer =
        new_renderer(VecBuffer::<Xrgb8888>::new(12, 10), RendererConfig::default()).strategy(Scanline::default());
    let mut spare = VecBuffer::<Xrgb8888>::new(12, 10);
    for (index, color) in [
        Color::rgb(255, 0, 0),
        Color::rgb(0, 255, 0),
        Color::rgb(0, 255, 0),
        Color::rgb(0, 255, 0),
        Color::rgb(255, 0, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let source_area = LogicalRect::new(0.0, 0.0, 12.0, 1.0);
        let source = Rectangle::new().background(color);
        renderer.paint(source_area, Shift);
        source.paint(source_area, &mut renderer);
        renderer.paint(LogicalRect::new(0.0, 1.0, 12.0, 1.0), Shift);
        renderer.push_clip(LogicalRect::new(1.0, 0.0, 9.0, 8.0), BorderRadius::uniform(3.0));
        renderer.push_clip(LogicalRect::new(2.0, 0.0, 6.0, 8.0), BorderRadius::default());
        renderer.paint(LogicalRect::new(0.0, 2.0, 12.0, 1.0), Shift);
        renderer.pop_clip();
        renderer.pop_clip();
        renderer.render(if index < 2 { 0 } else { 2 });
        let expected = (u32::from(color.red) << 16) | (u32::from(color.green) << 8);
        assert_eq!(renderer.buffer().pixels()[2 * 12 + 5].raw(), expected);
        assert_eq!(renderer.buffer().pixels()[2 * 12].raw(), 0);
        assert_eq!(renderer.buffer().pixels()[2 * 12 + 9].raw(), 0);

        std::mem::swap(renderer.buffer_mut(), &mut spare);
    }
}

fn scene<S, D>(renderer: &mut Renderer<VecBuffer<Xrgb8888>, S, D>, frame: usize)
where
    S: RenderStrategy<VecBuffer<Xrgb8888>>,
    D: DamageTracker,
{
    Rectangle::new()
        .background(Color::BLACK)
        .paint(LogicalRect::new(0.0, 0.0, 96.0, 64.0), renderer);
    for x in (0..96).step_by(8) {
        Rectangle::new()
            .background(Color::rgb(x as u8 * 2, 80, 150))
            .paint(LogicalRect::new(x as f32, 4.0, 4.0, 56.0), renderer);
    }
    Rectangle::new()
        .background(Color::WHITE)
        .paint(LogicalRect::new(12.0 + frame as f32 * 4.0, 18.0, 4.0, 8.0), renderer);
    renderer.push_clip(LogicalRect::new(3.0, 2.0, 90.0, 60.0), BorderRadius::uniform(8.0));
    if frame != 3 {
        renderer.paint(LogicalRect::new(8.0, 8.0, 48.0, 32.0), BackdropBlur::new(3.0));
        renderer.paint(LogicalRect::new(32.0, 20.0, 40.0, 28.0), BackdropBlur::new(2.0));
    }
    renderer.pop_clip();
    Rectangle::new()
        .background(Color::WHITE)
        .paint(LogicalRect::new(20.0, 18.0, 4.0, 8.0), renderer);
}

fn new_renderer<B: PixelBuffer>(buffer: B, config: RendererConfig) -> Renderer<B> {
    Renderer::new(
        buffer,
        TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
        config,
    )
}
