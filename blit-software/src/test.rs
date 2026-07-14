use super::*;
use blit::{
    Color, ImageFormat, ImagePixels, LogicalRect, TextOptions, TextStyle,
    widgets::{
        BorderRadius, BoxShadow, BoxShadowRequest, GradientStop, ImageFit, ImageRequest,
        ImageSampling, LinearGradient, Rectangle,
    },
};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
struct BgrPixel {
    blue: u8,
    green: u8,
    red: u8,
}

impl Pixel for BgrPixel {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
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
    renderer.begin_frame(&[clip]);
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
    renderer.end_frame();
    assert_eq!(
        renderer.buffer().pixels()[0],
        BgrPixel {
            blue: 56,
            green: 34,
            red: 12,
        }
    );

    renderer.begin_frame(&[clip]);
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
    renderer.end_frame();
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

    renderer.end_frame();
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
    renderer.begin_frame(&damage);
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
    renderer.end_frame();

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
    renderer.begin_frame(&damage);
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
    renderer.end_frame();

    assert_eq!(renderer.buffer().ranges.len(), 1);
    assert_eq!(renderer.buffer().ranges[0], 1..3);
    assert_eq!(
        renderer.buffer().pixels,
        [0, 0xffffff, 0xffffff, 0, 0, 0, 0, 0]
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
    direct.begin_frame(&damage);
    scanline.begin_frame(&damage);

    direct.draw_rectangle(&red, clip);
    direct.draw_rectangle(&green, clip);
    direct.end_frame();
    scanline.draw_rectangle(&red, clip);
    scanline.draw_rectangle(&green, clip);
    scanline.end_frame();

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
        renderer.begin_frame(&[screen]);
        renderer.draw_box_shadow(&first, screen);
        renderer.draw_box_shadow(&second, screen);
        renderer.end_frame();
        assert_eq!(renderer.context.images.len(), 1);
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
        let mut renderer =
            Renderer::new(VecBuffer::<u32>::new(48, 36), renderer_config()).strategy(strategy);
        let screen = renderer.screen();
        renderer.begin_frame(&[screen]);
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
        renderer.end_frame();
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

        renderer.begin_frame(&[screen]);
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
        renderer.end_frame();
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
    renderer.begin_frame(&damage);
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
    renderer.end_frame();

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
    renderer.begin_frame(&damage);
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
    renderer.end_frame();

    assert!(renderer.buffer().pixels().iter().any(|pixel| *pixel != 0));
}
