use std::{ops::Range, time::Duration};

use blit::{
    RepaintBuffer, Ui, UiState,
    animation::Easing,
    color::Color,
    command_list::{BoxShadow, ClipId, CommandList, Rectangle},
    container::Sizing,
    geometry::{LogicalPoint, LogicalRect, PhysicalRect},
    image::{
        ImageData, ImageFit, ImageFormat, ImagePixels, ImageRequest, ImageSampling, ImageTiling,
    },
    input::Input,
    interact::WidgetId,
    layout::Flex,
    renderer::Renderer as _,
    style::{BorderRadius, Clip, GradientStop, LinearGradient, Shadow, Style},
    text::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextStyle, TextWrap},
    widget::{Image as ImageWidget, Rectangle as RectangleWidget, Text, Widget},
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

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self { blue, green, red }
    }
}

struct TrackingBuffer {
    pixels: Vec<Xrgb8888>,
    lines: Vec<usize>,
    ranges: Vec<Range<usize>>,
    width: usize,
    height: usize,
}

struct Harness<B: PixelBuffer, S: RenderStrategy<B>> {
    renderer: Renderer<B, S>,
    state: UiState,
}

impl<B: PixelBuffer + 'static, S: RenderStrategy<B> + 'static> Harness<B, S> {
    fn new(renderer: Renderer<B, S>, repaint_buffer: RepaintBuffer) -> Self {
        let state = UiState::new(renderer.screen(), repaint_buffer, 1.0);
        Self { renderer, state }
    }

    fn render<R>(&mut self, time: Duration, input: Input, build: impl FnMut(&mut Ui) -> R) -> R {
        blit::render(&mut self.renderer, &mut self.state, time, [input], build)
    }

    fn renderer(&mut self) -> &mut Renderer<B, S> {
        &mut self.renderer
    }

    fn invalidate_all(&mut self) {
        self.state.invalidate_all();
    }

    fn has_pending_redraw(&self) -> bool {
        self.state.has_pending_redraw()
    }
}

struct SwappedBuffer {
    pixels: [Vec<Xrgb8888>; 2],
    active: usize,
    width: usize,
    height: usize,
    rendered_pixels: usize,
}

impl SwappedBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: std::array::from_fn(|_| vec![Xrgb8888::default(); width * height]),
            active: 0,
            width,
            height,
            rendered_pixels: 0,
        }
    }

    fn swap(&mut self) {
        self.active ^= 1;
    }

    fn pixels(&self) -> &[Xrgb8888] {
        &self.pixels[self.active]
    }

    fn take_rendered_pixels(&mut self) -> usize {
        std::mem::take(&mut self.rendered_pixels)
    }

    fn replace_inactive(&mut self, pixel: Xrgb8888) {
        self.pixels[self.active ^ 1].fill(pixel);
    }
}

impl PixelBuffer for SwappedBuffer {
    type Pixel = Xrgb8888;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn line_mut(&mut self, line: usize) -> &mut [Xrgb8888] {
        let start = line * self.width;
        &mut self.pixels[self.active][start..start + self.width]
    }

    fn process_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        process: impl FnOnce(&mut [Xrgb8888]),
    ) {
        self.rendered_pixels += range.len();
        let start = line * self.width;
        process(&mut self.pixels[self.active][start + range.start..start + range.end]);
    }
}

struct CoherenceHarness {
    partial: Harness<SwappedBuffer, Scanline>,
    full: Harness<SwappedBuffer, Scanline>,
    frame: usize,
    id: WidgetId,
}

impl CoherenceHarness {
    fn new(width: usize, height: usize) -> Self {
        let make_harness = || {
            Harness::new(
                Renderer::new(SwappedBuffer::new(width, height), renderer_config())
                    .strategy(Scanline::default()),
                RepaintBuffer::Swapped,
            )
        };
        Self {
            partial: make_harness(),
            full: make_harness(),
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
            self.partial.renderer().buffer_mut().swap();
            self.full.renderer().buffer_mut().swap();
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
            self.partial.renderer().buffer().pixels(),
            self.full.renderer().buffer().pixels(),
            "frame {} at position {position}",
            self.frame
        );
        let partial = self.partial.renderer().buffer_mut().take_rendered_pixels();
        let full = self.full.renderer().buffer_mut().take_rendered_pixels();
        self.frame += 1;
        (partial, full)
    }
}

fn render_coherence_scene(ui: &mut blit::Ui, id: WidgetId, position: f32, duration: Duration) {
    let screen_width = ui.screen().width;
    let x = ui.animate(id, position, duration, Easing::Linear);
    let mut scene = ui
        .layout(Flex::column())
        .grow()
        .background(Color::from_rgba8(24, 36, 48, 255))
        .open();
    scene.add(|ui: &mut Ui| {
        let mut row = ui
            .layout(Flex::row())
            .width(Sizing::grow())
            .height(Sizing::fixed(20.0))
            .open();
        row.add(RectangleWidget::new().width(Sizing::fixed(x)));
        row.add(|ui: &mut Ui| {
            let mut movement = ui
                .layout(Flex::column().padding(blit::geometry::LogicalInsets::uniform(4.0)))
                .fixed(12.0, 20.0)
                .background(Color::from_rgba8(20, 20, 20, 160))
                .uniform_radius(4.0)
                .open();
            movement.add(
                RectangleWidget::new()
                    .width(Sizing::grow())
                    .height(Sizing::grow())
                    .background(if position < screen_width / 2.0 {
                        Color::from_rgba8(230, 220, 180, 255)
                    } else {
                        Color::from_rgba8(180, 210, 240, 255)
                    }),
            );
        });
    });
}

impl PixelBuffer for TrackingBuffer {
    type Pixel = Xrgb8888;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn line_mut(&mut self, line: usize) -> &mut [Xrgb8888] {
        let start = line * self.width;
        &mut self.pixels[start..start + self.width]
    }

    fn process_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        process: impl FnOnce(&mut [Xrgb8888]),
    ) {
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
            font: Font::from_static(include_bytes!(env!("BLIT_TEST_FONT"))).unwrap(),
        }],
        font_metric_cache_capacity: 256,
        glyph_cache_capacity: 1024 * 1024,
        paragraph_cache_capacity: 1024 * 1024,
        shadow_cache_capacity: 1024 * 1024,
    }
}

#[test]
fn resolved_nodes_match_direct_commands() {
    let renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(32, 24), renderer_config())
        .strategy(Scanline::default());
    let mut harness = Harness::new(renderer, RepaintBuffer::Reused);
    let image = harness.renderer().create_image(ImageData::new(
        ImagePixels::Owned([220, 80, 40].repeat(8 * 8).into_boxed_slice()),
        ImageFormat::Rgb8,
        8,
        8,
    ));
    let stops = [
        GradientStop::new(0.0, Color::WHITE),
        GradientStop::new(1.0, Color::BLACK),
    ];
    let radius = BorderRadius {
        top_left: 4.0,
        top_right: 4.0,
        bottom_right: 4.0,
        bottom_left: 4.0,
    };
    let screen = LogicalRect {
        x: 0.0,
        y: 0.0,
        width: 32.0,
        height: 24.0,
    };
    let shadow = Shadow::new(Color::from_rgba8(20, 30, 40, 128))
        .offset(1.0, 1.0)
        .blur(1.0);
    let inset_shadow = Shadow::new(Color::from_rgba8(10, 20, 30, 96))
        .offset(0.0, 1.0)
        .blur(2.0);

    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut panel = ui
            .layout(Flex::column())
            .grow()
            .style(
                Style::new()
                    .background(Color::from_rgba8(40, 70, 100, 255))
                    .gradient_border(2.0, LinearGradient::new(&stops).angle(35.0))
                    .radius(radius)
                    .shadow(Some(shadow))
                    .inset_shadow(Some(inset_shadow)),
            )
            .clip(Clip::Rounded(radius))
            .open();
        panel.add(
            ImageWidget::new(&image)
                .width(Sizing::fixed(8.0))
                .height(Sizing::fixed(8.0))
                .fit(ImageFit::Fill)
                .sampling(ImageSampling::Nearest),
        );
        panel.add(
            Text::new("M")
                .width(Sizing::fixed(24.0))
                .height(Sizing::fixed(8.0))
                .color(Color::WHITE),
        );
    });

    let mut direct = Renderer::new(VecBuffer::<Xrgb8888>::new(32, 24), renderer_config())
        .strategy(Scanline::default());
    let direct_image = direct.create_image(ImageData::new(
        ImagePixels::Owned([220, 80, 40].repeat(8 * 8).into_boxed_slice()),
        ImageFormat::Rgb8,
        8,
        8,
    ));
    let mut commands = CommandList::default();
    let box_shadow = BoxShadow::new(screen, Color::from_rgba8(20, 30, 40, 128))
        .radius(radius)
        .offset(1.0, 1.0)
        .blur(1.0);
    commands.push_box_shadow(
        box_shadow,
        box_shadow.bounds().to_physical(1.0),
        ClipId::default(),
    );
    commands.push_rectangle(
        Rectangle::new(screen)
            .background(Color::from_rgba8(40, 70, 100, 255))
            .gradient_border(2.0, LinearGradient::new(&stops).angle(35.0))
            .radius(radius),
        screen.to_physical(1.0),
        ClipId::default(),
    );
    let inset_shadow = BoxShadow::new(screen, inset_shadow.color)
        .radius(radius)
        .offset(inset_shadow.offset_x, inset_shadow.offset_y)
        .blur(inset_shadow.blur)
        .inset(true);
    commands.push_box_shadow(
        inset_shadow,
        inset_shadow.bounds().to_physical(1.0),
        ClipId::default(),
    );
    let clip = commands.push_clip(ClipId::default(), screen, radius);
    let image_area = LogicalRect {
        width: 8.0,
        height: 8.0,
        ..LogicalRect::default()
    };
    commands.push_image(
        ImageRequest {
            image: direct_image.id(),
            area: image_area,
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        },
        image_area.to_physical(1.0),
        clip,
    );
    let text_area = LogicalRect {
        y: 8.0,
        width: 24.0,
        height: 8.0,
        ..LogicalRect::default()
    };
    let text = direct.text_run("M", TextStyle::default());
    commands.push_text(
        TextRequest {
            text,
            area: text_area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions::default(),
        },
        text_area.to_physical(1.0),
        clip,
    );
    direct.render(&commands, &[screen.to_physical(1.0)]);

    assert_eq!(
        harness.renderer().buffer().pixels(),
        direct.buffer().pixels()
    );
}

#[test]
fn renderer_supports_custom_pixel_layouts() {
    let mut renderer = Renderer::new(VecBuffer::<BgrPixel>::new(32, 24), renderer_config());
    let m = renderer.text_run("M", TextStyle::default());
    let clip = PhysicalRect {
        x: 0,
        y: 0,
        width: 32,
        height: 24,
    };
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 24.0,
        })
        .background(Color::from_rgba8(12, 34, 56, 255)),
        clip,
        ClipId::default(),
    );
    renderer.render(&paint, &[clip]);
    assert_eq!(
        renderer.buffer().pixels()[0],
        BgrPixel {
            blue: 56,
            green: 34,
            red: 12
        }
    );

    paint.clear();
    paint.push_text(
        TextRequest {
            text: m,
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
        },
        clip,
        ClipId::default(),
    );
    renderer.render(&paint, &[clip]);
    assert!(
        renderer
            .buffer()
            .pixels()
            .iter()
            .any(|pixel| pixel.red > 12)
    );

    let request = TextRequest {
        text: renderer.text_run("abc", TextStyle::default()),
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
    };
    assert_eq!(
        renderer.text_offset_at_position(&request, LogicalPoint { x: 100.0, y: 12.0 },),
        "abc".len()
    );
    let start = renderer.text_cursor_rect(&request, 0);
    let end = renderer.text_cursor_rect(&request, "abc".len());
    assert!(end.x > start.x);
}

#[test]
fn text_measurement_reports_wrapped_layout_size() {
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(32, 24), renderer_config());
    let request = TextLayoutRequest {
        text: renderer.text_run("hello world", TextStyle::default()),
        style: TextStyle::default(),
        wrap: TextWrap::None,
        max_width: None,
        max_lines: None,
    };
    let unwrapped = renderer.measure_text(&request);
    let wrapped = renderer.measure_text(&TextLayoutRequest {
        wrap: TextWrap::Word,
        max_width: Some(unwrapped.width / 2.0),
        ..request
    });

    assert!(wrapped.width < unwrapped.width);
    assert!(wrapped.height > unwrapped.height);
}

#[test]
fn clear_resets_stale_pixels_before_drawing() {
    fn render<S: RenderStrategy<VecBuffer<Argb8888>>>(
        strategy: S,
        clear: bool,
        stale: Argb8888,
    ) -> Vec<Argb8888> {
        let mut renderer =
            Renderer::new(VecBuffer::<Argb8888>::new(12, 10), renderer_config()).strategy(strategy);
        renderer.buffer_mut().pixels_mut().fill(stale);
        let screen = renderer.screen();
        let rectangle = Rectangle::new(LogicalRect {
            width: 12.0,
            height: 10.0,
            ..LogicalRect::default()
        })
        .background(Color::from_rgba8(40, 120, 220, 144))
        .solid_border(2.0, Color::from_rgba8(240, 80, 30, 192))
        .uniform_radius(5.0);
        let mut paint = CommandList::default();
        if clear {
            paint.push_clear(screen);
        }
        paint.push_rectangle(rectangle, screen, ClipId::default());
        renderer.render(&paint, &[screen]);
        renderer.buffer().pixels().to_vec()
    }

    let transparent = Argb8888::default();
    let stale = Argb8888::from_raw(0xff32_6496);
    let expected = render(Direct::default(), false, transparent);
    let direct = render(Direct::default(), true, stale);
    let scanline = render(Scanline::default(), true, stale);

    assert_eq!(direct, expected);
    assert_eq!(scanline, expected);
    assert_eq!(expected[0], transparent);
    assert_ne!(expected[12 / 2 + 10 / 2 * 12], transparent);

    let mut renderer = Renderer::new(VecBuffer::<Argb8888>::new(12, 10), renderer_config());
    renderer.buffer_mut().pixels_mut().fill(stale);
    let screen = renderer.screen();
    let mut paint = CommandList::default();
    paint.push_clear(screen);
    renderer.render(&paint, &[screen]);
    assert!(
        renderer
            .buffer()
            .pixels()
            .iter()
            .all(|pixel| *pixel == transparent)
    );
}

#[test]
fn commands_outside_damage_are_not_prepared() {
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(8, 4), renderer_config());
    let damaged = LogicalRect {
        x: 0.0,
        y: 0.0,
        width: 2.0,
        height: 2.0,
    };
    let outside = LogicalRect {
        x: 4.0,
        y: 0.0,
        width: 4.0,
        height: 4.0,
    };
    let mut paint = CommandList::default();
    paint.push_text(
        TextRequest {
            text: TextRunId(u64::MAX),
            area: outside,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions::default(),
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
            0xffffff, 0xffffff, 0, 0, 0, 0, 0, 0, 0xffffff, 0xffffff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
        .map(Xrgb8888::from_raw)
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

    let stale = Xrgb8888::from_raw(0x00ff_00ff);
    harness
        .partial
        .renderer()
        .buffer_mut()
        .replace_inactive(stale);
    harness.full.renderer().buffer_mut().replace_inactive(stale);
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
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config());
    let texture = ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgba8, 1, 1);

    let first = renderer.create_image(texture);
    let retained = first.clone();
    let first_id = first.id();
    drop(first);
    let first_key = RendererImageId::from(KeyData::from_ffi(first_id.0));
    renderer.render(&CommandList::default(), &[]);
    assert!(renderer.context.images.contains_key(first_key));

    drop(retained);
    renderer.render(&CommandList::default(), &[]);
    assert!(!renderer.context.images.contains_key(first_key));

    let second = renderer.create_image(ImageData::new(
        ImagePixels::Static(&PIXEL),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    assert_ne!(second.id(), first_id);
}

#[test]
fn image_alpha_rows_are_cached_and_used() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static BLENDED: AtomicUsize = AtomicUsize::new(0);
    static COPIED: AtomicUsize = AtomicUsize::new(0);

    #[derive(Clone, Copy)]
    struct TrackingPixel;

    impl Pixel for TrackingPixel {
        fn blend_translucent(&mut self, _color: PremultipliedRgbaColor) {
            unreachable!()
        }

        fn from_rgb(_red: u8, _green: u8, _blue: u8) -> Self {
            Self
        }

        fn blend_texture_slice_rgba(
            pixels: &mut [Self],
            source: &[PremultipliedRgbaColor],
            _opacity: u8,
        ) {
            BLENDED.fetch_add(pixels.len().min(source.len()), Ordering::Relaxed);
        }

        fn copy_texture_slice_rgba(pixels: &mut [Self], source: &[PremultipliedRgbaColor]) {
            COPIED.fetch_add(pixels.len().min(source.len()), Ordering::Relaxed);
        }

        fn blend_texture_slice_alpha(pixels: &mut [Self], _color: Color, alpha: &[u8]) {
            BLENDED.fetch_add(pixels.len().min(alpha.len()), Ordering::Relaxed);
        }
    }

    let alpha = [
        0, 255, 255, 255, 0, 0, 0, 255, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 0, 0,
        255, 255,
    ];
    let mut pixels = [0; 6 * 4 * 4];
    for (pixel, alpha) in pixels.chunks_exact_mut(4).zip(alpha) {
        pixel.copy_from_slice(&[alpha / 2, alpha / 4, alpha / 8, alpha]);
    }
    let mut renderer = Renderer::new(VecBuffer::<TrackingPixel>::new(6, 4), renderer_config())
        .strategy(Scanline::default());
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Owned(pixels.into()),
        ImageFormat::Rgba8Premultiplied,
        6,
        4,
    ));
    let key = RendererImageId::from(KeyData::from_ffi(image.id().0));
    let rows = &renderer.context.images[key].alpha_rows;
    let rows: [_; 4] =
        std::array::from_fn(|index| rows.get(ImageFormat::Rgba8Premultiplied, index).unwrap());
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([4, 4, 6, 6]));
    assert!(rows.iter().map(|row| row.opaque_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.opaque_end).eq([4, 4, 2, 2]));
    assert!(renderer.context.images[key].has_opaque_spans);
    assert!(!renderer.context.images[key].opaque);

    let screen = renderer.screen();
    let request = ImageRequest {
        image: image.id(),
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
    let mut paint = CommandList::default();
    paint.push_image(request, screen, ClipId::default());
    renderer.render(&paint, &[screen]);
    assert_eq!(COPIED.load(Ordering::Relaxed), 10);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 8);

    BLENDED.store(0, Ordering::Relaxed);
    COPIED.store(0, Ordering::Relaxed);
    paint.clear();
    paint.push_image(
        ImageRequest {
            opacity: 0.5,
            ..request
        },
        screen,
        ClipId::default(),
    );
    renderer.render(&paint, &[screen]);
    assert_eq!(COPIED.load(Ordering::Relaxed), 0);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 18);

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Owned(
            [
                0, 64, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 2, 0, 0, 255, 255, 255, 255, 255,
                255,
            ]
            .into(),
        ),
        ImageFormat::Alpha8(Color::WHITE),
        6,
        4,
    ));
    let key = RendererImageId::from(KeyData::from_ffi(image.id().0));
    let rows = &renderer.context.images[key].alpha_rows;
    let rows: [_; 4] =
        std::array::from_fn(|index| rows.get(ImageFormat::Alpha8(Color::WHITE), index).unwrap());
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 0, 1, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([3, 0, 4, 6]));
    assert!(
        rows.iter()
            .all(|row| row.opaque_start == 0 && row.opaque_end == 0)
    );
    BLENDED.store(0, Ordering::Relaxed);
    paint.clear();
    paint.push_image(
        ImageRequest {
            image: image.id(),
            opacity: 1.0,
            ..request
        },
        screen,
        ClipId::default(),
    );
    renderer.render(&paint, &[screen]);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 11);

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[255; 6 * 4]),
        ImageFormat::Alpha8(Color::WHITE),
        6,
        4,
    ));
    let key = RendererImageId::from(KeyData::from_ffi(image.id().0));
    let image = &renderer.context.images[key];
    assert!(image.opaque);
    assert!(
        image
            .alpha_rows
            .get(ImageFormat::Alpha8(Color::WHITE), 0)
            .is_none()
    );

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[255; 6 * 4 * 4]),
        ImageFormat::Rgba8Premultiplied,
        6,
        4,
    ));
    let key = RendererImageId::from(KeyData::from_ffi(image.id().0));
    let image = &renderer.context.images[key];
    assert!(image.opaque);
    assert!(
        image
            .alpha_rows
            .get(ImageFormat::Rgba8Premultiplied, 0)
            .is_none()
    );
}

#[test]
fn direct_preserves_exact_overlapping_damage() {
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(4, 4), renderer_config());
    let screen = renderer.screen();
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(screen.to_logical(1.0)).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(
        &paint,
        &[
            PhysicalRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            PhysicalRect {
                x: 0,
                y: 2,
                width: 3,
                height: 2,
            },
            PhysicalRect {
                x: 2,
                y: 0,
                width: 2,
                height: 3,
            },
        ],
    );

    let pixels = renderer.buffer().pixels();
    assert_ne!(pixels[0].raw(), 0);
    let painted = pixels[0];
    let unpainted = Xrgb8888::default();
    assert_eq!(
        pixels,
        [
            painted, unpainted, painted, painted, unpainted, unpainted, painted, painted, painted,
            painted, painted, painted, painted, painted, painted, unpainted,
        ]
    );
}

#[test]
fn direct_does_not_merge_touching_damage() {
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(3, 3), renderer_config());
    let screen = renderer.screen();
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(screen.to_logical(1.0)).background(Color::WHITE),
        screen,
        ClipId::default(),
    );
    renderer.render(
        &paint,
        &[
            PhysicalRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            PhysicalRect {
                x: 2,
                y: 1,
                width: 1,
                height: 2,
            },
        ],
    );

    assert_eq!(
        renderer.buffer().pixels(),
        [0xffffff, 0xffffff, 0, 0, 0, 0xffffff, 0, 0, 0xffffff].map(Xrgb8888::from_raw)
    );
}

#[test]
fn direct_preserves_damage_beyond_stack_capacity() {
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(9, 1), renderer_config());
    let screen = renderer.screen();
    let damage: [PhysicalRect; 9] = std::array::from_fn(|x| PhysicalRect {
        x: x as i32,
        y: 0,
        width: 1,
        height: 1,
    });
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(screen.to_logical(1.0)).background(Color::WHITE),
        screen,
        ClipId::default(),
    );
    renderer.render(&paint, &damage);

    assert!(
        renderer
            .buffer()
            .pixels()
            .iter()
            .all(|pixel| pixel.raw() == 0xffffff)
    );
}

#[test]
fn frame_is_rendered_once_per_affected_line_in_order() {
    let mut renderer = Renderer::new(
        TrackingBuffer {
            pixels: vec![Xrgb8888::default(); 16],
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
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(LogicalRect {
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
        ClipId::default(),
    );
    renderer.render(&paint, &damage);

    assert_eq!(renderer.buffer().lines, [0, 2]);
    assert_eq!(renderer.buffer().ranges, [0..4, 0..4]);
}

#[test]
fn scanline_merges_overlapping_damage_per_line() {
    let mut renderer = Renderer::new(
        TrackingBuffer {
            pixels: vec![Xrgb8888::default(); 20],
            lines: Vec::new(),
            ranges: Vec::new(),
            width: 5,
            height: 4,
        },
        renderer_config(),
    )
    .strategy(Scanline::default());
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 5.0,
            height: 4.0,
        })
        .background(Color::WHITE),
        PhysicalRect {
            x: 0,
            y: 0,
            width: 5,
            height: 4,
        },
        ClipId::default(),
    );
    renderer.render(
        &paint,
        &[
            PhysicalRect {
                x: 0,
                y: 0,
                width: 3,
                height: 3,
            },
            PhysicalRect {
                x: 2,
                y: 1,
                width: 3,
                height: 3,
            },
        ],
    );

    assert_eq!(renderer.buffer().lines, [0, 1, 2, 3]);
    assert_eq!(renderer.buffer().ranges, [0..3, 0..5, 0..5, 2..5]);
}

#[test]
fn scanline_only_borrows_dirty_horizontal_ranges() {
    let mut renderer = Renderer::new(
        TrackingBuffer {
            pixels: vec![Xrgb8888::default(); 8],
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
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(LogicalRect {
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
        ClipId::default(),
    );
    renderer.render(&paint, &damage);

    assert_eq!(renderer.buffer().ranges.len(), 1);
    assert_eq!(renderer.buffer().ranges[0], 1..3);
    assert_eq!(
        renderer.buffer().pixels,
        [0, 0xffffff, 0xffffff, 0, 0, 0, 0, 0].map(Xrgb8888::from_raw)
    );
}

#[test]
fn scanline_skips_commands_behind_opaque_content() {
    static RECTANGLE_PIXELS: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);

    #[derive(Clone, Copy, Default)]
    struct CountingPixel {
        color: Xrgb8888,
        draws: u8,
    }

    impl Pixel for CountingPixel {
        fn blend_translucent(&mut self, color: PremultipliedRgbaColor) {
            self.color.blend(color);
            self.draws += 1;
        }

        fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
            Self {
                color: Xrgb8888::from_rgb(red, green, blue),
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
                    pixel.color = Xrgb8888::from_rgb(color.red, color.green, color.blue);
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
    let mut paint = CommandList::default();
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
    paint.clear();
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area)
            .background(Color::from_rgba8(0, 255, 0, 255))
            .uniform_radius(3.0),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area).background(Color::from_rgba8(0, 0, 255, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(&paint, &[damage]);

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
        image: image.id(),
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
        image: transparent_image.id(),
        ..image
    };
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

    assert_eq!(
        RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed),
        16
    );

    static PARTIAL_IMAGE_PIXELS: [u8; 24] = [
        0, 0, 0, 0, 0, 128, 0, 128, 0, 255, 0, 255, 0, 255, 0, 255, 0, 128, 0, 128, 0, 0, 0, 0,
    ];
    static UNDERLAY_ALPHA: [u8; 1] = [128];
    let mut renderer = Renderer::new(VecBuffer::<CountingPixel>::new(6, 1), renderer_config())
        .strategy(Scanline::default());
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
        image: underlay.id(),
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
        image: partial_image.id(),
        area: screen.to_logical(1.0),
        fit: ImageFit::Fill,
        sampling: ImageSampling::Nearest,
        opacity: 1.0,
        colorize: None,
        nine_slice: None,
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
    };
    let background =
        Rectangle::new(screen.to_logical(1.0)).background(Color::from_rgba8(255, 0, 0, 128));
    let overlay =
        Rectangle::new(screen.to_logical(1.0)).background(Color::from_rgba8(0, 0, 255, 128));
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
    paint.clear();
    paint.push_rectangle(background, screen, ClipId::default());
    paint.push_image(underlay, screen, ClipId::default());
    paint.push_image(partial_image, screen, ClipId::default());
    paint.push_rectangle(overlay, screen, ClipId::default());
    renderer.render(&paint, &[screen]);
    assert_eq!(
        RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed),
        14
    );
    for (rendered, source) in renderer
        .buffer()
        .pixels()
        .iter()
        .zip(PARTIAL_IMAGE_PIXELS.chunks_exact(4))
    {
        let mut expected = Xrgb8888::default();
        expected.blend(PremultipliedRgbaColor::new(
            Color::from_rgba8(255, 0, 0, 128),
            255,
        ));
        expected.blend(PremultipliedRgbaColor::new(Color::BLACK, 128));
        expected.blend(PremultipliedRgbaColor {
            red: source[0],
            green: source[1],
            blue: source[2],
            alpha: source[3],
        });
        expected.blend(PremultipliedRgbaColor::new(
            Color::from_rgba8(0, 0, 255, 128),
            255,
        ));
        assert_eq!(rendered.color, expected);
    }

    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
    paint.clear();
    paint.push_rectangle(background, screen, ClipId::default());
    paint.push_image(underlay, screen, ClipId::default());
    paint.push_image(
        ImageRequest {
            opacity: 0.5,
            ..partial_image
        },
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(overlay, screen, ClipId::default());
    renderer.render(&paint, &[screen]);
    assert_eq!(
        RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed),
        18
    );
}

#[test]
fn cached_dirty_ranges_match_direct_rendering() {
    let mut direct = Renderer::new(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config());
    let mut scanline = Renderer::new(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config())
        .strategy(Scanline::default());
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
    let mut paint = CommandList::default();
    paint.push_rectangle(red, clip, ClipId::default());
    paint.push_rectangle(green, clip, ClipId::default());
    direct.render(&paint, &damage);
    scanline.render(&paint, &damage);

    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
}

#[test]
fn box_shadows_match_between_strategies_and_cache_sizes() {
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(
        strategy: S,
    ) -> Renderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(128, 96), renderer_config())
            .strategy(strategy);
        renderer.set_scale_factor(2.0);
        let screen = renderer.screen();
        let first = BoxShadow::new(
            LogicalRect {
                x: 12.0,
                y: 10.0,
                width: 36.0,
                height: 24.0,
            },
            Color::from_rgba8(220, 40, 20, 180),
        )
        .radius(BorderRadius {
            top_left: 6.0,
            top_right: 6.0,
            bottom_right: 6.0,
            bottom_left: 6.0,
        })
        .offset(2.0, 3.0)
        .blur(5.0)
        .spread(1.0);
        let second = BoxShadow {
            area: LogicalRect {
                x: 4.0,
                y: 24.0,
                width: 52.0,
                height: 20.0,
            },
            color: Color::from_rgba8(20, 80, 220, 140),
            ..first
        };
        let mut paint = CommandList::default();
        paint.push_box_shadow(first, screen, ClipId::default());
        paint.push_box_shadow(second, screen, ClipId::default());
        renderer.render(&paint, &[screen]);
        assert_eq!(renderer.context.images.len(), 2);
        paint.clear();
        paint.push_box_shadow(
            BoxShadow {
                area: LogicalRect {
                    x: 20.0,
                    ..first.area
                },
                ..first
            },
            screen,
            ClipId::default(),
        );
        paint.push_rectangle(
            Rectangle::new(first.area)
                .background(Color::WHITE)
                .radius(first.radius),
            screen,
            ClipId::default(),
        );
        let inset = BoxShadow {
            area: first.area,
            color: Color::BLACK,
            offset_x: 0.0,
            offset_y: 0.0,
            spread: 0.0,
            inset: true,
            ..first
        };
        paint.push_box_shadow(inset, screen, ClipId::default());
        renderer.render(&paint, &[screen]);
        assert_eq!(renderer.context.images.len(), 3);
        renderer
    }

    let direct = render(Direct::default());
    let scanline = render(Scanline::default());
    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
    let pixels = direct.buffer().pixels();
    assert_eq!(pixels[44 * 128 + 60].raw(), 0x00ff_ffff);
    assert_ne!(pixels[20 * 128 + 60].raw(), 0x00ff_ffff);
    assert_eq!(pixels[0].raw(), 0);
}

#[test]
fn gradient_borders_match_between_strategies_and_rounded_clips() {
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(
        strategy: S,
    ) -> Renderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer =
            Renderer::new(VecBuffer::<Xrgb8888>::new(48, 36), renderer_config()).strategy(strategy);
        let screen = renderer.screen();
        let mut paint = CommandList::default();
        let clip = paint.push_clip(
            ClipId::default(),
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
            paint.push_rectangle(
                Rectangle::new(LogicalRect {
                    x: 4.0,
                    y: 3.0,
                    width: 40.0,
                    height: 30.0,
                })
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
    assert_eq!(direct.buffer().pixels()[18 * 48 + 24].raw(), 0x0010_131a);
}

#[test]
fn rounded_clips_match_between_strategies() {
    static PIXEL: [u8; 3] = [0, 255, 0];
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(
        strategy: S,
    ) -> Renderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer =
            Renderer::new(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(strategy);
        let image = renderer.create_image(ImageData::new(
            ImagePixels::Static(&PIXEL),
            ImageFormat::Rgb8,
            1,
            1,
        ));
        let string = renderer.text_run("M", TextStyle::default());
        let screen = renderer.screen();
        let area = LogicalRect {
            width: 16.0,
            height: 16.0,
            ..LogicalRect::default()
        };
        let red = Rectangle::new(area).background(Color::from_rgba8(255, 0, 0, 255));
        let image = ImageRequest {
            image: image.id(),
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
            text: string,
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            style: TextStyle::default(),
            options: TextOptions::default(),
        };

        let mut paint = CommandList::default();
        let outer_clip = paint.push_clip(
            ClipId::default(),
            area,
            BorderRadius {
                top_left: 8.0,
                top_right: 8.0,
                bottom_right: 8.0,
                bottom_left: 8.0,
            },
        );
        paint.push_rectangle(red, screen, outer_clip);
        let inner_clip = paint.push_clip(
            outer_clip,
            LogicalRect {
                width: 8.0,
                height: 8.0,
                ..area
            },
            BorderRadius::default(),
        );
        paint.push_image(image, screen, inner_clip);
        paint.push_text(text, screen, inner_clip);
        paint.push_rectangle(
            Rectangle::new(LogicalRect {
                x: 15.0,
                y: 15.0,
                width: 1.0,
                height: 1.0,
            })
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
    assert_eq!(direct.buffer().pixels()[0].raw(), 0);
    let edge = direct.buffer().pixels()[6].raw();
    assert!((1..255).contains(&((edge >> 8) & 0xff)));
    let edge = direct.buffer().pixels()[9].raw();
    assert!((1..255).contains(&((edge >> 16) & 0xff)));
    assert_eq!(direct.buffer().pixels()[15 * 16 + 15].raw(), 0x0000_00ff);
}

#[test]
fn dropped_image_remains_valid_until_frame_end() {
    static PIXEL: [u8; 4] = [255, 0, 0, 255];
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config())
        .strategy(Scanline::default());
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
    let mut paint = CommandList::default();
    paint.push_image(
        ImageRequest {
            image: image.id(),
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
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        },
        damage[0],
        ClipId::default(),
    );
    let image_id = image.id();
    drop(image);
    renderer.render(&paint, &damage);

    assert_eq!(renderer.buffer().pixels()[0].raw(), 0x00ff_0000);
    let image = RendererImageId::from(KeyData::from_ffi(image_id.0));
    assert!(!renderer.context.images.contains_key(image));
}

#[test]
fn text_runs_are_keyed_by_content_and_style() {
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(32, 24), renderer_config());
    let style = TextStyle::default();
    let first = renderer.text_run("same", style);

    assert_eq!(renderer.text_run("same", style), first);
    assert_ne!(renderer.text_run("changed", style), first);
    assert_ne!(
        renderer.text_run(
            "same",
            TextStyle {
                weight: 500,
                ..style
            },
        ),
        first
    );
}

#[test]
fn borrowed_dynamic_text_renders_after_the_source_is_reused() {
    let mut harness = Harness::new(
        Renderer::new(VecBuffer::<Xrgb8888>::new(96, 48), renderer_config())
            .strategy(Scanline::default()),
        RepaintBuffer::Reused,
    );
    let mut text = String::from("managed");

    harness.render(Duration::ZERO, Input::None, |ui| {
        Text::new(&text).color(Color::WHITE).render(ui);
    });
    text.clear();
    text.push_str("updated");
    harness.render(Duration::ZERO, Input::None, |ui| {
        Text::new(&text).color(Color::WHITE).render(ui);
    });

    assert!(
        harness
            .renderer()
            .buffer()
            .pixels()
            .iter()
            .any(|pixel| pixel.raw() != 0)
    );
}
