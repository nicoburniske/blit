use std::ops::Range;

use crate::{
    color::Color,
    command_list::{BoxShadow, ClipId, CommandList, Rectangle},
    image::{
        ImageData, ImageFit, ImageFormat, ImagePixels, ImageRequest, ImageSampling, ImageTiling,
    },
    style::{Border, BorderRadius, GradientStop, LinearGradient},
    text_types::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextStyle, TextWrap},
};
use blit::{LogicalPoint, LogicalRect, PhysicalRect, Scale2};

use super::*;

const SCALE: Scale2 = Scale2::IDENTITY;

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
        fonts: Vec::new(),
        glyph_cache_capacity: 1024 * 1024,
        shadow_cache_capacity: 1024 * 1024,
    }
}

fn new_renderer<B: PixelBuffer>(buffer: B, mut config: RendererConfig) -> Renderer<B> {
    let mut text = TextSystem::new(CosmicBackend::without_system_fonts());
    let font = text
        .register_font(FontData::Static(include_bytes!(env!("BLIT_TEST_FONT"))), 0)
        .unwrap();
    config.fonts.push(FontFace {
        id: FontId::default(),
        weight: 400,
        font,
    });
    Renderer::new(buffer, config, text)
}

#[test]
fn renderer_supports_custom_pixel_layouts() {
    let mut renderer = new_renderer(VecBuffer::<BgrPixel>::new(32, 24), renderer_config());
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

    renderer.set_scale(Scale2::uniform(2.0));
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
    assert_eq!(start.width, 0.5);
    assert!(end.x > start.x);
}

#[test]
fn text_measurement_reports_wrapped_layout_size() {
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(32, 24), renderer_config());
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
            new_renderer(VecBuffer::<Argb8888>::new(12, 10), renderer_config()).strategy(strategy);
        renderer.buffer_mut().pixels_mut().fill(stale);
        let screen = renderer.screen();
        let rectangle = Rectangle::new(LogicalRect {
            width: 12.0,
            height: 10.0,
            ..LogicalRect::default()
        })
        .background(Color::from_rgba8(40, 120, 220, 144))
        .border(Border::solid(2.0, Color::from_rgba8(240, 80, 30, 192)))
        .radius(BorderRadius::uniform(5.0));
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

    let mut renderer = new_renderer(VecBuffer::<Argb8888>::new(12, 10), renderer_config());
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
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(8, 4), renderer_config());
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
        outside.to_physical(SCALE),
        ClipId::default(),
    );
    paint.push_rectangle(
        Rectangle::new(damaged).background(Color::WHITE),
        damaged.to_physical(SCALE),
        ClipId::default(),
    );

    renderer.render(&paint, &[damaged.to_physical(SCALE)]);

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
fn dropped_image_is_removed_after_last_handle() {
    static PIXEL: [u8; 4] = [255, 255, 255, 255];
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config());
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
    let mut renderer = new_renderer(VecBuffer::<TrackingPixel>::new(6, 4), renderer_config())
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
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(4, 4), renderer_config());
    let screen = renderer.screen();
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(screen.to_logical(SCALE)).background(Color::from_rgba8(255, 0, 0, 128)),
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
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(3, 3), renderer_config());
    let screen = renderer.screen();
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(screen.to_logical(SCALE)).background(Color::WHITE),
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
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(9, 1), renderer_config());
    let screen = renderer.screen();
    let damage: [PhysicalRect; 9] = std::array::from_fn(|x| PhysicalRect {
        x: x as i32,
        y: 0,
        width: 1,
        height: 1,
    });
    let mut paint = CommandList::default();
    paint.push_rectangle(
        Rectangle::new(screen.to_logical(SCALE)).background(Color::WHITE),
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
    let mut renderer = new_renderer(
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
    let mut renderer = new_renderer(
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

        fn blend_solid_pair(
            pixels: &mut [Self],
            first: PremultipliedRgbaColor,
            second: PremultipliedRgbaColor,
        ) {
            SOLID_PAIRS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Self::blend_slice(pixels, first);
            Self::blend_slice(pixels, second);
        }
    }

    let mut renderer = new_renderer(VecBuffer::<CountingPixel>::new(4, 2), renderer_config())
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

    assert_eq!(SOLID_PAIRS.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert!(
        renderer
            .buffer()
            .pixels()
            .iter()
            .all(|pixel| pixel.draws == 2)
    );

    let mut renderer = new_renderer(VecBuffer::<CountingPixel>::new(8, 7), renderer_config())
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
            .radius(BorderRadius::uniform(3.0)),
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
    let mut renderer = new_renderer(VecBuffer::<CountingPixel>::new(4, 2), renderer_config())
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
    let mut renderer = new_renderer(VecBuffer::<CountingPixel>::new(6, 1), renderer_config())
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
    let background =
        Rectangle::new(screen.to_logical(SCALE)).background(Color::from_rgba8(255, 0, 0, 128));
    let overlay =
        Rectangle::new(screen.to_logical(SCALE)).background(Color::from_rgba8(0, 0, 255, 128));
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
    let mut direct = new_renderer(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config());
    let mut scanline = new_renderer(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config())
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
        let mut renderer =
            new_renderer(VecBuffer::<Xrgb8888>::new(128, 96), renderer_config()).strategy(strategy);
        renderer.set_scale(Scale2::uniform(2.0));
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
            new_renderer(VecBuffer::<Xrgb8888>::new(48, 36), renderer_config()).strategy(strategy);
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
                .border(Border::gradient(
                    3.0,
                    LinearGradient::new(&stops).angle(135.0),
                ))
                .radius(BorderRadius::uniform(9.0)),
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
            new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(strategy);
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
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config())
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
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(32, 24), renderer_config());
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
