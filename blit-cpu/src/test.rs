use super::*;
use blit::{
    Color, Easing, ImageData, ImageFormat, ImageId, ImagePixels, Input, KeyboardRequest,
    LogicalPoint, LogicalRect, LogicalSize, PhysicalRect, PlatformImpl, Runtime, TextOptions,
    TextRequest, TextStyle, WidgetId,
    widgets::{
        BorderRadius, BoxShadow, BoxShadowRequest, GradientStop, ImageFit, ImageRequest,
        ImageSampling, LinearGradient, Rectangle,
    },
};
use std::{ops::Range, time::Duration};

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

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self { blue, green, red }
    }
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
}

impl<B: PixelBuffer + 'static, S: RenderStrategy<B> + 'static> PlatformImpl
    for RuntimePlatform<B, S>
{
    fn begin_frame(&mut self) {
        self.renderer.begin_frame()
    }

    fn end_frame(&mut self, damage: &[PhysicalRect]) {
        self.renderer.end_frame(damage)
    }

    fn screen(&mut self) -> PhysicalRect {
        self.renderer.screen()
    }

    fn push_rounded_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.renderer.push_rounded_clip(area, radius)
    }

    fn pop_rounded_clip(&mut self) {
        self.renderer.pop_rounded_clip()
    }

    fn draw_rectangle(&mut self, rectangle: &Rectangle<'_>, clip: PhysicalRect) {
        self.renderer.draw_rectangle(rectangle, clip)
    }

    fn draw_box_shadow(&mut self, shadow: &BoxShadowRequest, clip: PhysicalRect) {
        self.renderer.draw_box_shadow(shadow, clip)
    }

    fn create_image(&mut self, data: ImageData) -> ImageId {
        self.renderer.create_image(data)
    }

    fn drop_image(&mut self, image: ImageId) {
        self.renderer.drop_image(image)
    }

    fn draw_image(&mut self, image: &ImageRequest, clip: PhysicalRect) {
        self.renderer.draw_image(image, clip)
    }

    fn draw_text(&mut self, request: &TextRequest<'_>, clip: PhysicalRect) -> Option<PhysicalRect> {
        self.renderer.draw_text(request, clip)
    }

    fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize {
        self.renderer.text_offset_at_position(request, position)
    }

    fn measure_text(&mut self, request: &TextRequest<'_>) -> LogicalSize {
        self.renderer.measure_text(request)
    }

    fn measure_text_height(&mut self, request: &TextRequest<'_>) -> f32 {
        self.renderer.measure_text_height(request)
    }

    fn text_cursor_rect(&mut self, request: &TextRequest<'_>, byte_offset: usize) -> LogicalRect {
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

    fn swap(&mut self) {
        self.active ^= 1;
    }

    fn pixels(&self) -> &[u32] {
        &self.pixels[self.active]
    }

    fn take_rendered_pixels(&mut self) -> usize {
        std::mem::take(&mut self.rendered_pixels)
    }

    fn replace_inactive(&mut self, pixel: u32) {
        self.pixels[self.active ^ 1].fill(pixel);
    }
}

impl PixelBuffer for SwappedBuffer {
    type Pixel = u32;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

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
            })
            .with_repaint_buffer(blit::RepaintBuffer::Swapped)
        };
        Self {
            partial: make_runtime(),
            full: make_runtime(),
            frame: 0,
            id: WidgetId::new("coherence harness movement"),
        }
    }

    fn render(&mut self, position: f32) -> (usize, usize) {
        self.render_at(
            Duration::from_millis(self.frame as u64),
            position,
            Duration::ZERO,
        )
    }

    fn render_at(&mut self, time: Duration, position: f32, duration: Duration) -> (usize, usize) {
        if self.frame != 0 {
            self.partial.platform().renderer.buffer_mut().swap();
            self.full.platform().renderer.buffer_mut().swap();
        }
        let id = self.id;
        self.partial.render(time, Input::None, |ui| {
            render_coherence_scene(ui, id, position, duration)
        });
        self.full.invalidate_all();
        self.full.render(time, Input::None, |ui| {
            render_coherence_scene(ui, id, position, duration)
        });

        assert_eq!(
            self.partial.platform().renderer.buffer().pixels(),
            self.full.platform().renderer.buffer().pixels(),
            "frame {} at position {position}",
            self.frame
        );
        let partial = self
            .partial
            .platform()
            .renderer
            .buffer_mut()
            .take_rendered_pixels();
        let full = self
            .full
            .platform()
            .renderer
            .buffer_mut()
            .take_rendered_pixels();
        self.frame += 1;
        (partial, full)
    }
}

fn render_coherence_scene(ui: &mut blit::Ui, id: WidgetId, position: f32, duration: Duration) {
    let screen = ui.screen();
    Rectangle::new(screen)
        .background(Color::from_rgba8(24, 36, 48, 255))
        .render(ui);
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
    Rectangle::new(LogicalRect {
        x,
        y: 6.0,
        width: 12.0,
        height: 20.0,
    })
    .background(Color::from_rgba8(20, 20, 20, 160))
    .uniform_radius(4.0)
    .render(&mut movement);
    Rectangle::new(LogicalRect {
        x: x + 4.0,
        y: 10.0,
        width: 4.0,
        height: 12.0,
    })
    .background(if position < screen.width / 2.0 {
        Color::from_rgba8(230, 220, 180, 255)
    } else {
        Color::from_rgba8(180, 210, 240, 255)
    })
    .render(&mut movement);
}

impl PixelBuffer for TrackingBuffer {
    type Pixel = u32;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

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
            font: Font::from_bytes(
                include_bytes!("../../example/assets/Montserrat-Regular.ttf") as &[u8],
                FontSettings::default(),
            )
            .unwrap(),
        }],
        glyph_cache_capacity: 1024 * 1024,
        paragraph_cache_capacity: 1024 * 1024,
        shadow_cache_capacity: 1024 * 1024,
    }
}

#[test]
fn renderer_supports_custom_pixel_layouts() {
    let mut renderer = Renderer::new(VecBuffer::<BgrPixel>::new(32, 24), renderer_config());
    let clip = PhysicalRect {
        x: 0,
        y: 0,
        width: 32,
        height: 24,
    };
    renderer.begin_frame();
    renderer.draw_rectangle(
        &Rectangle::new(LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 24.0,
        })
        .background(Color::from_rgba8(12, 34, 56, 255)),
        clip,
    );
    renderer.end_frame(&[clip]);
    assert_eq!(
        renderer.buffer().pixels()[0],
        BgrPixel {
            blue: 56,
            green: 34,
            red: 12,
        }
    );

    renderer.begin_frame();
    renderer.draw_text(
        &TextRequest {
            text: "M",
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 32.0,
                height: 24.0,
            },
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions::default(),
            intrinsic_height: false,
        },
        clip,
    );
    renderer.end_frame(&[clip]);
    assert!(
        renderer
            .buffer()
            .pixels()
            .iter()
            .any(|pixel| pixel.red > 12)
    );

    let request = TextRequest {
        text: "abc",
        area: LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 24.0,
        },
        offset_x: 0.0,
        color: Color::WHITE,
        style: TextStyle::default(),
        options: TextOptions::default(),
        intrinsic_height: false,
    };
    assert_eq!(
        renderer.text_offset_at_position(&request, LogicalPoint { x: 100.0, y: 12.0 },),
        request.text.len()
    );
    let start = renderer.text_cursor_rect(&request, 0);
    let end = renderer.text_cursor_rect(&request, request.text.len());
    assert!(end.x > start.x);
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

    harness
        .partial
        .platform()
        .renderer
        .buffer_mut()
        .replace_inactive(0x00ff_00ff);
    harness
        .full
        .platform()
        .renderer
        .buffer_mut()
        .replace_inactive(0x00ff_00ff);
    harness.partial.invalidate_all();
    harness.render(4.0);
    harness.render(4.0);
    assert!(!harness.partial.has_pending_redraw());

    let mut random = 0x4d59_5df4_d0f3_3173_u64;
    let mut position = 4.0;
    for _ in 0..256 {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
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
    assert!(
        partial_pixels * 4 < full_pixels,
        "partial={partial_pixels}, full={full_pixels}"
    );
}

#[test]
fn dropped_image_slots_are_reused_after_end_frame() {
    static PIXEL: [u8; 4] = [255, 255, 255, 255];
    let mut renderer = Renderer::new(VecBuffer::<u32>::new(1, 1), renderer_config());
    let texture = ImageData::new(
        blit::ImagePixels::Static(&PIXEL),
        blit::ImageFormat::Rgba8,
        1,
        1,
    );

    let first = renderer.create_image(texture);
    renderer.drop_image(first);
    let first_key = RendererImageId::from(KeyData::from_ffi(first.0));
    assert!(renderer.context.images.contains_key(first_key));

    renderer.end_frame(&[]);
    assert!(!renderer.context.images.contains_key(first_key));

    let second = renderer.create_image(ImageData::new(
        blit::ImagePixels::Static(&PIXEL),
        blit::ImageFormat::Rgba8,
        1,
        1,
    ));
    assert_ne!(second, first);
}

#[test]
fn frame_is_rendered_once_per_affected_line_in_order() {
    let mut renderer = Renderer::new(
        TrackingBuffer {
            pixels: vec![0; 16],
            lines: Vec::new(),
            ranges: Vec::new(),
            width: 4,
            height: 4,
        },
        renderer_config(),
    )
    .strategy(Scanline::default());
    let damage = [
        PhysicalRect {
            x: 0,
            y: 2,
            width: 4,
            height: 1,
        },
        PhysicalRect {
            x: 0,
            y: 0,
            width: 4,
            height: 1,
        },
    ];
    renderer.begin_frame();
    renderer.draw_rectangle(
        &Rectangle::new(LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
        })
        .background(Color::WHITE),
        PhysicalRect {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        },
    );
    renderer.end_frame(&damage);

    assert_eq!(renderer.buffer().lines, [0, 2]);
    assert_eq!(renderer.buffer().ranges, [0..4, 0..4]);
}

#[test]
fn scanline_only_borrows_dirty_horizontal_ranges() {
    let mut renderer = Renderer::new(
        TrackingBuffer {
            pixels: vec![0; 8],
            lines: Vec::new(),
            ranges: Vec::new(),
            width: 4,
            height: 2,
        },
        renderer_config(),
    )
    .strategy(Scanline::default());
    let damage = [PhysicalRect {
        x: 1,
        y: 0,
        width: 2,
        height: 1,
    }];
    renderer.begin_frame();
    renderer.draw_rectangle(
        &Rectangle::new(LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 1.0,
        })
        .background(Color::WHITE),
        PhysicalRect {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        },
    );
    renderer.end_frame(&damage);

    assert_eq!(renderer.buffer().ranges.len(), 1);
    assert_eq!(renderer.buffer().ranges[0], 1..3);
    assert_eq!(
        renderer.buffer().pixels,
        [0, 0xffffff, 0xffffff, 0, 0, 0, 0, 0]
    );
}

#[test]
fn scanline_skips_commands_behind_opaque_content() {
    static RECTANGLE_PIXELS: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);

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
            Self {
                color: <u32 as Pixel>::from_rgb(red, green, blue),
                draws: 0,
            }
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

    let mut renderer = Renderer::new(VecBuffer::<CountingPixel>::new(4, 2), renderer_config())
        .strategy(Scanline::default());
    let screen = renderer.screen();
    let area = LogicalRect {
        width: 4.0,
        height: 2.0,
        ..LogicalRect::default()
    };
    renderer.begin_frame();
    renderer.draw_rectangle(
        &Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
    );
    renderer.draw_rectangle(
        &Rectangle::new(area).background(Color::from_rgba8(0, 255, 0, 255)),
        screen,
    );
    renderer.draw_rectangle(
        &Rectangle::new(area).background(Color::from_rgba8(0, 0, 255, 128)),
        screen,
    );
    renderer.end_frame(&[screen]);

    assert!(
        renderer
            .buffer()
            .pixels()
            .iter()
            .all(|pixel| pixel.draws == 2)
    );

    let mut renderer = Renderer::new(VecBuffer::<CountingPixel>::new(8, 7), renderer_config())
        .strategy(Scanline::default());
    let screen = renderer.screen();
    let damage = PhysicalRect {
        y: 3,
        height: 1,
        ..screen
    };
    let area = LogicalRect {
        width: 8.0,
        height: 7.0,
        ..LogicalRect::default()
    };
    renderer.begin_frame();
    renderer.draw_rectangle(
        &Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
    );
    renderer.draw_rectangle(
        &Rectangle::new(area)
            .background(Color::from_rgba8(0, 255, 0, 255))
            .uniform_radius(3.0),
        screen,
    );
    renderer.draw_rectangle(
        &Rectangle::new(area).background(Color::from_rgba8(0, 0, 255, 128)),
        screen,
    );
    renderer.end_frame(&[damage]);

    assert!(
        renderer.buffer().pixels()[3 * 8..4 * 8]
            .iter()
            .all(|pixel| pixel.draws == 2)
    );

    static IMAGE_PIXEL: [u8; 4] = [0, 255, 0, 255];
    let mut renderer = Renderer::new(VecBuffer::<CountingPixel>::new(4, 2), renderer_config())
        .strategy(Scanline::default());
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&IMAGE_PIXEL),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    let screen = renderer.screen();
    let area = LogicalRect {
        width: 4.0,
        height: 2.0,
        ..LogicalRect::default()
    };
    let image = ImageRequest {
        image,
        area,
        fit: ImageFit::Fill,
        sampling: ImageSampling::Nearest,
        opacity: 1.0,
        colorize: None,
        nine_slice: None,
        horizontal_tiling: blit::widgets::ImageTiling::None,
        vertical_tiling: blit::widgets::ImageTiling::None,
    };
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
    renderer.begin_frame();
    renderer.draw_rectangle(
        &Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
    );
    renderer.draw_image(&image, screen);
    renderer.draw_rectangle(
        &Rectangle::new(area).background(Color::from_rgba8(0, 0, 255, 128)),
        screen,
    );
    renderer.end_frame(&[screen]);

    assert_eq!(
        RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed),
        8
    );

    static TRANSPARENT_IMAGE_PIXEL: [u8; 4] = [0, 255, 0, 254];
    let transparent_image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&TRANSPARENT_IMAGE_PIXEL),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    let transparent_image = ImageRequest {
        image: transparent_image,
        ..image
    };
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
    renderer.begin_frame();
    renderer.draw_rectangle(
        &Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
    );
    renderer.draw_image(&transparent_image, screen);
    renderer.draw_rectangle(
        &Rectangle::new(area).background(Color::from_rgba8(0, 0, 255, 128)),
        screen,
    );
    renderer.end_frame(&[screen]);

    assert_eq!(
        RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed),
        16
    );
}

#[test]
fn cached_dirty_ranges_match_direct_rendering() {
    let mut direct = Renderer::new(VecBuffer::<u32>::new(8, 8), renderer_config());
    let mut scanline =
        Renderer::new(VecBuffer::<u32>::new(8, 8), renderer_config()).strategy(Scanline::default());
    let red = Rectangle::new(LogicalRect {
        x: 0.0,
        y: 0.0,
        width: 8.0,
        height: 8.0,
    })
    .background(Color::from_rgba8(255, 0, 0, 128));
    let green = Rectangle::new(red.area).background(Color::from_rgba8(0, 255, 0, 128));
    let red_clips = [
        PhysicalRect {
            x: 0,
            y: 0,
            width: 3,
            height: 3,
        },
        PhysicalRect {
            x: 5,
            y: 5,
            width: 3,
            height: 3,
        },
    ];
    let green_clips = [
        PhysicalRect {
            x: 5,
            y: 0,
            width: 3,
            height: 3,
        },
        PhysicalRect {
            x: 0,
            y: 5,
            width: 3,
            height: 3,
        },
    ];
    let damage = [red_clips[0], red_clips[1], green_clips[0], green_clips[1]];
    let clip = PhysicalRect {
        x: 0,
        y: 0,
        width: 8,
        height: 8,
    };
    direct.begin_frame();
    scanline.begin_frame();

    direct.draw_rectangle(&red, clip);
    direct.draw_rectangle(&green, clip);
    direct.end_frame(&damage);
    scanline.draw_rectangle(&red, clip);
    scanline.draw_rectangle(&green, clip);
    scanline.end_frame(&damage);

    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
}

#[test]
fn box_shadows_match_between_strategies_and_reuse_masks() {
    fn render<S: RenderStrategy<VecBuffer<u32>>>(strategy: S) -> Renderer<VecBuffer<u32>, S> {
        let mut renderer = Renderer::new(VecBuffer::<u32>::new(128, 96), renderer_config())
            .with_scale_factor(2.0)
            .strategy(strategy);
        let screen = renderer.screen();
        let first = BoxShadowRequest {
            area: LogicalRect {
                x: 12.0,
                y: 10.0,
                width: 36.0,
                height: 24.0,
            },
            radius: BorderRadius {
                top_left: 6.0,
                top_right: 6.0,
                bottom_right: 6.0,
                bottom_left: 6.0,
            },
            shadow: BoxShadow::new(Color::from_rgba8(220, 40, 20, 180))
                .offset(2.0, 3.0)
                .blur(5.0)
                .spread(1.0),
        };
        let second = BoxShadowRequest {
            area: LogicalRect {
                x: 4.0,
                y: 24.0,
                width: 52.0,
                height: 20.0,
            },
            shadow: BoxShadow::new(Color::from_rgba8(20, 80, 220, 140))
                .offset(2.0, 3.0)
                .blur(5.0)
                .spread(1.0),
            ..first
        };
        renderer.begin_frame();
        renderer.draw_box_shadow(&first, screen);
        renderer.draw_box_shadow(&second, screen);
        renderer.end_frame(&[screen]);
        assert_eq!(renderer.context.images.len(), 1);
        renderer
    }

    let direct = render(Direct);
    let scanline = render(Scanline::default());
    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
    assert!(direct.buffer().pixels().iter().any(|pixel| *pixel != 0));
}

#[test]
fn gradient_borders_match_between_strategies_and_rounded_clips() {
    fn render<S: RenderStrategy<VecBuffer<u32>>>(strategy: S) -> Renderer<VecBuffer<u32>, S> {
        let mut renderer =
            Renderer::new(VecBuffer::<u32>::new(48, 36), renderer_config()).strategy(strategy);
        let screen = renderer.screen();
        renderer.begin_frame();
        renderer.push_rounded_clip(
            LogicalRect {
                x: 2.0,
                y: 2.0,
                width: 44.0,
                height: 32.0,
            },
            BorderRadius {
                top_left: 10.0,
                top_right: 10.0,
                bottom_right: 10.0,
                bottom_left: 10.0,
            },
        );
        {
            let stops = [
                GradientStop::new(0.0, Color::from_rgba8(255, 32, 16, 220)),
                GradientStop::new(0.4, Color::from_rgba8(40, 240, 80, 180)),
                GradientStop::new(1.0, Color::from_rgba8(32, 64, 255, 240)),
            ];
            renderer.draw_rectangle(
                &Rectangle::new(LogicalRect {
                    x: 4.0,
                    y: 3.0,
                    width: 40.0,
                    height: 30.0,
                })
                .background(Color::from_rgba8(20, 24, 32, 210))
                .gradient_border(3.0, LinearGradient::new(&stops).angle(135.0))
                .uniform_radius(9.0),
                screen,
            );
        }
        renderer.pop_rounded_clip();
        renderer.end_frame(&[screen]);
        renderer
    }

    let direct = render(Direct);
    let scanline = render(Scanline::default());
    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
    assert_eq!(direct.buffer().pixels()[18 * 48 + 24], 0x0010_131a);
}

#[test]
fn rounded_clips_match_between_strategies() {
    static PIXEL: [u8; 3] = [0, 255, 0];
    fn render<S: RenderStrategy<VecBuffer<u32>>>(strategy: S) -> Renderer<VecBuffer<u32>, S> {
        let mut renderer =
            Renderer::new(VecBuffer::<u32>::new(16, 16), renderer_config()).strategy(strategy);
        let image = renderer.create_image(ImageData::new(
            ImagePixels::Static(&PIXEL),
            ImageFormat::Rgb8,
            1,
            1,
        ));
        let screen = renderer.screen();
        let area = LogicalRect {
            width: 16.0,
            height: 16.0,
            ..LogicalRect::default()
        };
        let red = Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 255));
        let image = ImageRequest {
            image,
            area,
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: blit::widgets::ImageTiling::None,
            vertical_tiling: blit::widgets::ImageTiling::None,
        };
        let text = TextRequest {
            text: "M",
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions::default(),
            intrinsic_height: false,
        };

        renderer.begin_frame();
        renderer.push_rounded_clip(
            area,
            BorderRadius {
                top_left: 8.0,
                top_right: 8.0,
                bottom_right: 8.0,
                bottom_left: 8.0,
            },
        );
        renderer.draw_rectangle(&red, screen);
        renderer.push_rounded_clip(
            LogicalRect {
                width: 8.0,
                height: 8.0,
                ..area
            },
            BorderRadius::default(),
        );
        renderer.draw_image(&image, screen);
        renderer.draw_text(&text, screen);
        renderer.pop_rounded_clip();
        renderer.pop_rounded_clip();
        renderer.draw_rectangle(
            &Rectangle::new(LogicalRect {
                x: 15.0,
                y: 15.0,
                width: 1.0,
                height: 1.0,
            })
            .background(Color::from_rgba8(0, 0, 255, 255)),
            screen,
        );
        renderer.end_frame(&[screen]);
        renderer
    }

    let direct = render(Direct);
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
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&PIXEL),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    let damage = [PhysicalRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    }];
    renderer.begin_frame();
    renderer.draw_image(
        &ImageRequest {
            image,
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: blit::widgets::ImageTiling::None,
            vertical_tiling: blit::widgets::ImageTiling::None,
        },
        damage[0],
    );
    renderer.drop_image(image);
    renderer.end_frame(&damage);

    assert_eq!(renderer.buffer().pixels()[0], 0x00ff_0000);
    let image = RendererImageId::from(KeyData::from_ffi(image.0));
    assert!(!renderer.context.images.contains_key(image));
}

#[test]
fn text_source_can_drop_before_frame_end() {
    let mut renderer = Renderer::new(VecBuffer::<u32>::new(32, 24), renderer_config())
        .strategy(Scanline::default());
    let damage = [PhysicalRect {
        x: 0,
        y: 0,
        width: 32,
        height: 24,
    }];
    renderer.begin_frame();
    {
        let text = String::from("M");
        renderer.draw_text(
            &TextRequest {
                text: &text,
                area: LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: 32.0,
                    height: 24.0,
                },
                offset_x: 0.0,
                color: Color::WHITE,
                style: TextStyle::default(),
                options: TextOptions::default(),
                intrinsic_height: false,
            },
            damage[0],
        );
    }
    renderer.end_frame(&damage);

    assert!(renderer.buffer().pixels().iter().any(|pixel| *pixel != 0));
}
