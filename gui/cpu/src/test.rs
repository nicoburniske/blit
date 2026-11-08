use std::ops::Range;

use blit::{LogicalPoint, LogicalRect, Paint, PhysicalRect, Scale2};
use blit_gui::{
    FontData, FontStack, TextConfig, TextSystem,
    color::Color,
    image::{ImageData, ImageFit, ImageFormat, ImagePixels, ImageRequest, ImageSampling, ImageTiling},
    paint::{Mesh, MeshVertex, Rectangle},
    style::{Border, BorderRadius, Fill, GradientStop, LinearGradient},
    text::{
        FontId, HorizontalAlign, Span, SpanStyle, TextLayoutRequest, TextOptions, TextRequest, TextStyle, TextWrap,
        VerticalAlign,
    },
};
use blit_text::{
    Caret, Family, FontError, FontStackId, GlyphImage, GlyphRequest, LayoutLine, LayoutRequest, PreparedGlyph,
    TextLayout, TextLayoutEngine,
};

use super::*;

const SCALE: Scale2 = Scale2::IDENTITY;

#[test]
fn shadow_resources_are_created_only_when_visible() {
    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(32, 20), text_system(), renderer_config())
        .handlers(shadow::Handler::default())
        .strategy(Scanline::default());
    let bounds = LogicalRect::new(0.0, 0.0, 32.0, 20.0);
    let shadow = blit_gui::shadow::BoxShadow::new(LogicalRect::new(8.0, 6.0, 8.0, 6.0), Color::WHITE).blur(2.0);
    for covered in [true, false] {
        renderer.paint(bounds, Rectangle::new(bounds).background(Color::BLACK));
        renderer.paint(shadow.bounds(), shadow);
        assert!(renderer.context.images.is_empty());
        if covered {
            renderer.paint(bounds, Rectangle::new(bounds).background(Color::WHITE));
        }
        renderer.render(1);
        assert_eq!(renderer.context.images.is_empty(), covered);
    }
}

#[test]
fn atoms_record_into_the_renderer_context() {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Input, LogicalSize};
    use blit_gui::{BoundsClip, atom, widget};

    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(64, 48), text_system(), renderer_config())
        .handlers(shadow::Handler::default());
    let mut frame = Frame::default();
    for _ in 0..2 {
        frame.build(
            &mut renderer,
            FrameInfo::new(LogicalSize::new(64.0, 48.0)),
            Duration::ZERO,
            Input::None,
            |ui: blit::Ui<'_, _>| {
                let mut panel = ui.layout(()).clip(BoundsClip);
                panel.insert(atom::Rectangle::new().background(Color::WHITE));
                panel.insert(atom::Shadow::new(Color::BLACK).blur(4.0).inset(true));
                panel.insert(widget::Text::new("paint"));
            },
        );
        frame.layout(&mut renderer);
        frame.paint(&mut renderer);
        renderer.render(1);
        assert!(renderer.damage_input.records.is_empty());
        assert!(renderer.buffer().pixels().iter().any(|pixel| pixel.raw() != 0xffffff));
    }
}

#[test]
fn oversized_borders_fill_rectangle() {
    let area = LogicalRect::new(0.0, 0.0, 8.0, 8.0);
    let stops = [
        GradientStop::new(0.0, Color::WHITE),
        GradientStop::new(1.0, Color::WHITE),
    ];
    for border in [
        Border::solid(f32::MAX, Color::WHITE),
        Border::gradient(f32::MAX, LinearGradient::new(&stops)),
    ] {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config());
        renderer.paint(area, Rectangle::new(area).border(border));
        renderer.render(0);
        assert!(renderer.buffer().pixels().iter().all(|pixel| pixel.raw() == 0xffffff));
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
        renderer.buffer_mut().ranges.clear();

        renderer.paint(
            (PhysicalRect::new(0, 0, 643, 53)).to_logical(SCALE),
            blit_gui::atom::Clear,
        );
        renderer.push_clip(
            LogicalRect::new(0.0, 0.0, if frame == 2 { 635.0 } else { 643.0 }, 53.0),
            BorderRadius::default(),
        );
        for position in 0..40 {
            let index = if frame == 2 { 39 - position } else { position };
            let area = LogicalRect::new((index * 16 + 1) as f32, 1.0, 2.0, 2.0);
            renderer.paint(
                area,
                Rectangle::new(area).background(if frame == 0 { Color::WHITE } else { Color::BLACK }),
            );
        }
        renderer.pop_clip();
        renderer.render(1);

        let touched: usize = renderer.buffer().ranges.iter().map(Range::len).sum();
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
            renderer.paint(
                screen.to_logical(SCALE),
                Rectangle::new(screen.to_logical(SCALE)).background(Color::rgba(255, 0, 0, 128)),
            );
            for region in regions {
                renderer.paint(
                    region.to_logical(SCALE),
                    Rectangle::new(region.to_logical(SCALE))
                        .background(Color::WHITE)
                        .opacity(frame as f32),
                );
            }
            if frame == 0 {
                // seed tracking without drawing to isolate partial rasterization
                renderer.damage.update(&renderer.damage_input, screen, 1.0, 1);
                renderer.damage_input.clear();
                renderer.context.commands.clear();
            } else {
                renderer.render(1);
            }
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
        for (index, pixel) in renderer.buffer().pixels().iter().enumerate() {
            assert_eq!(pixel.raw(), expected[index / 7][index % 7], "pixel {index}");
        }
    }
    check::<Direct>();
    check::<Scanline>();
}

#[test]
fn reused_buffers_receive_every_change_since_their_last_presentation() {
    fn check<const COUNT: usize, D: DamageTracker>(stale: bool, damage: D) {
        let renderer = new_renderer(
            VecBuffer::<Argb8888>::new(37, 21),
            RendererConfig {
                damage_tile_size: 8,
                ..renderer_config()
            },
        )
        .strategy(Scanline::default());
        let mut renderer = renderer.damage_tracker(damage);
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
            std::mem::swap(renderer.buffer_mut(), &mut buffers[index]);
            let left = frame.min(19) % 5 * 7;

            renderer.paint(
                (PhysicalRect::new(0, 0, 37, 21)).to_logical(SCALE),
                blit_gui::atom::Clear,
            );
            let area = LogicalRect::new(left as f32, 2.0, 6.0, 16.0);
            renderer.paint(area, Rectangle::new(area).background(Color::WHITE));
            renderer.render(age);
            let mut expected = [Argb8888::default(); 37 * 21];
            for row in 2..18 {
                expected[row * 37 + left..row * 37 + left + 6].fill(Argb8888::from_raw(u32::MAX));
            }
            assert_eq!(
                renderer.buffer().pixels(),
                expected,
                "buffers {COUNT} stale {stale} frame {frame} age {age}"
            );

            std::mem::swap(renderer.buffer_mut(), &mut buffers[index]);
            presented[index] = Some(frame);
        }
    }
    check::<1, _>(false, TileDamage::new(8));
    check::<1, _>(false, damage::command::CommandDiff::default());
    check::<2, _>(false, TileDamage::new(8));
    check::<2, _>(false, damage::command::CommandDiff::default());
    check::<3, _>(false, TileDamage::new(8));
    check::<3, _>(false, damage::command::CommandDiff::default());
    check::<3, _>(true, TileDamage::new(8));
    check::<3, _>(true, damage::command::CommandDiff::default());
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
        glyph_cache_capacity: 1024 * 1024,
    }
}

fn new_renderer<B: PixelBuffer>(buffer: B, config: RendererConfig) -> Renderer<B> {
    Renderer::new(buffer, text_system(), config)
}

fn text_system() -> TextSystem {
    TextSystem::new(
        TextConfig {
            fonts: vec![FontStack {
                id: FontId::default(),
                families: vec![blit_gui::Family::embedded([FontData::Static(include_bytes!(env!(
                    "BLIT_TEST_FONT"
                )))])],
            }],
            text_cache_capacity: 1024 * 1024,
            layout_cache_capacity: 1024 * 1024,
        },
        blit_text_parley::Backend::without_system_fonts(),
    )
    .unwrap()
}

#[test]
fn renderer_supports_custom_pixel_layouts() {
    let mut renderer = new_renderer(VecBuffer::<BgrPixel>::new(96, 24), renderer_config());
    let clip = renderer.screen();

    renderer.paint(
        clip.to_logical(SCALE),
        Rectangle::new(clip.to_logical(SCALE)).background(Color::rgb(12, 34, 56)),
    );
    renderer.render(0);
    assert_eq!(
        renderer.buffer().pixels()[0],
        BgrPixel {
            blue: 56,
            green: 34,
            red: 12
        }
    );

    let text = renderer
        .text_system
        .rich_text("M M", &[Span::new(0..2), Span::new(2..3)], TextStyle::new().size(20.0));
    for (x, mut colors) in [
        (0.0, [Color::WHITE, Color::rgb(255, 0, 0)]),
        (32.0, [Color::rgb(255, 0, 0), Color::WHITE]),
    ] {
        let request = TextRequest {
            text,
            colors: &colors,
            area: LogicalRect::new(x, 0.0, 32.0, 24.0),
            offset_x: 0.0,
            options: TextOptions::default(),
        };
        renderer.paint(request.area, request);
        colors.fill(Color::TRANSPARENT);
    }
    let request = TextRequest {
        text,
        colors: &[Color::WHITE],
        area: LogicalRect::new(64.0, 0.0, 32.0, 24.0),
        offset_x: 0.0,
        options: TextOptions::default(),
    };
    renderer.paint(request.area, request);
    renderer.render(0);

    let pixels = renderer.buffer().pixels();
    for (column, white) in [true, false, false, true, true, true].into_iter().enumerate() {
        assert!(pixels.chunks_exact(96).any(|row| {
            row[column * 16..(column + 1) * 16]
                .iter()
                .any(|pixel| pixel.red > 12 && (pixel.green > 34) == white)
        }));
    }
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

    renderer.paint(
        renderer.screen().to_logical(SCALE),
        Mesh {
            vertices: &vertices,
            indices: &[0, 1, 2, 2, 1, 3, 4, 5, 6],
        },
    );

    renderer.render(0);

    let expected: [BgrPixel; 128] = renderer.buffer().pixels().try_into().unwrap();

    renderer.paint(
        bounds.to_logical(SCALE),
        Rectangle::new(LogicalRect::new(0.0, 0.0, 16.0, 8.0)).background(Color::BLACK),
    );
    renderer.render(0);
    renderer.paint(
        bounds.to_logical(SCALE),
        Mesh {
            vertices: &vertices,
            indices: &[0, 1, 2, 2, 1, 3, 4, 5, 6],
        },
    );
    renderer.render(0);
    assert_eq!(renderer.buffer().pixels(), expected);

    let pixels = renderer.buffer().pixels();
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
fn recorded_commands_track_damage_and_invalidation() {
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
    let render = |renderer: &mut Renderer<TrackingBuffer, Scanline>| {
        renderer.buffer_mut().lines.clear();
        renderer.paint(LogicalRect::new(0.0, 0.0, 4.0, 4.0), rectangle);
        renderer.render(1);
    };

    render(&mut renderer);
    assert!(!renderer.buffer().lines.is_empty());

    render(&mut renderer);
    assert!(renderer.buffer().lines.is_empty());

    renderer.invalidate_all();
    render(&mut renderer);
    assert!(!renderer.buffer().lines.is_empty());

    renderer.set_scale(2.0);
    render(&mut renderer);
    assert!(!renderer.buffer().lines.is_empty());

    let buffer = renderer.buffer_mut();
    buffer.width = 8;
    buffer.pixels.resize(8 * 4, Xrgb8888::default());
    render(&mut renderer);
    assert!(!renderer.buffer().lines.is_empty());
}

#[test]
fn paint_changes_invalidate_damage_without_changing_bounds() {
    fn check<D: DamageTracker>(damage: impl Fn() -> D) {
        let area = LogicalRect::new(0.0, 0.0, 16.0, 16.0);
        for case in ["fill", "border", "text", "image", "shadow", "mesh", "clip"] {
            let mut renderer = new_renderer(
                TrackingBuffer {
                    pixels: vec![Xrgb8888::default(); 16 * 16],
                    lines: Vec::new(),
                    ranges: Vec::new(),
                    width: 16,
                    height: 16,
                },
                renderer_config(),
            )
            .strategy(Scanline::default())
            .damage_tracker(damage())
            .handlers(shadow::Handler::default());
            let image = renderer.create_image(ImageData::new(
                ImagePixels::Static(&[255, 255, 255]),
                ImageFormat::Rgb8,
                1,
                1,
            ));
            let text = renderer.text().text_run("M", TextStyle::default());
            for (frame, color) in [Color::BLACK, Color::WHITE, Color::WHITE].into_iter().enumerate() {
                renderer.buffer_mut().ranges.clear();
                renderer.paint(area, blit_gui::atom::Clear);
                match case {
                    "fill" | "border" => {
                        let stops = [GradientStop::new(0.0, Color::BLACK), GradientStop::new(1.0, color)];
                        let gradient = LinearGradient::new(&stops);
                        let rectangle = if case == "fill" {
                            Rectangle::new(area).background(gradient)
                        } else {
                            Rectangle::new(area).border(Border::gradient(2.0, gradient))
                        };
                        renderer.paint(area, rectangle);
                    }
                    "text" => renderer.paint(
                        area,
                        TextRequest {
                            text,
                            colors: &[color],
                            area,
                            offset_x: 0.0,
                            options: TextOptions::default(),
                        },
                    ),
                    "image" => renderer.paint(
                        area,
                        ImageRequest {
                            image: image.id(),
                            area,
                            fit: ImageFit::Fill,
                            sampling: ImageSampling::Nearest,
                            opacity: if frame == 0 { 0.5 } else { 1.0 },
                            colorize: None,
                            nine_slice: None,
                            horizontal_tiling: ImageTiling::None,
                            vertical_tiling: ImageTiling::None,
                        },
                    ),
                    "shadow" => renderer.paint(
                        area,
                        blit_gui::shadow::BoxShadow::new(area, color).blur(2.0).inset(true),
                    ),
                    "mesh" => renderer.paint(
                        area,
                        Mesh {
                            vertices: &[
                                MeshVertex::new(0.0, 0.0, color),
                                MeshVertex::new(16.0, 0.0, Color::WHITE),
                                MeshVertex::new(0.0, 16.0, Color::WHITE),
                            ],
                            indices: &[0, 1, 2],
                        },
                    ),
                    "clip" => {
                        renderer.push_clip(area, BorderRadius::uniform(if frame == 0 { 0.0 } else { 4.0 }));
                        renderer.paint(area, Rectangle::new(area).background(Color::WHITE));
                        renderer.pop_clip();
                    }
                    _ => unreachable!(),
                }
                renderer.render(1);
                assert_eq!(renderer.buffer().ranges.is_empty(), frame == 2, "{case} frame {frame}");
            }
        }
    }
    check(|| TileDamage::new(8));
    check(damage::command::CommandDiff::default);
}

#[test]
fn text_placement_tracks_physical_clip_size() {
    let config = renderer_config();
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(16, 24), config);
    for (x, y) in [(0.0, 0.0), (0.5, 0.0), (0.0, 0.5)] {
        let mut fresh = new_renderer(VecBuffer::<Xrgb8888>::new(16, 24), config);
        for renderer in [&mut renderer, &mut fresh] {
            let text =
                renderer
                    .text_system
                    .rich_text("MM", &[Span::new(0..1), Span::new(1..2)], TextStyle::new().size(20.0));
            let request = TextRequest {
                text,
                colors: &[Color::rgb(255, 0, 0), Color::WHITE],
                area: LogicalRect::new(x, y, 8.0, 8.0),
                offset_x: 12.0,
                options: TextOptions::default(),
            };
            let screen = renderer.screen();
            renderer.paint(screen.to_logical(SCALE), blit_gui::atom::Clear);
            renderer.paint(request.area, request);
            renderer.render(0);
        }
        assert_eq!(renderer.buffer().pixels(), fresh.buffer().pixels());
        assert!(fresh.buffer().pixels().iter().any(|pixel| pixel.raw() & 0xff00 != 0));
    }
}

#[test]
fn text_cache_stages_are_independent() {
    use std::{cell::Cell, rc::Rc};

    for (text_cache_capacity, layout_cache_capacity) in [(0, 0), (0, 1024), (1024, 0), (1024, 1024)] {
        let source_evicted = text_cache_capacity == 0;
        let counts = Rc::new(TextCounts::default());
        let mut text = TextSystem::new(
            TextConfig {
                fonts: vec![FontStack {
                    id: FontId::default(),
                    families: Vec::new(),
                }],
                text_cache_capacity,
                layout_cache_capacity,
            },
            CountingBackend(counts.clone()),
        )
        .unwrap();
        let mut request = TextLayoutRequest {
            text: text.text_run("cached", TextStyle::default()),
            wrap: TextWrap::None,
            max_width: None,
            max_lines: None,
        };

        text.measure(&request);
        let white = text.text_run("cached", TextStyle::new().color(Color::WHITE));
        text.measure(&TextLayoutRequest { text: white, ..request });

        let mut paint = TextRequest {
            text: request.text,
            colors: &[Color::BLACK],
            area: LogicalRect::new(0.0, 0.0, 1.0, 1.0),
            offset_x: 0.0,
            options: TextOptions::default(),
        };
        text.cursor_rect(&paint, 0, 0.0);
        paint.offset_x = 10.0;
        paint.area.width = 10.0;
        paint.area.height = 10.0;
        paint.options.horizontal_align = HorizontalAlign::Center;
        paint.options.vertical_align = VerticalAlign::Bottom;
        text.cursor_rect(&paint, 0, 0.0);
        assert_eq!(counts.shapes.get(), 1);
        assert_eq!(counts.layouts.get(), 1);
        assert_eq!(counts.carets.get(), 1);

        text.finish_frame();
        request.text = text.text_run("cached", TextStyle::default());
        assert_eq!(request.text == paint.text, !source_evicted);
        text.measure(&request);
        let shapes = if source_evicted { 2 } else { 1 };
        let layouts = if source_evicted || layout_cache_capacity == 0 {
            2
        } else {
            1
        };
        assert_eq!(counts.shapes.get(), shapes);
        assert_eq!(counts.layouts.get(), layouts);

        for width in [10.0, 20.0] {
            text.measure(&TextLayoutRequest {
                wrap: TextWrap::Word,
                max_width: Some(width),
                ..request
            });
        }
        assert_eq!(counts.shapes.get(), shapes);
        assert_eq!(counts.layouts.get(), layouts + 2);

        paint.text = request.text;
        for capacity in [1024, 0] {
            let mut glyphs = crate::glyph::GlyphCache::new(capacity);
            let mut previous = None;
            for _ in 0..2 {
                let mut resolved = text.paint_layout(&paint);
                let id = resolved
                    .prepare_glyph(GlyphRequest {
                        font: 0,
                        glyph: 0,
                        scale: 1.0,
                        origin: LogicalPoint::default(),
                    })
                    .key;
                if let Some(previous) = previous {
                    assert_eq!(id == previous, capacity != 0);
                }
                assert!(glyphs.glyph(&mut resolved, id).is_some());
                previous = Some(id);
                glyphs.finish_frame(&mut text);
            }
        }
    }

    #[derive(Default)]
    struct TextCounts {
        shapes: Cell<usize>,
        layouts: Cell<usize>,
        carets: Cell<usize>,
    }

    struct CountingBackend(Rc<TextCounts>);

    impl TextLayoutEngine for CountingBackend {
        type Shape = ();
        type GlyphKey = ();

        fn register_stack(&mut self, _families: &[Family]) -> Result<FontStackId, FontError> {
            Ok(FontStackId(1))
        }

        fn prepare_glyph(&mut self, _shape: &(), _request: GlyphRequest) -> PreparedGlyph<()> {
            PreparedGlyph {
                key: (),
                origin: blit::PhysicalPoint::default(),
            }
        }

        fn rasterize_glyph(&mut self, _key: &()) -> Result<GlyphImage, FontError> {
            Ok(GlyphImage::default())
        }

        fn shape(&mut self, _text: blit_text::Text<'_>) -> ((), usize) {
            self.0.shapes.set(self.0.shapes.get() + 1);
            ((), 0)
        }

        fn layout(&mut self, _shape: &mut (), _text: &str, _request: LayoutRequest) -> TextLayout {
            self.0.layouts.set(self.0.layouts.get() + 1);
            TextLayout {
                lines: Box::new([LayoutLine {
                    bounds: LogicalRect::default(),
                    text: 0..0,
                }]),
                ..TextLayout::default()
            }
        }

        fn carets(&mut self, _shape: &mut (), _text: &str, _request: LayoutRequest, _line: usize) -> Box<[Caret]> {
            self.0.carets.set(self.0.carets.get() + 1);
            Box::new([Caret {
                byte_offset: 0,
                position: LogicalPoint::default(),
                height: 0.0,
            }])
        }
    }
}

#[test]
fn text_measurement_and_carets_follow_layout() {
    let mut gui = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config()).text_system;
    let request = TextLayoutRequest {
        text: gui.text_run("hello world", TextStyle::default()),
        wrap: TextWrap::None,
        max_width: None,
        max_lines: None,
    };
    let unwrapped = gui.measure(&request);
    let spans = [
        Span::new(0..6).style(SpanStyle::new().size(16.0)),
        Span::new(6..11).style(SpanStyle::new().size(32.0)),
    ];
    let mixed = gui.rich_text("hello world", &spans, TextStyle::default());
    let mixed_size = gui.measure(&TextLayoutRequest { text: mixed, ..request });
    assert!(mixed_size.height > unwrapped.height);
    let larger_base = gui.rich_text("hello world", &spans, TextStyle::new().size(48.0));
    assert_ne!(mixed, larger_base);
    let larger_size = gui.measure(&TextLayoutRequest {
        text: larger_base,
        ..request
    });
    assert!((larger_size.height - 48.0 * 1.2).abs() < 0.001);

    let wrapped_request = TextLayoutRequest {
        wrap: TextWrap::Word,
        max_width: Some(unwrapped.width / 2.0),
        ..request
    };
    let wrapped = gui.measure(&wrapped_request);
    assert!(wrapped.width < unwrapped.width);
    assert!(wrapped.height > unwrapped.height);
    assert_eq!(
        gui.measure(&TextLayoutRequest {
            max_lines: Some(1),
            ..wrapped_request
        })
        .height,
        unwrapped.height
    );

    let mut paint = TextRequest {
        text: request.text,
        colors: &[Color::BLACK],
        area: LogicalRect::new(10.0, 20.0, 120.0, 48.0),
        offset_x: 0.0,
        options: TextOptions::default(),
    };
    let start = gui.cursor_rect(&paint, 0, 0.5);
    assert_eq!(start.width, 0.5);
    assert_eq!(
        gui.offset_at_position(&paint, LogicalPoint::new(1000.0, 20.0)),
        "hello world".len()
    );
    paint.offset_x = 4.0;
    assert_eq!(gui.cursor_rect(&paint, 0, 0.5).x, start.x - 4.0);
    paint.area.width += 0.5;
    paint.area.height += 0.25;
    paint.options.horizontal_align = HorizontalAlign::Center;
    paint.options.vertical_align = VerticalAlign::Bottom;
    let placed = gui.cursor_rect(&paint, 0, 0.5);
    assert!((placed.x - (start.x - 4.0 + (paint.area.width - unwrapped.width) / 2.0)).abs() < 0.001);
    assert!((placed.y - (start.y + paint.area.height - unwrapped.height)).abs() < 0.001);
    let end = gui.cursor_rect(&paint, "hello world".len(), 0.5);
    assert_eq!(
        gui.offset_at_position(&paint, LogicalPoint::new(end.x, end.y)),
        "hello world".len()
    );
}

#[test]
fn clear_resets_stale_pixels_before_drawing() {
    fn render<S: RenderStrategy<VecBuffer<Argb8888>>>(strategy: S, clear: bool, stale: Argb8888) -> Vec<Argb8888> {
        let mut renderer = new_renderer(VecBuffer::<Argb8888>::new(12, 10), renderer_config()).strategy(strategy);
        renderer.buffer_mut().pixels_mut().fill(stale);
        let screen = renderer.screen();
        let rectangle = Rectangle::new(LogicalRect {
            width: 12.0,
            height: 10.0,
            ..LogicalRect::default()
        })
        .background(Color::rgba(40, 120, 220, 144))
        .border(Border::solid(2.0, Color::rgba(240, 80, 30, 192)))
        .radius(BorderRadius::uniform(5.0));

        if clear {
            renderer.paint(screen.to_logical(SCALE), blit_gui::atom::Clear);
        }
        renderer.paint(screen.to_logical(SCALE), rectangle);
        renderer.render(0);
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

    let mut renderer = new_renderer(VecBuffer::<Argb8888>::new(12, 10), renderer_config());
    renderer.buffer_mut().pixels_mut().fill(stale);
    let screen = renderer.screen();

    renderer.paint(screen.to_logical(SCALE), blit_gui::atom::Clear);
    renderer.render(0);
    assert!(renderer.buffer().pixels().iter().all(|pixel| *pixel == transparent));
}

#[test]
fn dropped_image_is_removed_after_last_handle() {
    static PIXEL: [u8; 4] = [255, 255, 255, 255];
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config());
    let texture = ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgba8, 1, 1);

    let first = renderer.create_image(texture);
    let retained = first.clone();
    let first_id = first.id();
    renderer.render(0);
    drop(first);
    renderer.render(0);
    assert!(renderer.context.images.get(first_id.into()).is_some());

    drop(retained);
    renderer.render(0);
    assert!(renderer.context.images.get(first_id.into()).is_none());

    let replacement = renderer.create_image(ImageData::new(
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
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Owned(pixels.into()),
        ImageFormat::Rgba8Premultiplied,
        6,
        4,
    ));
    renderer.render(0);
    let key = image.id();
    let rows = &renderer.context.images.get(key.into()).unwrap().alpha_rows;
    let rows: [_; 4] = std::array::from_fn(|index| rows.get(ImageFormat::Rgba8Premultiplied, index).unwrap());
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([4, 4, 6, 6]));
    assert!(rows.iter().map(|row| row.opaque_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.opaque_end).eq([4, 4, 2, 2]));
    assert!(renderer.context.images.get(key.into()).unwrap().has_opaque_spans);
    assert!(!renderer.context.images.get(key.into()).unwrap().opaque);

    let screen = renderer.screen();
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

    renderer.paint(screen.to_logical(SCALE), request);
    renderer.render(0);
    assert_eq!(COPIED.load(Ordering::Relaxed), 10);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 8);

    BLENDED.store(0, Ordering::Relaxed);
    COPIED.store(0, Ordering::Relaxed);

    renderer.paint(
        screen.to_logical(SCALE),
        ImageRequest {
            opacity: 0.5,
            ..request
        },
    );
    renderer.render(0);
    assert_eq!(COPIED.load(Ordering::Relaxed), 0);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 18);

    let image = renderer.create_image(ImageData::new(
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
    renderer.render(0);
    let key = image.id();
    let rows = &renderer.context.images.get(key.into()).unwrap().alpha_rows;
    let rows: [_; 4] = std::array::from_fn(|index| rows.get(ImageFormat::Alpha8(Color::WHITE), index).unwrap());
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 0, 1, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([3, 0, 4, 6]));
    assert!(rows.iter().all(|row| row.opaque_start == 0 && row.opaque_end == 0));
    BLENDED.store(0, Ordering::Relaxed);

    renderer.paint(
        screen.to_logical(SCALE),
        ImageRequest {
            image: image.id(),
            opacity: 1.0,
            ..request
        },
    );
    renderer.render(0);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 11);

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[255; 6 * 4]),
        ImageFormat::Alpha8(Color::WHITE),
        6,
        4,
    ));
    renderer.render(0);
    let key = image.id();
    let image = &renderer.context.images.get(key.into()).unwrap();
    assert!(image.opaque);
    assert!(image.alpha_rows.get(ImageFormat::Alpha8(Color::WHITE), 0).is_none());

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[255; 6 * 4 * 4]),
        ImageFormat::Rgba8Premultiplied,
        6,
        4,
    ));
    renderer.render(0);
    let key = image.id();
    let image = &renderer.context.images.get(key.into()).unwrap();
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
    let screen = renderer.screen();
    let area = LogicalRect {
        width: 4.0,
        height: 2.0,
        ..LogicalRect::default()
    };

    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area).background(Color::rgba(255, 0, 0, 128)),
    );
    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area).background(Color::rgb(0, 255, 0)),
    );
    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area).background(Color::rgba(0, 0, 255, 128)),
    );
    renderer.render(0);

    assert_eq!(SOLID_PAIRS.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert!(renderer.buffer().pixels().iter().all(|pixel| pixel.draws == 2));

    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(8, 7), renderer_config()).strategy(Scanline::default());
    let screen = renderer.screen();
    let area = LogicalRect {
        width: 8.0,
        height: 7.0,
        ..LogicalRect::default()
    };

    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area).background(Color::rgba(255, 0, 0, 128)),
    );
    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area)
            .background(Color::rgb(0, 255, 0))
            .radius(BorderRadius::uniform(3.0)),
    );
    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area).background(Color::rgba(0, 0, 255, 128)),
    );
    renderer.render(0);

    assert!(
        renderer.buffer().pixels()[3 * 8..4 * 8]
            .iter()
            .all(|pixel| pixel.draws == 2)
    );

    static IMAGE_PIXEL: [u8; 4] = [0, 255, 0, 255];
    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(4, 2), renderer_config()).strategy(Scanline::default());
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

    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area).background(Color::rgba(255, 0, 0, 128)),
    );
    renderer.paint(screen.to_logical(SCALE), image);
    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area).background(Color::rgba(0, 0, 255, 128)),
    );
    renderer.render(0);

    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 8);

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

    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area).background(Color::rgba(255, 0, 0, 128)),
    );
    renderer.paint(screen.to_logical(SCALE), transparent_image);
    renderer.paint(
        screen.to_logical(SCALE),
        Rectangle::new(area).background(Color::rgba(0, 0, 255, 128)),
    );
    renderer.render(0);

    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 16);

    static PARTIAL_IMAGE_PIXELS: [u8; 24] = [
        0, 0, 0, 0, 0, 128, 0, 128, 0, 255, 0, 255, 0, 255, 0, 255, 0, 128, 0, 128, 0, 0, 0, 0,
    ];
    static UNDERLAY_ALPHA: [u8; 1] = [128];
    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(6, 1), renderer_config()).strategy(Scanline::default());
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

    renderer.paint(screen.to_logical(SCALE), background);
    renderer.paint(screen.to_logical(SCALE), underlay);
    renderer.paint(screen.to_logical(SCALE), partial_image);
    renderer.paint(screen.to_logical(SCALE), overlay);
    renderer.render(0);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 14);
    for (rendered, source) in renderer
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

    renderer.paint(screen.to_logical(SCALE), background);
    renderer.paint(screen.to_logical(SCALE), underlay);
    renderer.paint(
        screen.to_logical(SCALE),
        ImageRequest {
            opacity: 0.5,
            ..partial_image
        },
    );
    renderer.paint(screen.to_logical(SCALE), overlay);
    renderer.render(0);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 18);

    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);

    renderer.paint(screen.to_logical(SCALE), background);
    renderer.paint(screen.to_logical(SCALE), partial_image);
    renderer.paint(screen.to_logical(SCALE), overlay);
    renderer.render(0);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 10);
}

#[test]
fn gradients_match_between_strategies_and_rounded_clips() {
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(
        strategy: S,
        gradient_background: bool,
    ) -> Renderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(48, 36), renderer_config()).strategy(strategy);
        let screen = renderer.screen();

        renderer.push_clip(
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
            renderer.paint(
                screen.to_logical(SCALE),
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
            );
        }
        renderer.pop_clip();
        renderer.render(0);
        renderer
    }

    for gradient_background in [false, true] {
        let direct = render(Direct::default(), gradient_background);
        let scanline = render(Scanline::default(), gradient_background);
        assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
        assert_eq!(
            direct.buffer().pixels()[18 * 48 + 24].raw(),
            if gradient_background { 0x001c_945a } else { 0x0010_131a }
        );
    }
}

#[test]
fn rounded_clips_match_between_strategies() {
    static PIXEL: [u8; 3] = [0, 255, 0];
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(strategy: S) -> Renderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(strategy);
        let image = renderer.create_image(ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgb8, 1, 1));
        let string = renderer.text_system.text_run("M", TextStyle::new());
        let screen = renderer.screen();
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
            colors: &[Color::WHITE],
            area,
            offset_x: 0.0,
            options: TextOptions::default(),
        };

        renderer.push_clip(
            area,
            BorderRadius {
                top_left: 8.0,
                top_right: 8.0,
                bottom_right: 8.0,
                bottom_left: 8.0,
            },
        );
        renderer.paint(screen.to_logical(SCALE), red);
        renderer.push_clip(
            LogicalRect {
                width: 8.0,
                height: 8.0,
                ..area
            },
            BorderRadius::default(),
        );
        renderer.paint(screen.to_logical(SCALE), image);
        renderer.paint(screen.to_logical(SCALE), text);
        renderer.pop_clip();
        renderer.pop_clip();
        renderer.paint(
            screen.to_logical(SCALE),
            Rectangle::new(LogicalRect {
                x: 15.0,
                y: 15.0,
                width: 1.0,
                height: 1.0,
            })
            .background(Color::rgb(0, 0, 255)),
        );
        renderer.render(0);
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
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config()).strategy(Scanline::default());
    let image = renderer.create_image(ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgba8, 1, 1));
    let screen = renderer.screen();

    renderer.paint(screen.to_logical(SCALE), blit_gui::atom::Clear);
    renderer.paint(
        screen.to_logical(SCALE),
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
    );
    let image_id = image.id();
    drop(image);
    renderer.render(0);

    assert_eq!(renderer.buffer().pixels()[0].raw(), 0x00ff_0000);
    assert!(renderer.context.images.get(image_id.into()).is_none());
    assert!(renderer.context.images.is_empty());

    let replacement = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[0, 255, 0, 255]),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    assert_ne!(replacement.id(), image_id);
    renderer.paint(screen.to_logical(SCALE), blit_gui::atom::Clear);
    renderer.render(0);
    assert_eq!(renderer.buffer().pixels()[0].raw(), 0);
}

#[test]
fn text_runs_are_keyed_by_content_and_style() {
    let mut gui = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config()).text_system;
    let style = TextStyle::default();
    let text = gui.text_run("same", style);
    let white = gui.text_run("same", style.color(Color::WHITE));
    assert_eq!(
        gui.rich_text(
            "same",
            &[Span::new(0..4).style(SpanStyle::new().color(Color::WHITE))],
            style.color(Color::TRANSPARENT),
        ),
        white
    );
    assert_eq!(white, text);
    assert_ne!(gui.text_run("changed", style), text);
    assert_ne!(gui.text_run("same", TextStyle { weight: 500, ..style }), text);
    use blit_gui::text::{FontSettings, OpticalSize};
    for settings in [
        FontSettings {
            features: &[(*b"liga", 0)],
            ..FontSettings::default()
        },
        FontSettings {
            variations: &[(*b"wght", 550.0)],
            ..FontSettings::default()
        },
        FontSettings {
            language: Some("tr"),
            ..FontSettings::default()
        },
        FontSettings {
            optical_size: OpticalSize::FontDefault,
            ..FontSettings::default()
        },
    ] {
        let configured = gui.text_run("same", style.settings(settings));
        assert_ne!(configured, text);
        assert_eq!(
            gui.rich_text(
                "same",
                &[Span::new(0..4).style(SpanStyle::new().settings(settings))],
                style.settings(settings)
            ),
            configured
        );
    }
    let owned = {
        let language = String::from("fr");
        gui.text_run(
            "same",
            style.settings(FontSettings {
                language: Some(&language),
                ..FontSettings::default()
            }),
        )
    };
    assert_eq!(
        gui.text_run(
            "same",
            style.settings(FontSettings {
                language: Some("fr"),
                ..FontSettings::default()
            })
        ),
        owned
    );
}

#[test]
fn damage_trackers_preserve_overlapping_reorders() {
    fn check<D: DamageTracker>(make_damage: impl Fn() -> D) {
        let incremental =
            new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(Scanline::default());
        let mut incremental = incremental.damage_tracker(make_damage());
        let reference =
            new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(Scanline::default());
        let mut reference = reference.damage_tracker(make_damage());
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
            reference.invalidate_all();
            for renderer in [&mut incremental, &mut reference] {
                renderer.paint(
                    (PhysicalRect::new(0, 0, 16, 16)).to_logical(SCALE),
                    blit_gui::atom::Clear,
                );
                for &index in *frame {
                    let rectangle = Rectangle::new(LogicalRect::new(index as f32, index as f32, 10.0, 10.0))
                        .background(Color::rgba(index * 50, 100, 200, 128));
                    renderer.paint(rectangle.area, rectangle);
                }
                renderer.render(1);
            }
            assert_eq!(incremental.buffer().pixels(), reference.buffer().pixels(), "{frame:?}");
        }
    }
    check(|| TileDamage::new(8));
    check(damage::command::CommandDiff::default);
}
