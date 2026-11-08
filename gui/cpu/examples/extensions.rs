use std::{cell::Cell, collections::HashMap, io::Write, rc::Rc};

use blit::{LogicalRect, Paint, PhysicalRect};
use blit_cpu::{
    Pixel, PixelBuffer, PremultipliedRgbaColor, Renderer, RendererConfig, Scanline, VecBuffer, Xrgb8888,
    extension::{DrawContext, Extension, PrepareContext},
};
use blit_gui::{
    PaintCustom, TextConfig, TextSystem,
    color::Color,
    display_list::{
        CustomCommand, Rectangle,
        arena::{Arena, Slice},
    },
    image::{ImageData, ImageFormat, ImageHandle, ImagePixels},
    style::BorderRadius,
};

const WIDTH: usize = 160;
const HEIGHT: usize = 100;

fn main() -> std::io::Result<()> {
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(WIDTH, HEIGHT), RendererConfig::default())
        .strategy(Scanline::default());
    renderer.register(PathRenderer);
    renderer.register(ShadowRenderer::new(Rc::default()));
    renderer.register(BlurRenderer {
        scratch: Vec::new(),
        calls: Rc::default(),
    });
    scene(&mut renderer, 0, true);
    renderer.render(0);
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/blit-render-extensions.ppm".into());
    let mut output = std::io::BufWriter::new(std::fs::File::create(&path)?);
    write!(output, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
    for pixel in renderer.buffer().pixels() {
        let pixel = pixel.raw();
        output.write_all(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8])?;
    }
    println!("rendered custom path, cached shadow, and two pass blur to {path}");
    Ok(())
}

struct Path {
    points: Slice<[i16; 2]>,
    width: u16,
    color: Color,
}

#[derive(PartialEq, Hash)]
struct PathView<'a> {
    points: &'a [[i16; 2]],
    width: u16,
    color: Color,
}

impl CustomCommand for Path {
    type View<'a> = PathView<'a>;

    fn view<'a>(&'a self, arena: &'a Arena) -> PathView<'a> {
        PathView {
            points: arena.slice(self.points),
            width: self.width,
            color: self.color,
        }
    }
}

#[derive(Default)]
struct PathRenderer;

impl blit_cpu::extension::Command for Path {
    type Renderer = PathRenderer;
}

struct PreparedPath {
    segments: Slice<[[f32; 2]; 2]>,
    radius: f32,
    color: Color,
}

impl<P: Pixel> Extension<P> for PathRenderer {
    type Command = Path;
    type Prepared = PreparedPath;

    fn prepare(&mut self, path: PathView<'_>, cx: &mut PrepareContext<'_>) -> PreparedPath {
        PreparedPath {
            segments: cx.data.extend(path.points.windows(2).map(|points| {
                std::array::from_fn(|point| points[point].map(|coordinate| coordinate as f32 * cx.scale))
            })),
            radius: path.width as f32 * cx.scale * 0.5,
            color: path.color,
        }
    }
    fn render(&mut self, path: &PreparedPath, cx: &mut DrawContext<'_, P>) {
        cx.for_each_span(|span| {
            let segments = span.data.slice(path.segments);
            for (index, pixel) in span.pixels.iter_mut().enumerate() {
                let point = [span.x as f32 + index as f32 + 0.5, span.y as f32 + 0.5];
                let mut distance = f32::INFINITY;
                for [a, b] in segments {
                    let delta = [b[0] - a[0], b[1] - a[1]];
                    let length = delta[0] * delta[0] + delta[1] * delta[1];
                    let t = (((point[0] - a[0]) * delta[0] + (point[1] - a[1]) * delta[1])
                        / length.max(f32::MIN_POSITIVE))
                    .clamp(0.0, 1.0);
                    let dx = point[0] - a[0] - t * delta[0];
                    let dy = point[1] - a[1] - t * delta[1];
                    distance = distance.min(dx * dx + dy * dy);
                }
                let coverage = ((path.radius + 0.5 - distance.sqrt()).clamp(0.0, 1.0) * span.coverage as f32) as u8;
                pixel.blend(PremultipliedRgbaColor::new(path.color, coverage));
            }
        });
    }
}

#[derive(PartialEq, Hash)]
struct SoftShadow {
    blur: u16,
    color: Color,
}

#[derive(Default)]
struct Stats {
    prepares: Cell<usize>,
    builds: Cell<usize>,
    frames: Cell<usize>,
}

#[derive(Default)]
struct ShadowRenderer {
    masks: HashMap<[i32; 3], ImageHandle>,
    stats: Rc<Stats>,
}

impl blit_cpu::extension::Command for SoftShadow {
    type Renderer = ShadowRenderer;
}

struct PreparedShadow {
    image: ImageHandle,
    bounds: PhysicalRect,
    color: Color,
}

impl ShadowRenderer {
    fn new(stats: Rc<Stats>) -> Self {
        Self {
            masks: HashMap::new(),
            stats,
        }
    }
}

impl<P: Pixel> Extension<P> for ShadowRenderer {
    type Command = SoftShadow;
    type Prepared = PreparedShadow;

    fn prepare(&mut self, shadow: &SoftShadow, cx: &mut PrepareContext<'_>) -> PreparedShadow {
        self.stats.prepares.set(self.stats.prepares.get() + 1);
        let bounds = cx.bounds;
        let radius = (shadow.blur as f32 * cx.scale).round().max(1.0) as i32;
        let key = [bounds.width, bounds.height, radius];
        let image = if let Some(image) = self.masks.get(&key) {
            image.clone()
        } else {
            // eviction during preparation also exercises earlier prepared resource lifetimes
            self.masks.clear();
            self.stats.builds.set(self.stats.builds.get() + 1);
            let mut alpha = vec![0; bounds.width as usize * bounds.height as usize];
            for y in 0..bounds.height {
                for x in 0..bounds.width {
                    let horizontal = ((x + radius + 1).min(bounds.width - radius) - (x - radius).max(radius)).max(0);
                    let vertical = ((y + radius + 1).min(bounds.height - radius) - (y - radius).max(radius)).max(0);
                    alpha[(y * bounds.width + x) as usize] =
                        (horizontal * vertical * 255 / (2 * radius + 1).pow(2)) as u8;
                }
            }
            let image = cx.create_image(ImageData::new(
                ImagePixels::Owned(alpha.into_boxed_slice()),
                ImageFormat::Alpha8(Color::WHITE),
                bounds.width as usize,
                bounds.height as usize,
            ));
            self.masks.insert(key, image.clone());
            image
        };
        PreparedShadow {
            image,
            bounds,
            color: shadow.color,
        }
    }

    fn finish_frame(&mut self) {
        self.stats.frames.set(self.stats.frames.get() + 1);
    }
    fn render(&mut self, shadow: &PreparedShadow, cx: &mut DrawContext<'_, P>) {
        cx.for_each_span(|span| {
            let image = span.image(&shadow.image).unwrap();
            let start = (span.y - shadow.bounds.y) as usize * image.stride_bytes + (span.x - shadow.bounds.x) as usize;
            for (pixel, alpha) in span.pixels.iter_mut().zip(&image.pixels.bytes()[start..]) {
                pixel.blend(PremultipliedRgbaColor::new(
                    shadow.color,
                    (*alpha as u16 * span.coverage as u16 / 255) as u8,
                ));
            }
        });
    }
}

#[derive(PartialEq, Hash)]
struct BackdropBlur {
    radius: u16,
}

#[derive(Default)]
struct BlurRenderer {
    scratch: Vec<[u32; 3]>,
    calls: Rc<Cell<usize>>,
}

impl blit_cpu::extension::Command for BackdropBlur {
    type Renderer = BlurRenderer;
}

impl Extension<Xrgb8888> for BlurRenderer {
    type Command = BackdropBlur;
    type Prepared = usize;

    fn prepare(&mut self, blur: &BackdropBlur, cx: &mut PrepareContext<'_>) -> usize {
        (blur.radius as f32 * cx.scale).round() as usize
    }
    fn read_bounds(&self, blur: &BackdropBlur, output: PhysicalRect, scale: f32) -> Option<PhysicalRect> {
        let radius = (blur.radius as f32 * scale).round() as i32;
        Some(PhysicalRect::new(
            output.x.saturating_sub(radius),
            output.y.saturating_sub(radius),
            output.width.saturating_add(radius.saturating_mul(2)),
            output.height.saturating_add(radius.saturating_mul(2)),
        ))
    }

    fn render(&mut self, radius: &usize, cx: &mut DrawContext<'_, Xrgb8888>) {
        self.calls.set(self.calls.get() + 1);
        let source = cx.source().unwrap();
        let width = source.width();
        let height = source.height();
        self.scratch.resize(width * height, [0; 3]);
        for bounds in cx.output_regions() {
            for y in (bounds.y as usize).saturating_sub(*radius)
                ..(bounds.y as usize + bounds.height as usize + radius).min(height)
            {
                for x in bounds.x as usize..(bounds.x + bounds.width) as usize {
                    let start = x.saturating_sub(*radius);
                    let end = (x + radius + 1).min(width);
                    let mut channels = [0; 3];
                    for pixel in source.row(y, start..end) {
                        let pixel = pixel.raw();
                        for (channel, shift) in channels.iter_mut().zip([16, 8, 0]) {
                            *channel += (pixel >> shift) & 255;
                        }
                    }
                    self.scratch[y * width + x] = channels.map(|channel| channel / (end - start) as u32);
                }
            }
        }
        cx.for_each_span(|span| {
            let start = (span.y as usize).saturating_sub(*radius);
            let end = (span.y as usize + radius + 1).min(height);
            for (offset, pixel) in span.pixels.iter_mut().enumerate() {
                let x = span.x as usize + offset;
                let mut channels = [0; 3];
                for y in start..end {
                    for (channel, sample) in channels.iter_mut().zip(self.scratch[y * width + x]) {
                        *channel += sample;
                    }
                }
                let [red, green, blue] = channels.map(|channel| (channel / (end - start) as u32) as u8);
                pixel.blend(PremultipliedRgbaColor::new(Color::rgb(red, green, blue), span.coverage));
            }
        });
    }
}

fn scene<C>(renderer: &mut C, frame: usize, pass: bool)
where
    C: for<'a> Paint<Rectangle<'a>, Scalar = f32>
        + PaintCustom<SoftShadow>
        + PaintCustom<Path>
        + PaintCustom<BackdropBlur>,
{
    let background =
        Rectangle::new(LogicalRect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32)).background(Color::rgb(18, 22, 30));
    renderer.paint(background.area, background);
    renderer.paint_with(LogicalRect::new(12.0, 10.0, 76.0, 48.0), |_| SoftShadow {
        blur: 7,
        color: Color::rgba(0, 180, 255, 190),
    });
    let panel = Rectangle::new(LogicalRect::new(22.0, 20.0, 56.0, 28.0))
        .background(Color::rgb(35, 45, 65))
        .radius(BorderRadius::uniform(6.0));
    renderer.paint(panel.area, panel);
    renderer.paint_with(LogicalRect::new(6.0, 5.0, 144.0, 60.0), |data| Path {
        points: data.extend([[12, 20], [65, 20], [100, 53], [145, if frame == 0 { 15 } else { 22 }]].into_iter()),
        width: 3,
        color: Color::rgb(255, 190, 70),
    });
    for x in (0..WIDTH).step_by(8) {
        let stripe = Rectangle::new(LogicalRect::new(x as f32, 67.0, 4.0, 28.0)).background(if x == 16 && frame != 0 {
            Color::WHITE
        } else {
            Color::rgb((x * 7) as u8, 80, 150)
        });
        renderer.paint(stripe.area, stripe);
    }
    if pass {
        renderer.paint_with(LogicalRect::new(20.0, 65.0, 120.0, 28.0), |_| BackdropBlur {
            radius: 4,
        });
    }
    let overlay = Rectangle::new(LogicalRect::new(34.0, 18.0, 8.0, 8.0)).background(Color::WHITE);
    renderer.paint(overlay.area, overlay);
}

fn new_renderer<B: PixelBuffer>(buffer: B, config: RendererConfig) -> Renderer<B> {
    Renderer::new(
        buffer,
        TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap(),
        config,
    )
}

#[cfg(test)]
mod tests {
    use blit_cpu::{DamageTracker, Direct, PixelBuffer, RenderStrategy, TileDamage, damage::command::CommandDiff};
    use blit_gui::ClipContext;

    use super::*;

    const DAMAGE_WIDTH: usize = 384;
    const DAMAGE_HEIGHT: usize = 256;

    fn damage_scene<C>(renderer: &mut C, frame: usize)
    where
        C: for<'a> Paint<Rectangle<'a>, Scalar = f32> + PaintCustom<BackdropBlur> + ClipContext,
    {
        let background = Rectangle::new(LogicalRect::new(0.0, 0.0, DAMAGE_WIDTH as f32, DAMAGE_HEIGHT as f32))
            .background(Color::rgb(18, 30, 50));
        renderer.paint(background.area, background);
        let edit = Rectangle::new(LogicalRect::new(60.0, 80.0, 2.0, 2.0)).background(if frame == 0 {
            Color::rgb(240, 20, 10)
        } else {
            Color::WHITE
        });
        renderer.paint(edit.area, edit);
        if frame != 4 {
            let offset = if frame >= 5 { 12 } else { 0 };
            let bounds = LogicalRect::new((64 + offset) as f32, 48.0, 192.0, 128.0);
            let outer = LogicalRect::new(
                (64 + offset) as f32,
                48.0,
                if frame >= 6 { 150.0 } else { 192.0 },
                128.0,
            );
            renderer.push_clip(outer, BorderRadius::uniform(12.0));
            renderer.paint_with(bounds, |_| BackdropBlur {
                radius: if frame == 3 { 6 } else { 4 },
            });
            renderer.pop_clip();
            let overlay =
                Rectangle::new(LogicalRect::new(67.0, 80.0, 2.0, 2.0)).background(Color::rgba(100, 220, 40, 170));
            renderer.paint(overlay.area, overlay);
            renderer.push_clip(outer, BorderRadius::uniform(12.0));
            renderer.push_clip(bounds, BorderRadius::default());
            renderer.paint_with(bounds, |_| BackdropBlur { radius: 3 });
            renderer.pop_clip();
            renderer.pop_clip();
        }
        let overlay = Rectangle::new(LogicalRect::new(68.0, 80.0, 1.0, 1.0)).background(Color::rgba(
            255,
            180,
            0,
            if frame >= 7 { 128 } else { 255 },
        ));
        renderer.paint(overlay.area, overlay);
        let unrelated = Rectangle::new(LogicalRect::new(340.0, 220.0, 2.0, 2.0)).background(if frame >= 2 {
            Color::rgb(50, 100, 255)
        } else {
            Color::rgb(10, 200, 30)
        });
        renderer.paint(unrelated.area, unrelated);
        let overlay = Rectangle::new(LogicalRect::new(84.0, 80.0, 2.0, 2.0)).background(Color::rgba(255, 0, 255, 200));
        renderer.paint(overlay.area, overlay);
    }

    struct TrackedBuffer {
        buffer: VecBuffer<Xrgb8888>,
        reads: Cell<usize>,
        writes: usize,
        touched: Vec<bool>,
    }

    impl PixelBuffer for TrackedBuffer {
        type Pixel = Xrgb8888;
        fn width(&self) -> usize {
            self.buffer.width()
        }
        fn height(&self) -> usize {
            self.buffer.height()
        }
        fn line_mut(&mut self, _: usize) -> &mut [Xrgb8888] {
            panic!("untracked framebuffer write")
        }
        fn process_line(&mut self, y: usize, range: std::ops::Range<usize>, draw: impl FnOnce(&mut [Xrgb8888])) {
            self.writes += range.len();
            let start = y * self.width() + range.start;
            self.touched[start..start + range.len()].fill(true);
            self.buffer.process_line(y, range, draw);
        }
        fn read_line(&mut self, y: usize, range: std::ops::Range<usize>) -> &[Xrgb8888] {
            self.reads.set(self.reads.get() + range.len());
            self.buffer.read_line(y, range)
        }
    }

    #[test]
    fn pass_damage_keeps_small_edits_local_and_skips_unrelated_passes() {
        fn check<D: DamageTracker>(damage: D) {
            let calls = Rc::<Cell<usize>>::default();
            let mut incremental = new_renderer(
                TrackedBuffer {
                    buffer: VecBuffer::new(DAMAGE_WIDTH, DAMAGE_HEIGHT),
                    reads: Cell::new(0),
                    writes: 0,
                    touched: vec![false; DAMAGE_WIDTH * DAMAGE_HEIGHT],
                },
                RendererConfig {
                    damage_tile_size: 8,
                    ..RendererConfig::default()
                },
            )
            .strategy(Scanline::default())
            .damage_tracker(damage);
            incremental.register(BlurRenderer {
                scratch: Vec::new(),
                calls: calls.clone(),
            });
            let mut reference = new_renderer(
                VecBuffer::<Xrgb8888>::new(DAMAGE_WIDTH, DAMAGE_HEIGHT),
                RendererConfig::default(),
            );
            reference.register(BlurRenderer {
                scratch: Vec::new(),
                calls: Rc::default(),
            });
            for (step, frame) in [0, 0, 1, 1, 2, 2].into_iter().enumerate() {
                damage_scene(&mut incremental, frame);
                damage_scene(&mut reference, frame);
                incremental.buffer_mut().reads.set(0);
                incremental.buffer_mut().writes = 0;
                incremental.buffer_mut().touched.fill(false);
                let before_calls = calls.get();
                incremental.render(if step == 0 { 0 } else { 1 });
                reference.render(0);
                assert_eq!(
                    incremental
                        .buffer()
                        .buffer
                        .pixels()
                        .iter()
                        .zip(reference.buffer().pixels())
                        .enumerate()
                        .find(|(_, (actual, expected))| actual != expected),
                    None,
                    "step {step}"
                );
                let writes = incremental.buffer().writes;
                let reads = incremental.buffer().reads.get();
                if step == 2 {
                    let touched = incremental.buffer().touched.iter().filter(|pixel| **pixel).count();
                    println!(
                        "small source edit: {touched} distinct pixels rewritten, {writes} pixel writes, {reads} pixels read, {} pixels in frame",
                        DAMAGE_WIDTH * DAMAGE_HEIGHT
                    );
                    assert!(writes > 0 && writes < DAMAGE_WIDTH * DAMAGE_HEIGHT / 8, "{writes}");
                    assert!(reads > 0 && reads < DAMAGE_WIDTH * DAMAGE_HEIGHT / 8, "{reads}");
                    assert_eq!(calls.get() - before_calls, 2);
                } else if step == 4 {
                    assert_eq!(calls.get(), before_calls);
                    assert_eq!(reads, 0);
                    assert!(writes > 0 && writes < 256);
                } else if step != 0 {
                    assert_eq!(writes, 0);
                    assert_eq!(reads, 0);
                    assert_eq!(calls.get(), before_calls);
                }
            }
        }
        check(TileDamage::new(8));
        check(CommandDiff::default());
    }

    #[test]
    fn regional_passes_match_full_rendering_across_buffer_ages_and_scene_changes() {
        fn check<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default, D: DamageTracker>(damage: D) {
            let mut renderer = new_renderer(
                VecBuffer::<Xrgb8888>::new(DAMAGE_WIDTH, DAMAGE_HEIGHT),
                RendererConfig {
                    damage_tile_size: 8,
                    ..RendererConfig::default()
                },
            )
            .strategy(S::default())
            .damage_tracker(damage);
            renderer.register(BlurRenderer {
                scratch: Vec::new(),
                calls: Rc::default(),
            });
            let mut reference = new_renderer(
                VecBuffer::<Xrgb8888>::new(DAMAGE_WIDTH, DAMAGE_HEIGHT),
                RendererConfig::default(),
            );
            reference.register(BlurRenderer {
                scratch: Vec::new(),
                calls: Rc::default(),
            });
            let mut spare = VecBuffer::new(DAMAGE_WIDTH, DAMAGE_HEIGHT);
            for (index, frame) in [0, 0, 1, 1, 2, 3, 4, 5, 6, 7, 7, 1, 0].into_iter().enumerate() {
                damage_scene(&mut renderer, frame);
                damage_scene(&mut reference, frame);
                renderer.render(if index < 2 { 0 } else { 2 });
                reference.render(0);
                assert_eq!(
                    renderer
                        .buffer()
                        .pixels()
                        .iter()
                        .zip(reference.buffer().pixels())
                        .enumerate()
                        .find(|(_, (actual, expected))| actual != expected),
                    None,
                    "frame {index}"
                );

                std::mem::swap(renderer.buffer_mut(), &mut spare);
            }
        }
        check::<Direct, _>(TileDamage::new(8));
        check::<Scanline, _>(TileDamage::new(8));
        check::<Direct, _>(CommandDiff::default());
        check::<Scanline, _>(CommandDiff::default());
    }

    #[test]
    #[should_panic(expected = "outside its declared source footprint")]
    fn pass_reads_outside_declared_dependencies_are_rejected() {
        #[derive(PartialEq, Hash)]
        struct BadRead;
        #[derive(Default)]
        struct BadRenderer;
        impl blit_cpu::extension::Command for BadRead {
            type Renderer = BadRenderer;
        }
        impl Extension<Xrgb8888> for BadRenderer {
            type Command = BadRead;
            type Prepared = ();
            fn prepare(&mut self, _: &BadRead, _: &mut PrepareContext<'_>) {}
            fn read_bounds(&self, _: &BadRead, output: PhysicalRect, _: f32) -> Option<PhysicalRect> {
                Some(output)
            }
            fn render(&mut self, _: &(), cx: &mut DrawContext<'_, Xrgb8888>) {
                let _ = cx.source().unwrap().row(220, 340..341);
            }
        }
        let mut renderer = new_renderer(
            VecBuffer::<Xrgb8888>::new(DAMAGE_WIDTH, DAMAGE_HEIGHT),
            RendererConfig {
                damage_tile_size: 8,
                ..RendererConfig::default()
            },
        );
        renderer.register(BadRenderer);
        renderer.paint_with(LogicalRect::new(64.0, 64.0, 8.0, 8.0), |_| BadRead);
        renderer.render(0);
    }

    #[test]
    fn nonlocal_pass_dependencies_rebuild_source_before_later_overlays() {
        #[derive(Clone, Copy, PartialEq, Hash)]
        struct CopyArea(bool);
        #[derive(Default)]
        struct CopyRenderer;
        impl blit_cpu::extension::Command for CopyArea {
            type Renderer = CopyRenderer;
        }
        impl Extension<Xrgb8888> for CopyRenderer {
            type Command = CopyArea;
            type Prepared = ();
            fn prepare(&mut self, _: &CopyArea, _: &mut PrepareContext<'_>) {}
            fn read_bounds(&self, command: &CopyArea, output: PhysicalRect, _: f32) -> Option<PhysicalRect> {
                command.0.then_some(PhysicalRect::new(
                    output.x - 140,
                    output.y + 116,
                    output.width,
                    output.height,
                ))
            }
            fn render(&mut self, _: &(), cx: &mut DrawContext<'_, Xrgb8888>) {
                let Some(source) = cx.source() else {
                    cx.for_each_span(|span| {
                        Xrgb8888::blend_slice(span.pixels, PremultipliedRgbaColor::new(Color::WHITE, span.coverage));
                    });
                    return;
                };
                cx.for_each_span(|span| {
                    let x = (span.x - 140) as usize;
                    let source = source.row((span.y + 116) as usize, x..x + span.pixels.len());
                    for (pixel, source) in span.pixels.iter_mut().zip(source) {
                        let source = source.raw();
                        pixel.blend(PremultipliedRgbaColor::new(
                            Color::rgb((source >> 16) as u8, (source >> 8) as u8, source as u8),
                            span.coverage,
                        ));
                    }
                });
            }
        }
        let mut renderer = new_renderer(
            VecBuffer::<Xrgb8888>::new(DAMAGE_WIDTH, DAMAGE_HEIGHT),
            RendererConfig {
                damage_tile_size: 8,
                ..RendererConfig::default()
            },
        )
        .strategy(Scanline::default());
        renderer.register(CopyRenderer);
        for (backdrop, color) in [
            (true, Color::rgb(255, 0, 0)),
            (true, Color::rgb(0, 255, 0)),
            (false, Color::rgb(0, 255, 0)),
            (false, Color::rgb(0, 0, 255)),
            (true, Color::rgb(0, 0, 255)),
        ] {
            let background = Rectangle::new(LogicalRect::new(0.0, 0.0, DAMAGE_WIDTH as f32, DAMAGE_HEIGHT as f32))
                .background(Color::BLACK);
            renderer.paint(background.area, background);
            let source = Rectangle::new(LogicalRect::new(20.0, 180.0, 2.0, 2.0)).background(color);
            renderer.paint(source.area, source);
            renderer.paint_with(LogicalRect::new(160.0, 64.0, 16.0, 16.0), |_| CopyArea(backdrop));
            let overlay = Rectangle::new(LogicalRect::new(20.0, 180.0, 2.0, 2.0)).background(Color::WHITE);
            renderer.paint(overlay.area, overlay);
            renderer.render(1);
            let expected = if backdrop { color } else { Color::WHITE };
            assert_eq!(
                renderer.buffer().pixels()[64 * DAMAGE_WIDTH + 160],
                Xrgb8888::from_rgb(expected.red, expected.green, expected.blue)
            );
            assert_eq!(renderer.buffer().pixels()[180 * DAMAGE_WIDTH + 20].raw(), 0xffffff);
        }
    }

    #[test]
    fn native_rasters_cache_and_incremental_damage_match_full_rendering() {
        fn check<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default, D: DamageTracker>(damage: D) {
            let stats = Rc::<Stats>::default();
            let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(WIDTH, HEIGHT), RendererConfig::default())
                .strategy(S::default())
                .damage_tracker(damage);
            renderer.register(PathRenderer);
            renderer.register(ShadowRenderer::new(stats.clone()));
            let mut reference = new_renderer(VecBuffer::<Xrgb8888>::new(WIDTH, HEIGHT), RendererConfig::default());
            reference.register(PathRenderer);
            reference.register(ShadowRenderer::new(Rc::default()));
            for frame in [0, 0, 1, 1, 0] {
                scene(&mut renderer, frame, false);
                scene(&mut reference, frame, false);
                renderer.render(1);
                reference.render(0);
                assert_eq!(renderer.buffer().pixels(), reference.buffer().pixels());
                assert_eq!(renderer.buffer().pixels()[20 * WIDTH + 20].raw(), 0xffbe46);
                assert_eq!(renderer.buffer().pixels()[20 * WIDTH + 36].raw(), 0xffffff);
            }
            assert_eq!(stats.prepares.get(), 3);
            assert_eq!(stats.builds.get(), 1);
            assert_eq!(stats.frames.get(), 5);
        }
        check::<Direct, _>(TileDamage::new(8));
        check::<Scanline, _>(TileDamage::new(8));
        check::<Direct, _>(CommandDiff::default());
        check::<Scanline, _>(CommandDiff::default());
    }

    #[test]
    fn framebuffer_passes_observe_prefix_and_repaint_external_dependencies() {
        fn check<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default, D: DamageTracker>(damage: D) {
            let calls = Rc::<Cell<usize>>::default();
            let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(WIDTH, HEIGHT), RendererConfig::default())
                .strategy(S::default())
                .damage_tracker(damage);
            renderer.register(PathRenderer);
            renderer.register(ShadowRenderer::new(Rc::default()));
            renderer.register(BlurRenderer {
                scratch: Vec::new(),
                calls: calls.clone(),
            });
            let mut previous = 0;
            for (index, frame) in [0, 0, 1, 1, 0].into_iter().enumerate() {
                scene(&mut renderer, frame, true);
                renderer.paint_with(LogicalRect::new(100.0, 65.0, 40.0, 28.0), |_| BackdropBlur {
                    radius: 2,
                });
                renderer.render(1);
                let sample = renderer.buffer().pixels()[75 * WIDTH + 20].raw();
                if index == 2 {
                    assert_ne!(sample, previous);
                }
                previous = sample;
                assert_eq!(renderer.buffer().pixels()[20 * WIDTH + 36].raw(), 0xffffff);
            }
            assert_eq!(calls.get(), 6);
        }
        check::<Direct, _>(TileDamage::new(8));
        check::<Scanline, _>(TileDamage::new(8));
        check::<Direct, _>(CommandDiff::default());
        check::<Scanline, _>(CommandDiff::default());
    }

    #[test]
    fn cached_resources_survive_eviction_before_rasterization() {
        let mut renderer =
            new_renderer(VecBuffer::<Xrgb8888>::new(20, 10), RendererConfig::default()).strategy(Scanline::default());
        let stats = Rc::<Stats>::default();
        renderer.register(ShadowRenderer::new(stats.clone()));
        renderer.paint_with(LogicalRect::new(0.0, 0.0, 10.0, 10.0), |_| SoftShadow {
            blur: 1,
            color: Color::WHITE,
        });
        renderer.paint_with(LogicalRect::new(10.0, 0.0, 10.0, 10.0), |_| SoftShadow {
            blur: 2,
            color: Color::WHITE,
        });
        renderer.render(0);
        assert_eq!(stats.builds.get(), 2);
        assert_eq!(renderer.buffer().pixels()[5 * 20 + 5].raw(), 0xffffff);
        assert_eq!(renderer.buffer().pixels()[5 * 20 + 15].raw(), 0xffffff);
    }

    #[test]
    fn pass_barriers_read_current_frame_and_respect_nested_clips() {
        #[derive(PartialEq, Hash)]
        struct Shift;
        #[derive(Default)]
        struct ShiftRenderer;
        impl blit_cpu::extension::Command for Shift {
            type Renderer = ShiftRenderer;
        }
        impl Extension<Xrgb8888> for ShiftRenderer {
            type Command = Shift;
            type Prepared = ();
            fn prepare(&mut self, _: &Shift, _: &mut PrepareContext<'_>) {}
            fn read_bounds(&self, _: &Shift, output: PhysicalRect, _: f32) -> Option<PhysicalRect> {
                Some(PhysicalRect::new(
                    output.x,
                    output.y.saturating_sub(1).max(0),
                    output.width,
                    output.height,
                ))
            }

            fn render(&mut self, _: &(), cx: &mut DrawContext<'_, Xrgb8888>) {
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
        }
        let mut renderer =
            new_renderer(VecBuffer::<Xrgb8888>::new(12, 10), RendererConfig::default()).strategy(Scanline::default());
        renderer.register(ShiftRenderer);
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
            let source = Rectangle::new(LogicalRect::new(0.0, 0.0, 12.0, 1.0)).background(color);
            renderer.paint(source.area, source);
            renderer.paint_with(LogicalRect::new(0.0, 1.0, 12.0, 1.0), |_| Shift);
            renderer.push_clip(LogicalRect::new(1.0, 0.0, 9.0, 8.0), BorderRadius::uniform(3.0));
            renderer.push_clip(LogicalRect::new(2.0, 0.0, 6.0, 8.0), BorderRadius::default());
            renderer.paint_with(LogicalRect::new(0.0, 2.0, 12.0, 1.0), |_| Shift);
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
}
