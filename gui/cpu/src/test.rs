use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
};

use blit::{LogicalPoint, LogicalRect, PhysicalRect, Scale2};
use blit_gui::{
    FontData, FontFamily, GuiContext, TextConfig, TextSystem,
    color::Color,
    display_list::{BoxShadow, ClipId, DisplayList, Mesh, MeshVertex, Rectangle},
    image::{ImageData, ImageFit, ImageFormat, ImagePixels, ImageRequest, ImageSampling, ImageTiling},
    style::{Border, BorderRadius, Fill, GradientStop, LinearGradient},
    text::{
        FontId, HorizontalAlign, Span, SpanStyle, TextLayoutRequest, TextOptions, TextPhases, TextRequest, TextRunId,
        TextStyle, TextWrap, VerticalAlign,
    },
};
use blit_text::{
    Caret, FontCandidate, FontError, FontFaceId as BackendFontFaceId, FontSelectionId, GlyphMask, LayoutLine,
    LayoutRequest, TextLayout, TextLayoutEngine,
};

use super::*;

const SCALE: Scale2 = Scale2::IDENTITY;

#[test]
fn oversized_borders_fill_rectangle() {
    let area = LogicalRect::new(0.0, 0.0, 8.0, 8.0);
    let bounds = area.to_physical(SCALE);
    let stops = [
        GradientStop::new(0.0, Color::WHITE),
        GradientStop::new(1.0, Color::WHITE),
    ];
    for border in [
        Border::solid(f32::MAX, Color::WHITE),
        Border::gradient(f32::MAX, LinearGradient::new(&stops)),
    ] {
        let mut list = DisplayList::default();
        list.push_rectangle(Rectangle::new(area).border(border), bounds, ClipId::default());
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config());
        renderer.render(&mut list);
        assert!(
            renderer
                .render
                .buffer()
                .pixels()
                .iter()
                .all(|pixel| pixel.raw() == 0xffffff)
        );
    }
}

#[test]
fn scattered_tiles_stay_local_across_command_reorders() {
    let mut renderer = new_renderer(
        TrackingBuffer {
            pixels: vec![Xrgb8888::default(); 643 * 53],
            lines: Vec::new(),
            ranges: Vec::new(),
            width: 643,
            height: 53,
        },
        RendererConfig {
            damage_tile_size: 8,
            ..renderer_config()
        },
    )
    .strategy(Scanline::default());
    for frame in 0..3 {
        renderer.render.buffer_mut().ranges.clear();
        let list = renderer.gui.render_input().display_list;
        list.push_clear(PhysicalRect::new(0, 0, 643, 53));
        let clip = list.push_clip(
            ClipId::default(),
            LogicalRect::new(0.0, 0.0, if frame == 2 { 635.0 } else { 643.0 }, 53.0),
            BorderRadius::default(),
        );
        for position in 0..40 {
            let index = if frame == 2 { 39 - position } else { position };
            let area = LogicalRect::new((index * 16 + 1) as f32, 1.0, 2.0, 2.0);
            list.push_rectangle(
                Rectangle::new(area).background(if frame == 0 { Color::WHITE } else { Color::BLACK }),
                area.to_physical(SCALE),
                clip,
            );
        }
        renderer.render.render(renderer.gui.render_input(), 1);
        renderer.gui.finish_frame();
        let touched: usize = renderer.render.buffer().ranges.iter().map(Range::len).sum();
        assert_eq!(touched, [643 * 53, 40 * 8 * 8, 0][frame]);
    }
}

#[test]
fn dirty_tiles_preserve_gaps_and_blend_once() {
    fn check<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default>() {
        let screen = PhysicalRect::new(0, 0, 7, 5);
        let regions = [
            PhysicalRect::new(1, 0, 2, 2),
            PhysicalRect::new(3, 1, 1, 2),
            PhysicalRect::new(6, 4, 4, 4),
            PhysicalRect::new(-4, 4, 5, 1),
        ];
        let mut renderer = new_renderer(
            VecBuffer::<Xrgb8888>::new(7, 5),
            RendererConfig {
                damage_tile_size: 2,
                ..renderer_config()
            },
        )
        .strategy(S::default());
        for frame in 0..2 {
            let list = renderer.gui.render_input().display_list;
            list.push_rectangle(
                Rectangle::new(screen.to_logical(SCALE)).background(Color::rgba(255, 0, 0, 128)),
                screen,
                ClipId::default(),
            );
            for region in regions {
                list.push_rectangle(
                    Rectangle::new(region.to_logical(SCALE))
                        .background(Color::WHITE)
                        .opacity(frame as f32),
                    region,
                    ClipId::default(),
                );
            }
            if frame == 0 {
                // seed tracking without drawing to isolate partial rasterization
                renderer.render.damage.update(list, screen, 1.0, 1);
            } else {
                renderer.render.render(renderer.gui.render_input(), 1);
            }
            renderer.gui.finish_frame();
        }
        let red = 0x800000;
        let white = 0xffffff;
        let expected = [
            [red, white, white, red, 0, 0, 0],
            [red, white, white, white, 0, 0, 0],
            [0, 0, red, white, 0, 0, 0],
            [0, 0, red, red, 0, 0, 0],
            [white, red, 0, 0, 0, 0, white],
        ];
        for (index, pixel) in renderer.render.buffer().pixels().iter().enumerate() {
            assert_eq!(pixel.raw(), expected[index / 7][index % 7], "pixel {index}");
        }
    }
    check::<Direct>();
    check::<Scanline>();
}

#[test]
fn paint_changes_invalidate_tiles_without_changing_bounds() {
    let screen = PhysicalRect::new(0, 0, 16, 16);
    let area = screen.to_logical(SCALE);
    let mut damage = Damage::new(8);
    let mut list = DisplayList::default();
    for case in ["fill", "border", "text palette", "image", "shadow", "mesh", "clip"] {
        for (frame, color) in [Color::BLACK, Color::WHITE, Color::WHITE].into_iter().enumerate() {
            list.clear();
            match case {
                "fill" | "border" => {
                    let stops = [GradientStop::new(0.0, Color::BLACK), GradientStop::new(1.0, color)];
                    let gradient = LinearGradient::new(&stops);
                    let rectangle = if case == "fill" {
                        Rectangle::new(area).background(gradient)
                    } else {
                        Rectangle::new(area).border(Border::gradient(2.0, gradient))
                    };
                    list.push_rectangle(rectangle, screen, ClipId::default());
                }
                "text palette" => {
                    let spans = [Span::new(0..1).style(SpanStyle::new().color(color))];
                    let palette = list.text_palette(&spans);
                    list.push_text_palette(
                        TextRequest {
                            text: TextRunId(1),
                            area,
                            offset_x: 0.0,
                            color: Color::WHITE,
                            options: TextOptions::default(),
                        },
                        palette,
                        screen,
                        ClipId::default(),
                    );
                }
                "image" => list.push_image(
                    ImageRequest {
                        image: 1,
                        area,
                        fit: ImageFit::Fill,
                        sampling: ImageSampling::Nearest,
                        opacity: if color == Color::BLACK { 0.5 } else { 1.0 },
                        colorize: None,
                        nine_slice: None,
                        horizontal_tiling: ImageTiling::None,
                        vertical_tiling: ImageTiling::None,
                    },
                    screen,
                    ClipId::default(),
                ),
                "shadow" => list.push_box_shadow(BoxShadow::new(area, color).blur(2.0), screen, ClipId::default()),
                "mesh" => list.push_mesh(
                    Mesh {
                        vertices: &[
                            MeshVertex::new(0.0, 0.0, color),
                            MeshVertex::new(16.0, 0.0, Color::WHITE),
                            MeshVertex::new(0.0, 16.0, Color::WHITE),
                        ],
                        indices: &[0, 1, 2],
                    },
                    SCALE,
                    ClipId::default(),
                ),
                "clip" => {
                    let clip = list.push_clip(
                        ClipId::default(),
                        area,
                        BorderRadius::uniform(if color == Color::BLACK { 0.0 } else { 4.0 }),
                    );
                    list.push_rectangle(Rectangle::new(area).background(Color::WHITE), screen, clip);
                }
                _ => unreachable!(),
            }
            damage.update(&list, screen, 1.0, 1);
            assert_eq!(damage.is_empty(), frame == 2, "{case} frame {frame}");
        }
    }
}

#[test]
fn reused_buffers_receive_every_change_since_their_last_presentation() {
    fn check<const COUNT: usize>(stale: bool) {
        let mut renderer = new_renderer(
            VecBuffer::<Argb8888>::new(37, 21),
            RendererConfig {
                damage_tile_size: 8,
                ..renderer_config()
            },
        )
        .strategy(Scanline::default());
        let mut buffers: [_; COUNT] = std::array::from_fn(|_| VecBuffer::<Argb8888>::new(37, 21));
        let mut presented = [None; COUNT];
        for frame in 0..270usize {
            let index = if !stale {
                frame % COUNT
            } else if frame == 0 || frame == 255 {
                0
            } else {
                1 + frame % (COUNT - 1)
            };
            let age = presented[index]
                .map(|last| u8::try_from(frame - last).unwrap_or(0))
                .unwrap_or(0);
            std::mem::swap(renderer.render.buffer_mut(), &mut buffers[index]);
            let left = frame.min(19) % 5 * 7;
            let list = renderer.gui.render_input().display_list;
            list.push_clear(PhysicalRect::new(0, 0, 37, 21));
            let area = LogicalRect::new(left as f32, 2.0, 6.0, 16.0);
            list.push_rectangle(
                Rectangle::new(area).background(Color::WHITE),
                area.to_physical(SCALE),
                ClipId::default(),
            );
            renderer.render.render(renderer.gui.render_input(), age);
            let mut expected = [Argb8888::default(); 37 * 21];
            for row in 2..18 {
                expected[row * 37 + left..row * 37 + left + 6].fill(Argb8888::from_raw(u32::MAX));
            }
            assert_eq!(
                renderer.render.buffer().pixels(),
                expected,
                "buffers {COUNT} stale {stale} frame {frame} age {age}"
            );
            renderer.gui.finish_frame();
            std::mem::swap(renderer.render.buffer_mut(), &mut buffers[index]);
            presented[index] = Some(frame);
        }
    }
    check::<1>(false);
    check::<2>(false);
    check::<3>(false);
    check::<3>(true);
}

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

    fn process_line(&mut self, line: usize, range: Range<usize>, process: impl FnOnce(&mut [Xrgb8888])) {
        self.lines.push(line);
        self.ranges.push(range.clone());
        let start = line * self.width;
        process(&mut self.pixels[start + range.start..start + range.end]);
    }
}

fn renderer_config() -> RendererConfig {
    RendererConfig {
        damage_tile_size: 32,
        paint_cache_capacity: 1024 * 1024,
        glyph_cache_capacity: 1024 * 1024,
        shadow_cache_capacity: 1024 * 1024,
        text_phases: TextPhases::Four,
    }
}

struct TestRenderer<B: PixelBuffer, S: RenderStrategy<B> = Direct> {
    render: Renderer<B, S>,
    gui: GuiContext,
}

impl<B: PixelBuffer, S: RenderStrategy<B>> TestRenderer<B, S> {
    fn render(&mut self, display_list: &mut DisplayList) {
        let mut input = self.gui.render_input();
        input.display_list = display_list;
        self.render.render(input, 0);
        self.gui.finish_frame();
    }

    fn set_scale(&mut self, scale: Scale2) {
        self.gui.set_scale(scale.x);
        self.render.set_scale(scale);
    }
}

impl<B: PixelBuffer> TestRenderer<B> {
    fn strategy<T: RenderStrategy<B>>(self, strategy: T) -> TestRenderer<B, T> {
        TestRenderer {
            render: self.render.strategy(strategy),
            gui: self.gui,
        }
    }
}

fn new_renderer<B: PixelBuffer>(buffer: B, config: RendererConfig) -> TestRenderer<B> {
    new_renderer_with_backend(buffer, config, blit_text_cosmic::Backend::without_system_fonts())
}

fn new_renderer_with_backend<B: PixelBuffer, T: TextLayoutEngine>(
    buffer: B,
    config: RendererConfig,
    backend: T,
) -> TestRenderer<B> {
    TestRenderer {
        render: Renderer::new(buffer, config),
        gui: GuiContext::new(
            TextSystem::new(
                TextConfig {
                    fonts: vec![FontFamily {
                        id: FontId::default(),
                        fonts: vec![FontData::Static(include_bytes!(env!("BLIT_TEST_FONT")))],
                    }],
                    text_cache_capacity: 1024 * 1024,
                    layout_cache_capacity: 1024 * 1024,
                },
                backend,
            )
            .unwrap(),
        ),
    }
}

#[test]
fn renderer_supports_custom_pixel_layouts() {
    let mut renderer = new_renderer(VecBuffer::<BgrPixel>::new(32, 24), renderer_config());
    let clip = PhysicalRect {
        x: 0,
        y: 0,
        width: 32,
        height: 24,
    };
    let mut paint = DisplayList::default();
    paint.push_rectangle(
        Rectangle::new(LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 24.0,
        })
        .background(Color::rgb(12, 34, 56)),
        clip,
        ClipId::default(),
    );
    renderer.render(&mut paint);
    assert_eq!(
        renderer.render.buffer().pixels()[0],
        BgrPixel {
            blue: 56,
            green: 34,
            red: 12
        }
    );

    paint.clear();
    let spans = [Span::new(0..1).style(SpanStyle::new().size(20.0).color(Color::WHITE))];
    let m = renderer.gui.rich_text("M", &spans, TextStyle::default()).0;
    let palette = paint.text_palette(&spans);
    paint.push_text_palette(
        TextRequest {
            text: m,
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 32.0,
                height: 24.0,
            },
            offset_x: 0.0,
            color: Color::TRANSPARENT,
            options: TextOptions::default(),
        },
        palette,
        clip,
        ClipId::default(),
    );
    renderer.render(&mut paint);
    assert!(renderer.render.buffer().pixels().iter().any(|pixel| pixel.red > 12));

    renderer.set_scale(Scale2::uniform(2.0));
    let mut request = TextRequest {
        text: renderer.gui.text_run("abc", TextStyle::default()),
        area: LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 24.0,
        },
        offset_x: 0.0,
        color: Color::WHITE,
        options: TextOptions::default(),
    };
    assert_eq!(
        renderer
            .gui
            .text_offset_at_position(&request, LogicalPoint { x: 100.0, y: 12.0 },),
        "abc".len()
    );
    let start = renderer.gui.text_cursor_rect(&request, 0);
    let end = renderer.gui.text_cursor_rect(&request, "abc".len());
    assert_eq!(start.width, 0.5);
    assert!(end.x > start.x);
    let shifted = TextRequest {
        offset_x: 4.0,
        ..request
    };
    assert_eq!(renderer.gui.text_cursor_rect(&shifted, 0).x, start.x - 4.0);
    assert_eq!(
        renderer.gui.text_offset_at_position(
            &shifted,
            LogicalPoint {
                x: end.x - 4.0,
                y: end.y,
            },
        ),
        renderer
            .gui
            .text_offset_at_position(&request, LogicalPoint { x: end.x, y: end.y }),
    );
    request.area.width = 64.0;
    request.area.height = 48.0;
    request.options.horizontal_align = HorizontalAlign::Center;
    request.options.vertical_align = VerticalAlign::Bottom;
    let placed_start = renderer.gui.text_cursor_rect(&request, 0);
    assert!(placed_start.x > start.x);
    assert!(placed_start.y > start.y);
    request.area.width += 0.5;
    request.area.height += 0.25;
    let fractional_start = renderer.gui.text_cursor_rect(&request, 0);
    assert!((fractional_start.x - placed_start.x - 0.25).abs() < 0.001);
    assert!((fractional_start.y - placed_start.y - 0.25).abs() < 0.001);
}

#[test]
fn mesh_interpolates_colors_without_shared_edge_overdraw() {
    let mut renderer = new_renderer(VecBuffer::<BgrPixel>::new(16, 8), renderer_config()).strategy(Scanline::default());
    let bounds = PhysicalRect {
        x: 0,
        y: 0,
        width: 16,
        height: 8,
    };
    let color = Color::rgba(255, 0, 0, 128);
    let vertices = [
        MeshVertex::new(0.0, 0.0, color),
        MeshVertex::new(8.0, 0.0, color),
        MeshVertex::new(0.0, 8.0, color),
        MeshVertex::new(8.0, 8.0, color),
        MeshVertex::new(8.0, 0.0, Color::rgb(255, 0, 0)),
        MeshVertex::new(16.0, 0.0, Color::rgb(0, 255, 0)),
        MeshVertex::new(8.0, 8.0, Color::rgb(0, 0, 255)),
    ];
    let mut paint = DisplayList::default();
    paint.push_mesh(
        Mesh {
            vertices: &vertices,
            indices: &[0, 1, 2, 2, 1, 3, 4, 5, 6],
        },
        Scale2::IDENTITY,
        ClipId::default(),
    );

    renderer.render(&mut paint);

    let expected: [BgrPixel; 128] = renderer.render.buffer().pixels().try_into().unwrap();
    let mut rectangles = DisplayList::default();
    rectangles.push_rectangle(
        Rectangle::new(LogicalRect::new(0.0, 0.0, 16.0, 8.0)).background(Color::BLACK),
        bounds,
        ClipId::default(),
    );
    renderer.render(&mut rectangles);
    renderer.render(&mut paint);
    assert_eq!(renderer.render.buffer().pixels(), expected);

    let pixels = renderer.render.buffer().pixels();
    assert!(pixels.chunks_exact(16).all(|row| row[..8].iter().all(|pixel| *pixel
        == BgrPixel {
            red: 128,
            green: 0,
            blue: 0
        })));
    assert!(pixels[16 + 9].red > pixels[16 + 9].green);
    assert!(pixels[16 + 13].green > pixels[16 + 13].red);
    assert!(pixels[5 * 16 + 9].blue > pixels[5 * 16 + 9].red);
}

#[test]
fn render_input_tracks_damage_and_invalidation() {
    let mut renderer = new_renderer(
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
    let rectangle = Rectangle::new(LogicalRect::new(0.0, 0.0, 2.0, 2.0)).background(Color::WHITE);
    let render = |renderer: &mut TestRenderer<TrackingBuffer, Scanline>| {
        renderer.render.buffer_mut().lines.clear();
        let input = renderer.gui.render_input();
        input.display_list.clear();
        input
            .display_list
            .push_rectangle(rectangle, PhysicalRect::new(0, 0, 4, 4), ClipId::default());
        renderer.render.render(input, 1);
        renderer.gui.finish_frame();
    };

    render(&mut renderer);
    assert!(!renderer.render.buffer().lines.is_empty());

    render(&mut renderer);
    assert!(renderer.render.buffer().lines.is_empty());

    renderer.render.invalidate_all();
    render(&mut renderer);
    assert!(!renderer.render.buffer().lines.is_empty());

    renderer.gui.set_scale(2.0);
    render(&mut renderer);
    assert!(!renderer.render.buffer().lines.is_empty());

    let buffer = renderer.render.buffer_mut();
    buffer.width = 8;
    buffer.pixels.resize(8 * 4, Xrgb8888::default());
    render(&mut renderer);
    assert!(!renderer.render.buffer().lines.is_empty());
}

#[test]
fn fontdue_layout_renders_with_cpu_rasterization() {
    let mut renderer = new_renderer_with_backend(
        VecBuffer::<Xrgb8888>::new(32, 24),
        renderer_config(),
        blit_text_fontdue::Backend::new(),
    );
    let area = LogicalRect {
        x: 0.0,
        y: 0.0,
        width: 32.0,
        height: 24.0,
    };
    let mut display_list = DisplayList::default();
    let text = renderer
        .gui
        .rich_text(
            "M",
            &[Span::new(0..1).style(SpanStyle::new().size(20.0))],
            TextStyle::default(),
        )
        .0;
    display_list.push_text(
        TextRequest {
            text,
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            options: TextOptions::default(),
        },
        area.to_physical(SCALE),
        ClipId::default(),
    );
    renderer.render(&mut display_list);

    assert!(renderer.render.buffer().pixels().iter().any(|pixel| pixel.raw() != 0));
}

#[derive(Default)]
struct TextCounts {
    shapes: AtomicUsize,
    layouts: AtomicUsize,
    carets: AtomicUsize,
}

struct CountingBackend(Arc<TextCounts>);

impl TextLayoutEngine for CountingBackend {
    type Shape = ();

    fn register_font(&mut self, _data: FontData) -> Result<Vec<FontCandidate>, FontError> {
        Ok(vec![FontCandidate {
            face: BackendFontFaceId(1),
            weight: 400,
            stretch: 100,
            style: blit_text::FontStyle::Normal,
        }])
    }

    fn register_font_selection(&mut self, _candidates: &[FontCandidate]) -> Result<FontSelectionId, FontError> {
        Ok(FontSelectionId(1))
    }

    fn rasterize_glyph(&mut self, _face: BackendFontFaceId, _glyph: u16, _size: f32, _offset_x: f32) -> GlyphMask {
        GlyphMask::default()
    }

    fn shape(&mut self, _text: blit_text::Text<'_>) -> ((), usize) {
        self.0.shapes.fetch_add(1, Relaxed);
        ((), 0)
    }

    fn layout(&mut self, _shape: &mut (), _text: &str, _request: LayoutRequest) -> TextLayout {
        self.0.layouts.fetch_add(1, Relaxed);
        TextLayout {
            lines: Box::new([LayoutLine {
                bounds: LogicalRect::default(),
                text: 0..0,
            }]),
            ..TextLayout::default()
        }
    }

    fn carets(&mut self, _shape: &mut (), _text: &str, _request: LayoutRequest, _line: usize) -> Box<[Caret]> {
        self.0.carets.fetch_add(1, Relaxed);
        Box::new([Caret {
            byte_offset: 0,
            position: LogicalPoint::default(),
            height: 0.0,
        }])
    }
}

#[test]
fn text_cache_stages_are_independent() {
    let counts = Arc::new(TextCounts::default());
    let mut text = TextSystem::new(
        TextConfig {
            fonts: vec![FontFamily {
                id: FontId::default(),
                fonts: vec![FontData::Static(include_bytes!(env!("BLIT_TEST_FONT")))],
            }],
            text_cache_capacity: 1024,
            layout_cache_capacity: 0,
        },
        CountingBackend(counts.clone()),
    )
    .unwrap();
    let request = TextLayoutRequest {
        text: text.text_run("cached".into(), TextStyle::default()),
        wrap: TextWrap::None,
        max_width: None,
        max_lines: None,
    };

    text.measure(&request);
    text.measure(&request);
    assert_eq!(counts.shapes.load(Relaxed), 1);
    assert_eq!(counts.layouts.load(Relaxed), 1);

    let mut paint = TextRequest {
        text: request.text,
        area: LogicalRect {
            width: 1.0,
            height: 1.0,
            ..LogicalRect::default()
        },
        offset_x: 0.0,
        color: Color::WHITE,
        options: TextOptions::default(),
    };
    text.cursor_rect(&paint, 0, 0.0);
    paint.offset_x = 10.0;
    paint.area.width = 10.0;
    paint.area.height = 10.0;
    paint.options.horizontal_align = HorizontalAlign::Center;
    paint.options.vertical_align = VerticalAlign::Bottom;
    text.cursor_rect(&paint, 0, 0.0);
    assert_eq!(counts.layouts.load(Relaxed), 1);
    assert_eq!(counts.carets.load(Relaxed), 1);

    text.finish_frame();
    text.measure(&request);
    assert_eq!(counts.shapes.load(Relaxed), 1);
    assert_eq!(counts.layouts.load(Relaxed), 2);

    let mut wrapped = TextLayoutRequest {
        wrap: TextWrap::Word,
        max_width: Some(10.0),
        ..request
    };
    text.measure(&wrapped);
    wrapped.max_width = Some(20.0);
    text.measure(&wrapped);
    assert_eq!(counts.shapes.load(Relaxed), 1);
    assert_eq!(counts.layouts.load(Relaxed), 4);
}

#[test]
fn text_measurement_reports_wrapped_layout_size() {
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(32, 24), renderer_config());
    let request = TextLayoutRequest {
        text: renderer.gui.text_run("hello world", TextStyle::default()),
        wrap: TextWrap::None,
        max_width: None,
        max_lines: None,
    };
    let unwrapped = renderer.gui.measure_text(&request);
    let mixed = renderer
        .gui
        .rich_text(
            "hello\nworld",
            &[Span::new(0..6), Span::new(6..11).style(SpanStyle::new().size(32.0))],
            TextStyle::default(),
        )
        .0;
    let mixed_size = renderer.gui.measure_text(&TextLayoutRequest { text: mixed, ..request });
    assert!(mixed_size.height > unwrapped.height);
    assert_eq!(
        renderer
            .gui
            .measure_text(&TextLayoutRequest {
                text: mixed,
                max_lines: Some(1),
                ..request
            })
            .height,
        unwrapped.height
    );
    let wrapped = renderer.gui.measure_text(&TextLayoutRequest {
        wrap: TextWrap::Word,
        max_width: Some(unwrapped.width / 2.0),
        ..request
    });

    assert!(wrapped.width < unwrapped.width);
    assert!(wrapped.height > unwrapped.height);
}

#[test]
fn clear_resets_stale_pixels_before_drawing() {
    fn render<S: RenderStrategy<VecBuffer<Argb8888>>>(strategy: S, clear: bool, stale: Argb8888) -> Vec<Argb8888> {
        let mut renderer = new_renderer(VecBuffer::<Argb8888>::new(12, 10), renderer_config()).strategy(strategy);
        renderer.render.buffer_mut().pixels_mut().fill(stale);
        let screen = renderer.render.screen();
        let rectangle = Rectangle::new(LogicalRect {
            width: 12.0,
            height: 10.0,
            ..LogicalRect::default()
        })
        .background(Color::rgba(40, 120, 220, 144))
        .border(Border::solid(2.0, Color::rgba(240, 80, 30, 192)))
        .radius(BorderRadius::uniform(5.0));
        let mut paint = DisplayList::default();
        if clear {
            paint.push_clear(screen);
        }
        paint.push_rectangle(rectangle, screen, ClipId::default());
        renderer.render(&mut paint);
        renderer.render.buffer().pixels().to_vec()
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

    let mut renderer = new_renderer(VecBuffer::<Argb8888>::new(12, 10), renderer_config());
    renderer.render.buffer_mut().pixels_mut().fill(stale);
    let screen = renderer.render.screen();
    let mut paint = DisplayList::default();
    paint.push_clear(screen);
    renderer.render(&mut paint);
    assert!(
        renderer
            .render
            .buffer()
            .pixels()
            .iter()
            .all(|pixel| *pixel == transparent)
    );
}

#[test]
fn commands_outside_damage_are_not_prepared() {
    let mut renderer = new_renderer(
        VecBuffer::<Xrgb8888>::new(8, 4),
        RendererConfig {
            damage_tile_size: 2,
            ..renderer_config()
        },
    );
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
    let mut paint = DisplayList::default();
    paint.push_text(
        TextRequest {
            text: TextRunId(u64::MAX),
            area: outside,
            offset_x: 0.0,
            color: Color::WHITE,
            options: TextOptions::default(),
        },
        outside.to_physical(SCALE),
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(damaged).background(Color::WHITE),
        damaged.to_physical(SCALE),
        ClipId::default(),
    );

    // seed tracking without preparing the invalid text
    let screen = renderer.render.screen();
    renderer.render.damage.update(&paint, screen, 1.0, 1);
    paint.push_rectangle(
        Rectangle::new(damaged).background(Color::WHITE),
        damaged.to_physical(SCALE),
        ClipId::default(),
    );
    let mut input = renderer.gui.render_input();
    input.display_list = &mut paint;
    renderer.render.render(input, 1);
    renderer.gui.finish_frame();

    assert_eq!(
        renderer.render.buffer().pixels(),
        [
            0xffffff, 0xffffff, 0, 0, 0, 0, 0, 0, 0xffffff, 0xffffff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0
        ]
        .map(Xrgb8888::from_raw)
    );
}

#[test]
fn dropped_image_is_removed_after_last_handle() {
    static PIXEL: [u8; 4] = [255, 255, 255, 255];
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config());
    let texture = ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgba8, 1, 1);

    let first = renderer.render.create_image(texture);
    let retained = first.clone();
    let first_id = first.id();
    renderer.render(&mut DisplayList::default());
    drop(first);
    renderer.render(&mut DisplayList::default());
    assert!(renderer.render.context.images.get(first_id.into()).is_some());

    drop(retained);
    renderer.render(&mut DisplayList::default());
    assert!(renderer.render.context.images.get(first_id.into()).is_none());

    let replacement = renderer.render.create_image(ImageData::new(
        ImagePixels::Static(&[0, 255, 0, 255]),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    assert_ne!(replacement.id(), first_id);
    assert_eq!(replacement.id() as u32, first_id as u32);
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

    let alpha = [
        0, 255, 255, 255, 0, 0, 0, 255, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 0, 0, 255, 255,
    ];
    let mut pixels = [0; 6 * 4 * 4];
    for (pixel, alpha) in pixels.chunks_exact_mut(4).zip(alpha) {
        pixel.copy_from_slice(&[alpha / 2, alpha / 4, alpha / 8, alpha]);
    }
    let mut renderer =
        new_renderer(VecBuffer::<TrackingPixel>::new(6, 4), renderer_config()).strategy(Scanline::default());
    let image = renderer.render.create_image(ImageData::new(
        ImagePixels::Owned(pixels.into()),
        ImageFormat::Rgba8Premultiplied,
        6,
        4,
    ));
    renderer.render(&mut DisplayList::default());
    let key = image.id();
    let rows = &renderer.render.context.images.get(key.into()).unwrap().alpha_rows;
    let rows: [_; 4] = std::array::from_fn(|index| rows.get(ImageFormat::Rgba8Premultiplied, index).unwrap());
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([4, 4, 6, 6]));
    assert!(rows.iter().map(|row| row.opaque_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.opaque_end).eq([4, 4, 2, 2]));
    assert!(renderer.render.context.images.get(key.into()).unwrap().has_opaque_spans);
    assert!(!renderer.render.context.images.get(key.into()).unwrap().opaque);

    let screen = renderer.render.screen();
    let request = ImageRequest {
        image: image.id(),
        area: screen.to_logical(SCALE),
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
    let mut paint = DisplayList::default();
    paint.push_image(request, screen, ClipId::default());
    renderer.render(&mut paint);
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
    renderer.render(&mut paint);
    assert_eq!(COPIED.load(Ordering::Relaxed), 0);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 18);

    let image = renderer.render.create_image(ImageData::new(
        ImagePixels::Owned(
            [
                0, 64, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 2, 0, 0, 255, 255, 255, 255, 255, 255,
            ]
            .into(),
        ),
        ImageFormat::Alpha8(Color::WHITE),
        6,
        4,
    ));
    renderer.render(&mut DisplayList::default());
    let key = image.id();
    let rows = &renderer.render.context.images.get(key.into()).unwrap().alpha_rows;
    let rows: [_; 4] = std::array::from_fn(|index| rows.get(ImageFormat::Alpha8(Color::WHITE), index).unwrap());
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 0, 1, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([3, 0, 4, 6]));
    assert!(rows.iter().all(|row| row.opaque_start == 0 && row.opaque_end == 0));
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
    renderer.render(&mut paint);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 11);

    let image = renderer.render.create_image(ImageData::new(
        ImagePixels::Static(&[255; 6 * 4]),
        ImageFormat::Alpha8(Color::WHITE),
        6,
        4,
    ));
    renderer.render(&mut DisplayList::default());
    let key = image.id();
    let image = &renderer.render.context.images.get(key.into()).unwrap();
    assert!(image.opaque);
    assert!(image.alpha_rows.get(ImageFormat::Alpha8(Color::WHITE), 0).is_none());

    let image = renderer.render.create_image(ImageData::new(
        ImagePixels::Static(&[255; 6 * 4 * 4]),
        ImageFormat::Rgba8Premultiplied,
        6,
        4,
    ));
    renderer.render(&mut DisplayList::default());
    let key = image.id();
    let image = &renderer.render.context.images.get(key.into()).unwrap();
    assert!(image.opaque);
    assert!(image.alpha_rows.get(ImageFormat::Rgba8Premultiplied, 0).is_none());
}

#[test]
fn scanline_skips_commands_behind_opaque_content() {
    static RECTANGLE_PIXELS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static SOLID_PAIRS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

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

        fn blend_solid_pair(pixels: &mut [Self], first: PremultipliedRgbaColor, second: PremultipliedRgbaColor) {
            SOLID_PAIRS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Self::blend_slice(pixels, first);
            Self::blend_slice(pixels, second);
        }
    }

    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(4, 2), renderer_config()).strategy(Scanline::default());
    let screen = renderer.render.screen();
    let area = LogicalRect {
        width: 4.0,
        height: 2.0,
        ..LogicalRect::default()
    };
    let mut paint = DisplayList::default();
    paint.push_rectangle(
        Rectangle::new(area).background(Color::rgba(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area).background(Color::rgb(0, 255, 0)),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area).background(Color::rgba(0, 0, 255, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(&mut paint);

    assert_eq!(SOLID_PAIRS.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert!(renderer.render.buffer().pixels().iter().all(|pixel| pixel.draws == 2));

    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(8, 7), renderer_config()).strategy(Scanline::default());
    let screen = renderer.render.screen();
    let area = LogicalRect {
        width: 8.0,
        height: 7.0,
        ..LogicalRect::default()
    };
    paint.clear();
    paint.push_rectangle(
        Rectangle::new(area).background(Color::rgba(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area)
            .background(Color::rgb(0, 255, 0))
            .radius(BorderRadius::uniform(3.0)),
        screen,
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(area).background(Color::rgba(0, 0, 255, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(&mut paint);

    assert!(
        renderer.render.buffer().pixels()[3 * 8..4 * 8]
            .iter()
            .all(|pixel| pixel.draws == 2)
    );

    static IMAGE_PIXEL: [u8; 4] = [0, 255, 0, 255];
    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(4, 2), renderer_config()).strategy(Scanline::default());
    let image = renderer.render.create_image(ImageData::new(
        ImagePixels::Static(&IMAGE_PIXEL),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    let screen = renderer.render.screen();
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
        Rectangle::new(area).background(Color::rgba(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    paint.push_image(image, screen, ClipId::default());
    paint.push_rectangle(
        Rectangle::new(area).background(Color::rgba(0, 0, 255, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(&mut paint);

    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 8);

    static TRANSPARENT_IMAGE_PIXEL: [u8; 4] = [0, 255, 0, 254];
    let transparent_image = renderer.render.create_image(ImageData::new(
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
        Rectangle::new(area).background(Color::rgba(255, 0, 0, 128)),
        screen,
        ClipId::default(),
    );
    paint.push_image(transparent_image, screen, ClipId::default());
    paint.push_rectangle(
        Rectangle::new(area).background(Color::rgba(0, 0, 255, 128)),
        screen,
        ClipId::default(),
    );
    renderer.render(&mut paint);

    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 16);

    static PARTIAL_IMAGE_PIXELS: [u8; 24] = [
        0, 0, 0, 0, 0, 128, 0, 128, 0, 255, 0, 255, 0, 255, 0, 255, 0, 128, 0, 128, 0, 0, 0, 0,
    ];
    static UNDERLAY_ALPHA: [u8; 1] = [128];
    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(6, 1), renderer_config()).strategy(Scanline::default());
    let partial_image = renderer.render.create_image(ImageData::new(
        ImagePixels::Static(&PARTIAL_IMAGE_PIXELS),
        ImageFormat::Rgba8Premultiplied,
        6,
        1,
    ));
    let screen = renderer.render.screen();
    let underlay = renderer.render.create_image(ImageData::new(
        ImagePixels::Static(&UNDERLAY_ALPHA),
        ImageFormat::Alpha8(Color::BLACK),
        1,
        1,
    ));
    let underlay = ImageRequest {
        image: underlay.id(),
        area: screen.to_logical(SCALE),
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
        area: screen.to_logical(SCALE),
        fit: ImageFit::Fill,
        sampling: ImageSampling::Nearest,
        opacity: 1.0,
        colorize: None,
        nine_slice: None,
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
    };
    let background = Rectangle::new(screen.to_logical(SCALE)).background(Color::rgba(255, 0, 0, 128));
    let overlay = Rectangle::new(screen.to_logical(SCALE)).background(Color::rgba(0, 0, 255, 128));
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
    paint.clear();
    paint.push_rectangle(background, screen, ClipId::default());
    paint.push_image(underlay, screen, ClipId::default());
    paint.push_image(partial_image, screen, ClipId::default());
    paint.push_rectangle(overlay, screen, ClipId::default());
    renderer.render(&mut paint);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 14);
    for (rendered, source) in renderer
        .render
        .buffer()
        .pixels()
        .iter()
        .zip(PARTIAL_IMAGE_PIXELS.chunks_exact(4))
    {
        let mut expected = Xrgb8888::default();
        expected.blend(PremultipliedRgbaColor::new(Color::rgba(255, 0, 0, 128), 255));
        expected.blend(PremultipliedRgbaColor::new(Color::BLACK, 128));
        expected.blend(PremultipliedRgbaColor {
            red: source[0],
            green: source[1],
            blue: source[2],
            alpha: source[3],
        });
        expected.blend(PremultipliedRgbaColor::new(Color::rgba(0, 0, 255, 128), 255));
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
    renderer.render(&mut paint);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 18);
}

#[test]
fn box_shadows_match_between_strategies_and_cache_sizes() {
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(strategy: S) -> TestRenderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(128, 96), renderer_config()).strategy(strategy);
        renderer.set_scale(Scale2::uniform(2.0));
        let screen = renderer.render.screen();
        let image = renderer.render.create_image(ImageData::new(
            ImagePixels::Static(&[255, 0, 0, 255]),
            ImageFormat::Rgba8,
            1,
            1,
        ));
        let request = ImageRequest {
            image: image.id(),
            area: LogicalRect::new(0.0, 0.0, 1.0, 1.0),
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let first = BoxShadow::new(
            LogicalRect {
                x: 12.0,
                y: 10.0,
                width: 36.0,
                height: 24.0,
            },
            Color::rgba(220, 40, 20, 180),
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
            color: Color::rgba(20, 80, 220, 140),
            ..first
        };
        let mut paint = DisplayList::default();
        paint.push_box_shadow(first, screen, ClipId::default());
        paint.push_box_shadow(second, screen, ClipId::default());
        paint.push_image(request, screen, ClipId::default());
        renderer.render(&mut paint);
        assert_eq!(renderer.render.context.images.len(), 3);
        assert!(renderer.render.context.images.get(image.id().into()).is_some());
        paint.clear();
        paint.push_box_shadow(
            BoxShadow {
                area: LogicalRect { x: 20.0, ..first.area },
                ..first
            },
            screen,
            ClipId::default(),
        );
        paint.push_rectangle(
            Rectangle::new(first.area).background(Color::WHITE).radius(first.radius),
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
        paint.push_image(request, screen, ClipId::default());
        renderer.render(&mut paint);
        assert_eq!(renderer.render.context.images.len(), 4);
        renderer
    }

    let direct = render(Direct::default());
    let scanline = render(Scanline::default());
    assert_eq!(scanline.render.buffer().pixels(), direct.render.buffer().pixels());
    let pixels = direct.render.buffer().pixels();
    assert_eq!(pixels[44 * 128 + 60].raw(), 0x00ff_ffff);
    assert_ne!(pixels[20 * 128 + 60].raw(), 0x00ff_ffff);
    assert_eq!(pixels[0].raw(), 0x00ff_0000);
}

#[test]
fn gradients_match_between_strategies_and_rounded_clips() {
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(
        strategy: S,
        gradient_background: bool,
    ) -> TestRenderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(48, 36), renderer_config()).strategy(strategy);
        let screen = renderer.render.screen();
        let mut paint = DisplayList::default();
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
                GradientStop::new(0.0, Color::rgba(255, 32, 16, 220)),
                GradientStop::new(0.4, Color::rgba(40, 240, 80, 180)),
                GradientStop::new(1.0, Color::rgba(32, 64, 255, 240)),
            ];
            paint.push_rectangle(
                Rectangle::new(LogicalRect {
                    x: 4.0,
                    y: 3.0,
                    width: 40.0,
                    height: 30.0,
                })
                .background(if gradient_background {
                    Fill::LinearGradient(LinearGradient::new(&stops))
                } else {
                    Color::rgba(20, 24, 32, 210).into()
                })
                .border(Border::gradient(3.0, LinearGradient::new(&stops).angle(135.0)))
                .radius(BorderRadius::uniform(9.0)),
                screen,
                clip,
            );
        }
        renderer.render(&mut paint);
        renderer
    }

    for gradient_background in [false, true] {
        let direct = render(Direct::default(), gradient_background);
        let scanline = render(Scanline::default(), gradient_background);
        assert_eq!(scanline.render.buffer().pixels(), direct.render.buffer().pixels());
        assert_eq!(
            direct.render.buffer().pixels()[18 * 48 + 24].raw(),
            if gradient_background { 0x001c_945a } else { 0x0010_131a }
        );
    }
}

#[test]
fn rounded_clips_match_between_strategies() {
    static PIXEL: [u8; 3] = [0, 255, 0];
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(strategy: S) -> TestRenderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(strategy);
        let image = renderer
            .render
            .create_image(ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgb8, 1, 1));
        let string = renderer.gui.text_run("M", TextStyle::default());
        let screen = renderer.render.screen();
        let area = LogicalRect {
            width: 16.0,
            height: 16.0,
            ..LogicalRect::default()
        };
        let red = Rectangle::new(area).background(Color::rgb(255, 0, 0));
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
            options: TextOptions::default(),
        };

        let mut paint = DisplayList::default();
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
            .background(Color::rgb(0, 0, 255)),
            screen,
            ClipId::default(),
        );
        renderer.render(&mut paint);
        renderer
    }

    let direct = render(Direct::default());
    let scanline = render(Scanline::default());

    assert_eq!(scanline.render.buffer().pixels(), direct.render.buffer().pixels());
    assert_eq!(direct.render.buffer().pixels()[0].raw(), 0);
    let edge = direct.render.buffer().pixels()[6].raw();
    assert!((1..255).contains(&((edge >> 8) & 0xff)));
    let edge = direct.render.buffer().pixels()[9].raw();
    assert!((1..255).contains(&((edge >> 16) & 0xff)));
    assert_eq!(direct.render.buffer().pixels()[15 * 16 + 15].raw(), 0x0000_00ff);
}

#[test]
fn dropped_image_remains_valid_until_frame_end() {
    static PIXEL: [u8; 4] = [255, 0, 0, 255];
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config()).strategy(Scanline::default());
    let image = renderer
        .render
        .create_image(ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgba8, 1, 1));
    let screen = renderer.render.screen();
    let mut paint = DisplayList::default();
    paint.push_clear(screen);
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
        screen,
        ClipId::default(),
    );
    let image_id = image.id();
    drop(image);
    renderer.render(&mut paint);

    assert_eq!(renderer.render.buffer().pixels()[0].raw(), 0x00ff_0000);
    assert!(renderer.render.context.images.get(image_id.into()).is_none());
    assert!(renderer.render.context.images.is_empty());

    let replacement = renderer.render.create_image(ImageData::new(
        ImagePixels::Static(&[0, 255, 0, 255]),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    assert_ne!(replacement.id(), image_id);
    renderer.render(&mut paint);
    assert_eq!(renderer.render.buffer().pixels()[0].raw(), 0);
}

#[test]
fn text_runs_are_keyed_by_content_and_style() {
    let mut gui = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config()).gui;
    let style = TextStyle::default();
    let text = gui.text_run("same", style);
    let spans = [Span::new(0..4).style(SpanStyle::new().color(Color::WHITE))];
    assert_eq!(gui.rich_text("same", &spans, style).0, text,);
    assert_ne!(gui.text_run("changed", style), text);
    assert_ne!(gui.text_run("same", TextStyle { weight: 500, ..style }), text);
}

#[test]
fn tile_hashes_preserve_overlapping_reorders() {
    let mut incremental =
        new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(Scanline::default());
    let mut reference =
        new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(Scanline::default());
    let frames: &[&[u8]] = &[
        &[0, 1, 2],
        &[0, 3, 1, 2, 4],
        &[0, 1, 2],
        &[2, 1, 0],
        &[2, 2, 1, 0],
        &[3, 4, 3, 4, 3, 4, 3, 4, 3, 4, 3, 4, 2, 1, 0],
        &[],
        &[0, 1, 2],
    ];
    for frame in frames {
        reference.render.invalidate_all();
        for renderer in [&mut incremental, &mut reference] {
            let input = renderer.gui.render_input();
            input.display_list.clear();
            input.display_list.push_clear(PhysicalRect::new(0, 0, 16, 16));
            for &index in *frame {
                let rectangle = Rectangle::new(LogicalRect::new(index as f32, index as f32, 10.0, 10.0))
                    .background(Color::rgba(index * 50, 100, 200, 128));
                input
                    .display_list
                    .push_rectangle(rectangle, rectangle.area.to_physical(SCALE), ClipId::default());
            }
            renderer.render.render(input, 1);
            renderer.gui.finish_frame();
        }
        assert_eq!(
            incremental.render.buffer().pixels(),
            reference.render.buffer().pixels(),
            "{frame:?}"
        );
    }
}
