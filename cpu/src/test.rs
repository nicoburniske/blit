use std::{ops::Range, time::Duration};

use blit::{
    animation::Easing,
    color::Color,
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    input::Input,
    interact::WidgetId,
    keyboard::KeyboardRequest,
    paint::{
        BorderRadius, BoxShadow, GradientStop, ImageFit, ImageRequest, ImageSampling, ImageTiling,
        LinearGradient, Rectangle, TextOptions, TextRequest, TextStyle,
    },
    paint_list::{ClipId, PaintList},
    platform::PlatformImpl,
    resource::{ImageData, ImageFormat, ImageId, ImagePixels, StringData, StringId},
    widget::Text,
    RepaintBuffer, Runtime,
};

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
struct BgrPixel {
    blue: u8,
    green: u8,
    red: u8,
}

impl Pixel for BgrPixel {
    fn blend_translucent(&mut self, color: PremultipliedRgbaColor) {
        let inverse = 255 - color.alpha as u16;
        self.red = (self.red as u16 * inverse / 255) as u8 + color.red;
        self.green = (self.green as u16 * inverse / 255) as u8 + color.green;
        self.blue = (self.blue as u16 * inverse / 255) as u8 + color.blue;
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self { Self { blue, green, red } }
}

struct TrackingBuffer {
    pixels: Vec<u32>,
    lines: Vec<usize>,
    ranges: Vec<Range<usize>>,
    width: usize,
    height: usize,
}

struct RuntimePlatform<B: PixelBuffer = VecBuffer<u32>, S: RenderStrategy<B> = Scanline> {
    renderer: Renderer<B, S>,
    repaint_buffer: RepaintBuffer,
}

impl<B: PixelBuffer + 'static, S: RenderStrategy<B> + 'static> PlatformImpl for RuntimePlatform<B, S> {
    fn render(&mut self, paint: &PaintList, damage: &[PhysicalRect]) { self.renderer.render(paint, damage) }

    fn screen(&mut self) -> PhysicalRect { self.renderer.screen() }

    fn repaint_buffer(&self) -> RepaintBuffer { self.repaint_buffer }

    fn create_image(&mut self, data: ImageData) -> ImageId { self.renderer.create_image(data) }

    fn drop_image(&mut self, image: ImageId) { self.renderer.drop_image(image) }

    fn create_string(&mut self, string: StringData) -> StringId { self.renderer.create_string(string) }

    fn drop_string(&mut self, string: StringId) { self.renderer.drop_string(string) }

    fn string(&self, string: StringId) -> &str { self.renderer.string(string) }

    fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        self.renderer.text_offset_at_position(request, position)
    }

    fn measure_text(&mut self, request: &TextRequest) -> LogicalSize { self.renderer.measure_text(request) }

    fn measure_text_height(&mut self, request: &TextRequest) -> f32 {
        self.renderer.measure_text_height(request)
    }

    fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect {
        self.renderer.text_cursor_rect(request, byte_offset)
    }

    fn show_keyboard(&mut self, _: &KeyboardRequest<'_>) {}
}

struct SwappedBuffer {
    pixels: [Vec<u32>; 2],
    active: usize,
    width: usize,
    height: usize,
    rendered_pixels: usize,
}

impl SwappedBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: std::array::from_fn(|_| vec![0; width * height]),
            active: 0,
            width,
            height,
            rendered_pixels: 0,
        }
    }

    fn swap(&mut self) { self.active ^= 1; }

    fn pixels(&self) -> &[u32] { &self.pixels[self.active] }

    fn take_rendered_pixels(&mut self) -> usize { std::mem::take(&mut self.rendered_pixels) }

    fn replace_inactive(&mut self, pixel: u32) { self.pixels[self.active ^ 1].fill(pixel); }
}

impl PixelBuffer for SwappedBuffer {
    type Pixel = u32;

    fn width(&self) -> usize { self.width }

    fn height(&self) -> usize { self.height }

    fn line_mut(&mut self, line: usize) -> &mut [u32] {
        let start = line * self.width;
        &mut self.pixels[self.active][start..start + self.width]
    }

    fn process_line(&mut self, line: usize, range: Range<usize>, process: impl FnOnce(&mut [u32])) {
        self.rendered_pixels += range.len();
        let start = line * self.width;
        process(&mut self.pixels[self.active][start + range.start..start + range.end]);
    }
}

struct CoherenceHarness {
    partial: Runtime<RuntimePlatform<SwappedBuffer>>,
    full: Runtime<RuntimePlatform<SwappedBuffer>>,
    frame: usize,
    id: WidgetId,
}

impl CoherenceHarness {
    fn new(width: usize, height: usize) -> Self {
        let make_runtime = || {
            Runtime::new(RuntimePlatform {
                renderer: Renderer::new(SwappedBuffer::new(width, height), renderer_config())
                    .strategy(Scanline::default()),
                repaint_buffer: RepaintBuffer::Swapped,
            })
        };
        Self {
            partial: make_runtime(),
            full: make_runtime(),
            frame: 0,
            id: WidgetId::new("coherence harness movement"),
        }
    }

    fn render(&mut self, position: f32) -> (usize, usize) {
        self.render_at(Duration::from_millis(self.frame as u64), position, Duration::ZERO)
    }

    fn render_at(&mut self, time: Duration, position: f32, duration: Duration) -> (usize, usize) {
        if self.frame != 0 {
            self.partial.platform().renderer.buffer_mut().swap();
            self.full.platform().renderer.buffer_mut().swap();
        }
        let id = self.id;
        self.partial.render(time, Input::None, |ui| render_coherence_scene(ui, id, position, duration));
        self.full.invalidate_all();
        self.full.render(time, Input::None, |ui| render_coherence_scene(ui, id, position, duration));

        assert_eq!(
            self.partial.platform().renderer.buffer().pixels(),
            self.full.platform().renderer.buffer().pixels(),
            "frame {} at position {position}",
            self.frame
        );
        let partial = self.partial.platform().renderer.buffer_mut().take_rendered_pixels();
        let full = self.full.platform().renderer.buffer_mut().take_rendered_pixels();
        self.frame += 1;
        (partial, full)
    }
}

fn render_coherence_scene(ui: &mut blit::Ui, id: WidgetId, position: f32, duration: Duration) {
    let screen = ui.screen();
    Rectangle::new(screen).background(Color::from_rgba8(24, 36, 48, 255)).render(ui);
    for (index, color) in [
        Color::from_rgba8(90, 30, 40, 255),
        Color::from_rgba8(30, 80, 50, 255),
        Color::from_rgba8(40, 50, 100, 255),
        Color::from_rgba8(100, 80, 30, 255),
    ]
    .into_iter()
    .enumerate()
    {
        Rectangle::new(LogicalRect {
            x: index as f32 * screen.width / 4.0,
            width: screen.width / 4.0,
            ..screen
        })
        .background(color)
        .render(ui);
    }

    let mut movement = ui.animate(id, position, duration, Easing::Linear);
    let x = movement.value();
    Rectangle::new(LogicalRect { x, y: 6.0, width: 12.0, height: 20.0 })
        .background(Color::from_rgba8(20, 20, 20, 160))
        .uniform_radius(4.0)
        .render(&mut movement);
    Rectangle::new(LogicalRect { x: x + 4.0, y: 10.0, width: 4.0, height: 12.0 })
        .background(if position < screen.width / 2.0 {
            Color::from_rgba8(230, 220, 180, 255)
        } else {
            Color::from_rgba8(180, 210, 240, 255)
        })
        .render(&mut movement);
}

impl PixelBuffer for TrackingBuffer {
    type Pixel = u32;

    fn width(&self) -> usize { self.width }

    fn height(&self) -> usize { self.height }

    fn line_mut(&mut self, line: usize) -> &mut [u32] {
        let start = line * self.width;
        &mut self.pixels[start..start + self.width]
    }

    fn process_line(&mut self, line: usize, range: Range<usize>, process: impl FnOnce(&mut [u32])) {
        self.lines.push(line);
        self.ranges.push(range.clone());
        let start = line * self.width;
        process(&mut self.pixels[start + range.start..start + range.end]);
    }
}

fn renderer_config() -> RendererConfig {
    RendererConfig {
        fonts: vec![FontFace {
            id: FontId::default(),
            weight: 400,
            font: Font::from_static(include_bytes!("../../resources/fonts/Montserrat-Regular.ttf")).unwrap(),
        }],
        font_metric_cache_capacity: 256,
        glyph_cache_capacity: 1024 * 1024,
        paragraph_cache_capacity: 1024 * 1024,
        shadow_cache_capacity: 1024 * 1024,
    }
}

#[test]
fn renderer_supports_custom_pixel_layouts() {
    let mut renderer = Renderer::new(VecBuffer::<BgrPixel>::new(32, 24), renderer_config());
    let m = renderer.create_string(StringData::Static("M"));
    let clip = PhysicalRect { x: 0, y: 0, width: 32, height: 24 };
    let mut paint = PaintList::default();
    paint.push_rectangle(
        Rectangle::new(LogicalRect { x: 0.0, y: 0.0, width: 32.0, height: 24.0 })
            .background(Color::from_rgba8(12, 34, 56, 255)),
        clip,
        ClipId::default(),
    );
    renderer.render(&paint, &[clip]);
    assert_eq!(renderer.buffer().pixels()[0], BgrPixel { blue: 56, green: 34, red: 12 });

    paint.clear();
    paint.push_text(
        TextRequest {
            text: m.into(),
            area: LogicalRect { x: 0.0, y: 0.0, width: 32.0, height: 24.0 },
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions::default(),
            intrinsic_height: false,
        },
        clip,
        ClipId::default(),
    );
    renderer.render(&paint, &[clip]);
    assert!(renderer.buffer().pixels().iter().any(|pixel| pixel.red > 12));

    let request = TextRequest {
        text: "abc".into(),
        area: LogicalRect { x: 0.0, y: 0.0, width: 32.0, height: 24.0 },
        offset_x: 0.0,
        color: Color::WHITE,
        style: TextStyle::default(),
        options: TextOptions::default(),
        intrinsic_height: false,
    };
    assert_eq!(renderer.text_offset_at_position(&request, LogicalPoint { x: 100.0, y: 12.0 },), "abc".len());
    let start = renderer.text_cursor_rect(&request, 0);
    let end = renderer.text_cursor_rect(&request, "abc".len());
    assert!(end.x > start.x);
}

#[test]
fn commands_outside_damage_are_not_prepared() {
    let mut renderer = Renderer::new(VecBuffer::<u32>::new(8, 4), renderer_config());
    let damaged = LogicalRect { x: 0.0, y: 0.0, width: 2.0, height: 2.0 };
    let outside = LogicalRect { x: 4.0, y: 0.0, width: 4.0, height: 4.0 };
    let mut paint = PaintList::default();
    paint.push_text(
        TextRequest {
            text: StringId(u64::MAX).into(),
            area: outside,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions::default(),
            intrinsic_height: false,
        },
        outside.to_physical(1.0),
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(damaged).background(Color::WHITE),
        damaged.to_physical(1.0),
        ClipId::default(),
    );

    renderer.render(&paint, &[damaged.to_physical(1.0)]);

    assert_eq!(
        renderer.buffer().pixels(),
        [
            0xffffff, 0xffffff, 0, 0, 0, 0, 0, 0, 0xffffff, 0xffffff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );
}

#[test]
fn partial_frames_match_full_redraw() {
    let mut harness = CoherenceHarness::new(64, 32);
    for position in [4.0, 4.0, 9.0, 17.0, 29.0, 41.0, 33.0, 18.0, 7.0, 4.0] {
        harness.render(position);
    }
    assert!(harness.partial.has_pending_redraw());
    harness.render(4.0);
    assert!(!harness.partial.has_pending_redraw());

    harness.render(44.0);
    harness.render(44.0);
    harness.render_at(Duration::from_millis(10), 4.0, Duration::from_millis(100));
    for time in [35, 60, 85, 110] {
        harness.render_at(Duration::from_millis(time), 4.0, Duration::from_millis(100));
    }
    assert!(harness.partial.has_pending_redraw());
    harness.render_at(Duration::from_millis(111), 4.0, Duration::from_millis(100));
    assert!(!harness.partial.has_pending_redraw());

    harness.partial.platform().renderer.buffer_mut().replace_inactive(0x00ff_00ff);
    harness.full.platform().renderer.buffer_mut().replace_inactive(0x00ff_00ff);
    harness.partial.invalidate_all();
    harness.render(4.0);
    harness.render(4.0);
    assert!(!harness.partial.has_pending_redraw());

    let mut random = 0x4d59_5df4_d0f3_3173_u64;
    let mut position = 4.0;
    for _ in 0..256 {
        random = random.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        if random >> 61 != 0 {
            position = 4.0 + ((random >> 32) % 45) as f32;
        }
        harness.render(position);
    }
    for _ in 0..3 {
        if !harness.partial.has_pending_redraw() {
            break;
        }
        harness.render(position);
    }
    assert!(!harness.partial.has_pending_redraw());
}

#[test]
fn partial_drag_rasterizes_less_than_full_redraw() {
    let mut harness = CoherenceHarness::new(64, 32);
    harness.render(4.0);
    harness.render(4.0);

    let mut partial_pixels = 0;
    let mut full_pixels = 0;
    for position in [12.0, 20.0, 28.0, 36.0, 44.0, 44.0] {
        let (partial, full) = harness.render(position);
        partial_pixels += partial;
        full_pixels += full;
    }

    assert!(!harness.partial.has_pending_redraw());
    assert_eq!(full_pixels, 6 * 64 * 32);
    assert!(partial_pixels * 4 < full_pixels, "partial={partial_pixels}, full={full_pixels}");
}

#[test]
fn dropped_image_slots_are_reused_after_end_frame() {
    static PIXEL: [u8; 4] = [255, 255, 255, 255];
    let mut renderer = Renderer::new(VecBuffer::<u32>::new(1, 1), renderer_config());
    let texture = ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgba8, 1, 1);

    let first = renderer.create_image(texture);
    renderer.drop_image(first);
    let first_key = RendererImageId::from(KeyData::from_ffi(first.0));
    assert!(renderer.context.images.contains_key(first_key));

    renderer.render(&PaintList::default(), &[]);
    assert!(!renderer.context.images.contains_key(first_key));

    let second = renderer.create_image(ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgba8, 1, 1));
    assert_ne!(second, first);
}

#[test]
fn image_alpha_rows_are_cached_and_used() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static BLENDED: AtomicUsize = AtomicUsize::new(0);
    static COPIED: AtomicUsize = AtomicUsize::new(0);

    #[derive(Clone, Copy)]
    struct TrackingPixel;

    impl Pixel for TrackingPixel {
        fn blend_translucent(&mut self, _color: PremultipliedRgbaColor) { unreachable!() }

        fn from_rgb(_red: u8, _green: u8, _blue: u8) -> Self { Self }

        fn blend_texture_slice_rgba(pixels: &mut [Self], source: &[PremultipliedRgbaColor], _opacity: u8) {
            BLENDED.fetch_add(pixels.len().min(source.len()), Ordering::Relaxed);
        }

        fn copy_texture_slice_rgba(pixels: &mut [Self], source: &[PremultipliedRgbaColor]) {
            COPIED.fetch_add(pixels.len().min(source.len()), Ordering::Relaxed);
        }

        fn blend_texture_slice_alpha(pixels: &mut [Self], _color: Color, alpha: &[u8]) {
            BLENDED.fetch_add(pixels.len().min(alpha.len()), Ordering::Relaxed);
        }
    }

    let alpha =
        [0, 255, 255, 255, 0, 0, 0, 255, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 0, 0, 255, 255];
    let mut pixels = [0; 6 * 4 * 4];
    for (pixel, alpha) in pixels.chunks_exact_mut(4).zip(alpha) {
        pixel.copy_from_slice(&[alpha / 2, alpha / 4, alpha / 8, alpha]);
    }
    let mut renderer =
        Renderer::new(VecBuffer::<TrackingPixel>::new(6, 4), renderer_config()).strategy(Scanline::default());
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Owned(pixels.into()),
        ImageFormat::Rgba8Premultiplied,
        6,
        4,
    ));
    let key = RendererImageId::from(KeyData::from_ffi(image.0));
    let rows = &renderer.context.images[key].alpha_rows;
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([4, 4, 6, 6]));
    assert!(rows.iter().map(|row| row.opaque_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.opaque_end).eq([4, 4, 2, 2]));
    assert!(renderer.context.images[key].has_opaque_spans);
    assert!(!renderer.context.images[key].opaque);

    let screen = renderer.screen();
    let request = ImageRequest {
        image,
        area: screen.to_logical(1.0),
        fit: ImageFit::Fill,
        sampling: ImageSampling::Nearest,
        opacity: 1.0,
        colorize: None,
        nine_slice: None,
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
    };
    BLENDED.store(0, Ordering::Relaxed);
    COPIED.store(0, Ordering::Relaxed);
    let mut paint = PaintList::default();
    paint.push_image(request, screen, ClipId::default());
    renderer.render(&paint, &[screen]);
    assert_eq!(COPIED.load(Ordering::Relaxed), 10);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 8);

    BLENDED.store(0, Ordering::Relaxed);
    COPIED.store(0, Ordering::Relaxed);
    paint.clear();
    paint.push_image(ImageRequest { opacity: 0.5, ..request }, screen, ClipId::default());
    renderer.render(&paint, &[screen]);
    assert_eq!(COPIED.load(Ordering::Relaxed), 0);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 18);

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Owned(
            [0, 64, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 2, 0, 0, 255, 255, 255, 255, 255, 255].into(),
        ),
        ImageFormat::Alpha8(Color::WHITE),
        6,
        4,
    ));
    let key = RendererImageId::from(KeyData::from_ffi(image.0));
    let rows = &renderer.context.images[key].alpha_rows;
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 0, 1, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([3, 0, 4, 6]));
    assert!(rows.iter().all(|row| row.opaque_start == 0 && row.opaque_end == 0));
    BLENDED.store(0, Ordering::Relaxed);
    paint.clear();
    paint.push_image(ImageRequest { image, opacity: 1.0, ..request }, screen, ClipId::default());
    renderer.render(&paint, &[screen]);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 11);
}

#[test]
fn direct_preserves_exact_overlapping_damage() {
    let mut renderer = Renderer::new(VecBuffer::<u32>::new(4, 4), renderer_config());
    let screen = renderer.screen();
    let mut paint = PaintList::default();
    paint.push_rectangle(
        Rectangle::new(screen.to_logical(1.0)).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(
        &paint,
        &[
            PhysicalRect { x: 0, y: 0, width: 1, height: 1 },
            PhysicalRect { x: 0, y: 2, width: 3, height: 2 },
            PhysicalRect { x: 2, y: 0, width: 2, height: 3 },
        ],
    );

    let pixels = renderer.buffer().pixels();
    assert_ne!(pixels[0], 0);
    let painted = pixels[0];
    assert_eq!(
        pixels,
        [
            painted, 0, painted, painted, 0, 0, painted, painted, painted, painted, painted, painted,
            painted, painted, painted, 0,
        ]
    );
}

#[test]
fn direct_does_not_merge_touching_damage() {
    let mut renderer = Renderer::new(VecBuffer::<u32>::new(3, 3), renderer_config());
    let screen = renderer.screen();
    let mut paint = PaintList::default();
    paint.push_rectangle(
        Rectangle::new(screen.to_logical(1.0)).background(Color::WHITE),
        screen,
        ClipId::default(),
    );
    renderer.render(
        &paint,
        &[PhysicalRect { x: 0, y: 0, width: 2, height: 1 }, PhysicalRect { x: 2, y: 1, width: 1, height: 2 }],
    );

    assert_eq!(renderer.buffer().pixels(), [0xffffff, 0xffffff, 0, 0, 0, 0xffffff, 0, 0, 0xffffff]);
}

#[test]
fn direct_preserves_damage_beyond_stack_capacity() {
    let mut renderer = Renderer::new(VecBuffer::<u32>::new(9, 1), renderer_config());
    let screen = renderer.screen();
    let damage: [PhysicalRect; 9] =
        std::array::from_fn(|x| PhysicalRect { x: x as i32, y: 0, width: 1, height: 1 });
    let mut paint = PaintList::default();
    paint.push_rectangle(
        Rectangle::new(screen.to_logical(1.0)).background(Color::WHITE),
        screen,
        ClipId::default(),
    );
    renderer.render(&paint, &damage);

    assert!(renderer.buffer().pixels().iter().all(|pixel| *pixel == 0xffffff));
}

#[test]
fn frame_is_rendered_once_per_affected_line_in_order() {
    let mut renderer = Renderer::new(
        TrackingBuffer { pixels: vec![0; 16], lines: Vec::new(), ranges: Vec::new(), width: 4, height: 4 },
        renderer_config(),
    )
    .strategy(Scanline::default());
    let damage =
        [PhysicalRect { x: 0, y: 2, width: 4, height: 1 }, PhysicalRect { x: 0, y: 0, width: 4, height: 1 }];
    let mut paint = PaintList::default();
    paint.push_rectangle(
        Rectangle::new(LogicalRect { x: 0.0, y: 0.0, width: 4.0, height: 4.0 }).background(Color::WHITE),
        PhysicalRect { x: 0, y: 0, width: 4, height: 4 },
        ClipId::default(),
    );
    renderer.render(&paint, &damage);

    assert_eq!(renderer.buffer().lines, [0, 2]);
    assert_eq!(renderer.buffer().ranges, [0..4, 0..4]);
}

#[test]
fn scanline_merges_overlapping_damage_per_line() {
    let mut renderer = Renderer::new(
        TrackingBuffer { pixels: vec![0; 20], lines: Vec::new(), ranges: Vec::new(), width: 5, height: 4 },
        renderer_config(),
    )
    .strategy(Scanline::default());
    let mut paint = PaintList::default();
    paint.push_rectangle(
        Rectangle::new(LogicalRect { x: 0.0, y: 0.0, width: 5.0, height: 4.0 }).background(Color::WHITE),
        PhysicalRect { x: 0, y: 0, width: 5, height: 4 },
        ClipId::default(),
    );
    renderer.render(
        &paint,
        &[PhysicalRect { x: 0, y: 0, width: 3, height: 3 }, PhysicalRect { x: 2, y: 1, width: 3, height: 3 }],
    );

    assert_eq!(renderer.buffer().lines, [0, 1, 2, 3]);
    assert_eq!(renderer.buffer().ranges, [0..3, 0..5, 0..5, 2..5]);
}

#[test]
fn scanline_only_borrows_dirty_horizontal_ranges() {
    let mut renderer = Renderer::new(
        TrackingBuffer { pixels: vec![0; 8], lines: Vec::new(), ranges: Vec::new(), width: 4, height: 2 },
        renderer_config(),
    )
    .strategy(Scanline::default());
    let damage = [PhysicalRect { x: 1, y: 0, width: 2, height: 1 }];
    let mut paint = PaintList::default();
    paint.push_rectangle(
        Rectangle::new(LogicalRect { x: 0.0, y: 0.0, width: 4.0, height: 1.0 }).background(Color::WHITE),
        PhysicalRect { x: 0, y: 0, width: 4, height: 2 },
        ClipId::default(),
    );
    renderer.render(&paint, &damage);

    assert_eq!(renderer.buffer().ranges.len(), 1);
    assert_eq!(renderer.buffer().ranges[0], 1..3);
    assert_eq!(renderer.buffer().pixels, [0, 0xffffff, 0xffffff, 0, 0, 0, 0, 0]);
}

#[test]
fn scanline_skips_commands_behind_opaque_content() {
    static RECTANGLE_PIXELS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    #[derive(Clone, Copy, Default)]
    struct CountingPixel {
        color: u32,
        draws: u8,
    }

    impl Pixel for CountingPixel {
        fn blend_translucent(&mut self, color: PremultipliedRgbaColor) {
            self.color.blend(color);
            self.draws += 1;
        }

        fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
            Self { color: <u32 as Pixel>::from_rgb(red, green, blue), draws: 0 }
        }

        fn blend_slice(pixels: &mut [Self], color: PremultipliedRgbaColor) {
            if color.alpha != 0 {
                RECTANGLE_PIXELS.fetch_add(pixels.len(), std::sync::atomic::Ordering::Relaxed);
            }
            match color.alpha {
                0 => {}
                255 => pixels.iter_mut().for_each(|pixel| {
                    pixel.color = <u32 as Pixel>::from_rgb(color.red, color.green, color.blue);
                    pixel.draws += 1;
                }),
                _ => pixels.iter_mut().for_each(|pixel| pixel.blend(color)),
            }
        }
    }

    let mut renderer =
        Renderer::new(VecBuffer::<CountingPixel>::new(4, 2), renderer_config()).strategy(Scanline::default());
    let screen = renderer.screen();
    let area = LogicalRect { width: 4.0, height: 2.0, ..LogicalRect::default() };
    let mut paint = PaintList::default();
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(0, 255, 0, 255)),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(0, 0, 255, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(&paint, &[screen]);

    assert!(renderer.buffer().pixels().iter().all(|pixel| pixel.draws == 2));

    let mut renderer =
        Renderer::new(VecBuffer::<CountingPixel>::new(8, 7), renderer_config()).strategy(Scanline::default());
    let screen = renderer.screen();
    let damage = PhysicalRect { y: 3, height: 1, ..screen };
    let area = LogicalRect { width: 8.0, height: 7.0, ..LogicalRect::default() };
    paint.clear();
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(0, 255, 0, 255)).uniform_radius(3.0),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(0, 0, 255, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(&paint, &[damage]);

    assert!(renderer.buffer().pixels()[3 * 8..4 * 8].iter().all(|pixel| pixel.draws == 2));

    static IMAGE_PIXEL: [u8; 4] = [0, 255, 0, 255];
    let mut renderer =
        Renderer::new(VecBuffer::<CountingPixel>::new(4, 2), renderer_config()).strategy(Scanline::default());
    let image =
        renderer.create_image(ImageData::new(ImagePixels::Static(&IMAGE_PIXEL), ImageFormat::Rgba8, 1, 1));
    let screen = renderer.screen();
    let area = LogicalRect { width: 4.0, height: 2.0, ..LogicalRect::default() };
    let image = ImageRequest {
        image,
        area,
        fit: ImageFit::Fill,
        sampling: ImageSampling::Nearest,
        opacity: 1.0,
        colorize: None,
        nine_slice: None,
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
    };
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
    paint.clear();
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    paint.push_image(image, screen, ClipId::default());
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(0, 0, 255, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(&paint, &[screen]);

    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 8);

    static TRANSPARENT_IMAGE_PIXEL: [u8; 4] = [0, 255, 0, 254];
    let transparent_image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&TRANSPARENT_IMAGE_PIXEL),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    let transparent_image = ImageRequest { image: transparent_image, ..image };
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
    paint.clear();
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    paint.push_image(transparent_image, screen, ClipId::default());
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(0, 0, 255, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(&paint, &[screen]);

    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 16);

    static PARTIAL_IMAGE_PIXELS: [u8; 24] =
        [0, 0, 0, 0, 0, 128, 0, 128, 0, 255, 0, 255, 0, 255, 0, 255, 0, 128, 0, 128, 0, 0, 0, 0];
    static UNDERLAY_ALPHA: [u8; 1] = [128];
    let mut renderer =
        Renderer::new(VecBuffer::<CountingPixel>::new(6, 1), renderer_config()).strategy(Scanline::default());
    let partial_image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&PARTIAL_IMAGE_PIXELS),
        ImageFormat::Rgba8Premultiplied,
        6,
        1,
    ));
    let screen = renderer.screen();
    let underlay = renderer.create_image(ImageData::new(
        ImagePixels::Static(&UNDERLAY_ALPHA),
        ImageFormat::Alpha8(Color::BLACK),
        1,
        1,
    ));
    let underlay = ImageRequest {
        image: underlay,
        area: screen.to_logical(1.0),
        fit: ImageFit::Fill,
        sampling: ImageSampling::Nearest,
        opacity: 1.0,
        colorize: None,
        nine_slice: None,
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
    };
    let partial_image = ImageRequest {
        image: partial_image,
        area: screen.to_logical(1.0),
        fit: ImageFit::Fill,
        sampling: ImageSampling::Nearest,
        opacity: 1.0,
        colorize: None,
        nine_slice: None,
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
    };
    let background = Rectangle::new(screen.to_logical(1.0)).background(Color::from_rgba8(255, 0, 0, 128));
    let overlay = Rectangle::new(screen.to_logical(1.0)).background(Color::from_rgba8(0, 0, 255, 128));
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
    paint.clear();
    paint.push_rectangle(background, screen, ClipId::default());
    paint.push_image(underlay, screen, ClipId::default());
    paint.push_image(partial_image, screen, ClipId::default());
    paint.push_rectangle(overlay, screen, ClipId::default());
    renderer.render(&paint, &[screen]);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 14);
    for (rendered, source) in renderer.buffer().pixels().iter().zip(PARTIAL_IMAGE_PIXELS.chunks_exact(4)) {
        let mut expected = 0;
        expected.blend(PremultipliedRgbaColor::new(Color::from_rgba8(255, 0, 0, 128), 255));
        expected.blend(PremultipliedRgbaColor::new(Color::BLACK, 128));
        expected.blend(PremultipliedRgbaColor {
            red: source[0],
            green: source[1],
            blue: source[2],
            alpha: source[3],
        });
        expected.blend(PremultipliedRgbaColor::new(Color::from_rgba8(0, 0, 255, 128), 255));
        assert_eq!(rendered.color, expected);
    }

    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
    paint.clear();
    paint.push_rectangle(background, screen, ClipId::default());
    paint.push_image(underlay, screen, ClipId::default());
    paint.push_image(ImageRequest { opacity: 0.5, ..partial_image }, screen, ClipId::default());
    paint.push_rectangle(overlay, screen, ClipId::default());
    renderer.render(&paint, &[screen]);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 18);
}

#[test]
fn cached_dirty_ranges_match_direct_rendering() {
    let mut direct = Renderer::new(VecBuffer::<u32>::new(8, 8), renderer_config());
    let mut scanline =
        Renderer::new(VecBuffer::<u32>::new(8, 8), renderer_config()).strategy(Scanline::default());
    let red = Rectangle::new(LogicalRect { x: 0.0, y: 0.0, width: 8.0, height: 8.0 })
        .background(Color::from_rgba8(255, 0, 0, 128));
    let green = Rectangle::new(red.area).background(Color::from_rgba8(0, 255, 0, 128));
    let red_clips =
        [PhysicalRect { x: 0, y: 0, width: 3, height: 3 }, PhysicalRect { x: 5, y: 5, width: 3, height: 3 }];
    let green_clips =
        [PhysicalRect { x: 5, y: 0, width: 3, height: 3 }, PhysicalRect { x: 0, y: 5, width: 3, height: 3 }];
    let damage = [red_clips[0], red_clips[1], green_clips[0], green_clips[1]];
    let clip = PhysicalRect { x: 0, y: 0, width: 8, height: 8 };
    let mut paint = PaintList::default();
    paint.push_rectangle(red, clip, ClipId::default());
    paint.push_rectangle(green, clip, ClipId::default());
    direct.render(&paint, &damage);
    scanline.render(&paint, &damage);

    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
}

#[test]
fn box_shadows_match_between_strategies_and_cache_sizes() {
    fn render<S: RenderStrategy<VecBuffer<u32>>>(strategy: S) -> Renderer<VecBuffer<u32>, S> {
        let mut renderer = Renderer::new(VecBuffer::<u32>::new(128, 96), renderer_config())
            .with_scale_factor(2.0)
            .strategy(strategy);
        let screen = renderer.screen();
        let first = BoxShadow::new(
            LogicalRect { x: 12.0, y: 10.0, width: 36.0, height: 24.0 },
            Color::from_rgba8(220, 40, 20, 180),
        )
        .radius(BorderRadius { top_left: 6.0, top_right: 6.0, bottom_right: 6.0, bottom_left: 6.0 })
        .offset(2.0, 3.0)
        .blur(5.0)
        .spread(1.0);
        let second = BoxShadow {
            area: LogicalRect { x: 4.0, y: 24.0, width: 52.0, height: 20.0 },
            color: Color::from_rgba8(20, 80, 220, 140),
            ..first
        };
        let mut paint = PaintList::default();
        paint.push_box_shadow(first, screen, ClipId::default());
        paint.push_box_shadow(second, screen, ClipId::default());
        renderer.render(&paint, &[screen]);
        assert_eq!(renderer.context.images.len(), 2);
        paint.clear();
        paint.push_box_shadow(
            BoxShadow { area: LogicalRect { x: 20.0, ..first.area }, ..first },
            screen,
            ClipId::default(),
        );
        renderer.render(&paint, &[screen]);
        assert_eq!(renderer.context.images.len(), 2);
        renderer
    }

    let direct = render(Direct::default());
    let scanline = render(Scanline::default());
    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
    assert!(direct.buffer().pixels().iter().any(|pixel| *pixel != 0));
}

#[test]
fn gradient_borders_match_between_strategies_and_rounded_clips() {
    fn render<S: RenderStrategy<VecBuffer<u32>>>(strategy: S) -> Renderer<VecBuffer<u32>, S> {
        let mut renderer = Renderer::new(VecBuffer::<u32>::new(48, 36), renderer_config()).strategy(strategy);
        let screen = renderer.screen();
        let mut paint = PaintList::default();
        let clip = paint.push_clip(
            ClipId::default(),
            LogicalRect { x: 2.0, y: 2.0, width: 44.0, height: 32.0 },
            BorderRadius { top_left: 10.0, top_right: 10.0, bottom_right: 10.0, bottom_left: 10.0 },
        );
        {
            let stops = [
                GradientStop::new(0.0, Color::from_rgba8(255, 32, 16, 220)),
                GradientStop::new(0.4, Color::from_rgba8(40, 240, 80, 180)),
                GradientStop::new(1.0, Color::from_rgba8(32, 64, 255, 240)),
            ];
            paint.push_rectangle(
                Rectangle::new(LogicalRect { x: 4.0, y: 3.0, width: 40.0, height: 30.0 })
                    .background(Color::from_rgba8(20, 24, 32, 210))
                    .gradient_border(3.0, LinearGradient::new(&stops).angle(135.0))
                    .uniform_radius(9.0),
                screen,
                clip,
            );
        }
        renderer.render(&paint, &[screen]);
        renderer
    }

    let direct = render(Direct::default());
    let scanline = render(Scanline::default());
    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
    assert_eq!(direct.buffer().pixels()[18 * 48 + 24], 0x0010_131a);
}

#[test]
fn rounded_clips_match_between_strategies() {
    static PIXEL: [u8; 3] = [0, 255, 0];
    fn render<S: RenderStrategy<VecBuffer<u32>>>(strategy: S) -> Renderer<VecBuffer<u32>, S> {
        let mut renderer = Renderer::new(VecBuffer::<u32>::new(16, 16), renderer_config()).strategy(strategy);
        let image =
            renderer.create_image(ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgb8, 1, 1));
        let string = renderer.create_string(StringData::Static("M"));
        let screen = renderer.screen();
        let area = LogicalRect { width: 16.0, height: 16.0, ..LogicalRect::default() };
        let red = Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 255));
        let image = ImageRequest {
            image,
            area,
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let text = TextRequest {
            text: string.into(),
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions::default(),
            intrinsic_height: false,
        };

        let mut paint = PaintList::default();
        let outer_clip = paint.push_clip(
            ClipId::default(),
            area,
            BorderRadius { top_left: 8.0, top_right: 8.0, bottom_right: 8.0, bottom_left: 8.0 },
        );
        paint.push_rectangle(red, screen, outer_clip);
        let inner_clip = paint.push_clip(
            outer_clip,
            LogicalRect { width: 8.0, height: 8.0, ..area },
            BorderRadius::default(),
        );
        paint.push_image(image, screen, inner_clip);
        paint.push_text(text, screen, inner_clip);
        paint.push_rectangle(
            Rectangle::new(LogicalRect { x: 15.0, y: 15.0, width: 1.0, height: 1.0 })
                .background(Color::from_rgba8(0, 0, 255, 255)),
            screen,
            ClipId::default(),
        );
        renderer.render(&paint, &[screen]);
        renderer
    }

    let direct = render(Direct::default());
    let scanline = render(Scanline::default());

    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
    assert_eq!(direct.buffer().pixels()[0], 0);
    let edge = direct.buffer().pixels()[6];
    assert!((1..255).contains(&((edge >> 8) & 0xff)));
    let edge = direct.buffer().pixels()[9];
    assert!((1..255).contains(&((edge >> 16) & 0xff)));
    assert_eq!(direct.buffer().pixels()[15 * 16 + 15], 0x0000_00ff);
}

#[test]
fn dropped_image_remains_valid_until_frame_end() {
    static PIXEL: [u8; 4] = [255, 0, 0, 255];
    let mut renderer =
        Renderer::new(VecBuffer::<u32>::new(1, 1), renderer_config()).strategy(Scanline::default());
    let image = renderer.create_image(ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgba8, 1, 1));
    let damage = [PhysicalRect { x: 0, y: 0, width: 1, height: 1 }];
    let mut paint = PaintList::default();
    paint.push_image(
        ImageRequest {
            image,
            area: LogicalRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        },
        damage[0],
        ClipId::default(),
    );
    renderer.drop_image(image);
    renderer.render(&paint, &damage);

    assert_eq!(renderer.buffer().pixels()[0], 0x00ff_0000);
    let image = RendererImageId::from(KeyData::from_ffi(image.0));
    assert!(!renderer.context.images.contains_key(image));
}

#[test]
fn strings_drop_after_frame_end() {
    let mut renderer =
        Renderer::new(VecBuffer::<u32>::new(32, 24), renderer_config()).strategy(Scanline::default());
    let damage = [PhysicalRect { x: 0, y: 0, width: 32, height: 24 }];
    let string = renderer.create_string(StringData::Owned(String::from("M").into_boxed_str()));
    let mut paint = PaintList::default();
    paint.push_text(
        TextRequest {
            text: string.into(),
            area: LogicalRect { x: 0.0, y: 0.0, width: 32.0, height: 24.0 },
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions::default(),
            intrinsic_height: false,
        },
        damage[0],
        ClipId::default(),
    );
    renderer.drop_string(string);
    assert_eq!(renderer.string(string), "M");
    renderer.render(&paint, &damage);

    assert!(renderer.buffer().pixels().iter().any(|pixel| *pixel != 0));
    let string = RendererStringId::from(KeyData::from_ffi(string.0));
    assert!(!renderer.context.strings.contains_key(string));
}

#[test]
fn managed_strings_deref_render_and_drop() {
    let mut runtime = Runtime::new(RuntimePlatform {
        renderer: Renderer::new(VecBuffer::<u32>::new(96, 48), renderer_config())
            .strategy(Scanline::default()),
        repaint_buffer: RepaintBuffer::Reused,
    });
    let owned = runtime.erased_platform().create_string(String::from("managed"));
    let mut static_string = runtime.erased_platform().create_string("static");

    assert_eq!(&*owned, "managed");
    assert_eq!(&*static_string, "static");
    runtime.render(Duration::ZERO, Input::None, |ui| {
        Text::new(&owned)
            .color(Color::WHITE)
            .render(ui, LogicalRect { x: 0.0, y: 0.0, width: 96.0, height: 24.0 });
        Text::new(&static_string)
            .color(Color::WHITE)
            .render(ui, LogicalRect { x: 0.0, y: 24.0, width: 96.0, height: 24.0 });
        Text::new("literal")
            .color(Color::WHITE)
            .render(ui, LogicalRect { x: 48.0, y: 0.0, width: 48.0, height: 24.0 });
    });
    assert!(runtime.platform().renderer.buffer().pixels().iter().any(|pixel| *pixel != 0));

    assert!(runtime
        .platform()
        .renderer
        .context
        .strings
        .values()
        .any(|string| matches!(&string.data, StringData::Static("static"))));
    assert_eq!(runtime.platform().renderer.context.strings.len(), 2);

    let old = static_string.id();
    assert_eq!(static_string.edit().as_str(), "static");
    assert_eq!(static_string.id(), old);
    static_string.edit().push_str(" updated");
    assert_eq!(&*static_string, "static updated");
    assert_ne!(static_string.id(), old);
    assert_eq!(runtime.platform().renderer.context.strings.len(), 3);
    runtime.render(Duration::ZERO, Input::None, |ui| {
        Text::new(&static_string)
            .color(Color::WHITE)
            .render(ui, LogicalRect { x: 0.0, y: 24.0, width: 96.0, height: 24.0 });
    });
    assert_eq!(runtime.platform().renderer.context.strings.len(), 2);

    drop(owned);
    drop(static_string);
    assert_eq!(runtime.platform().renderer.context.strings.len(), 2);
    runtime.render(Duration::ZERO, Input::None, |_| {});
    assert_eq!(runtime.platform().renderer.context.strings.len(), 0);
}
