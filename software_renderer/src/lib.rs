mod image;
mod pixel;
mod rectangle;
mod strategy;
mod text;

pub use fontdue::{Font, FontSettings};
pub use pixel::{Pixel, PixelBuffer, PremultipliedRgbaColor, VecBuffer};
pub use strategy::{Direct, RenderStrategy, Scanline};

use bullseye::{
    FontId, FontWeight, ImageData, ImageId, LogicalPoint, LogicalRect, PhysicalRect, TextRequest,
    widgets::{ImageRequest, Rectangle},
};
use slotmap::{Key, KeyData, SlotMap, new_key_type};
use text::TextRenderer;

new_key_type! {
    struct RendererImageId;
}

pub struct FontFace {
    pub id: FontId,
    pub weight: FontWeight,
    pub font: Font,
}

pub struct RendererConfig {
    pub fonts: Vec<FontFace>,
    pub glyph_cache_capacity: usize,
    pub paragraph_cache_capacity: usize,
}

impl RendererConfig {
    pub fn new(font: Font) -> Self {
        Self {
            fonts: vec![FontFace {
                id: FontId::default(),
                weight: FontWeight::Normal,
                font,
            }],
            glyph_cache_capacity: 1024 * 1024,
            paragraph_cache_capacity: 1024 * 1024,
        }
    }
}

pub struct Renderer<B: PixelBuffer, S: RenderStrategy<B> = Direct> {
    context: RenderContext<B>,
    strategy: S,
}

#[doc(hidden)]
pub struct RenderContext<B: PixelBuffer> {
    buffer: B,
    scale_factor: f32,
    images: SlotMap<RendererImageId, StoredImage>,
    has_dead_images: bool,
    text: TextRenderer,
}

struct StoredImage {
    data: ImageData,
    live: bool,
}

impl<B: PixelBuffer> Renderer<B, Direct> {
    pub fn new(buffer: B, config: RendererConfig) -> Self {
        Self {
            context: RenderContext {
                buffer,
                scale_factor: 1.0,
                images: SlotMap::with_key(),
                has_dead_images: false,
                text: TextRenderer::new(config),
            },
            strategy: Direct,
        }
    }

    pub fn strategy<T: RenderStrategy<B>>(self, strategy: T) -> Renderer<B, T> {
        Renderer {
            context: self.context,
            strategy,
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>> Renderer<B, S> {
    pub fn with_scale_factor(mut self, scale_factor: f32) -> Self {
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        self.context.scale_factor = scale_factor;
        self
    }

    pub fn screen(&self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: self.context.buffer.width() as i32,
            height: self.context.buffer.height() as i32,
        }
    }

    pub fn scale_factor(&self) -> f32 {
        self.context.scale_factor
    }

    pub fn begin_frame(&mut self) {
        self.strategy.begin_frame(&mut self.context)
    }

    pub fn draw_rectangle(&mut self, request: &Rectangle, clips: &[PhysicalRect]) {
        self.strategy
            .draw_rectangle(&mut self.context, request, clips)
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageId {
        data.validate();
        let image = self.context.images.insert(StoredImage { data, live: true });
        ImageId(image.data().as_ffi())
    }

    pub fn drop_image(&mut self, image: ImageId) {
        let image = RendererImageId::from(KeyData::from_ffi(image.0));
        if let Some(image) = self.context.images.get_mut(image) {
            image.live = false;
            self.context.has_dead_images = true;
        }
    }

    pub fn draw_image(&mut self, request: &ImageRequest, clips: &[PhysicalRect]) {
        self.strategy.draw_image(&mut self.context, request, clips)
    }

    pub fn end_frame(&mut self) {
        self.strategy.end_frame(&mut self.context);
        self.context.finish_frame();
    }

    pub fn draw_text(&mut self, request: &TextRequest<'_>, clips: &[PhysicalRect]) {
        self.strategy.draw_text(&mut self.context, request, clips)
    }

    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize {
        self.context
            .text
            .offset_at_position(request, position, self.context.scale_factor)
    }

    pub fn text_cursor_rect(
        &mut self,
        request: &TextRequest<'_>,
        byte_offset: usize,
    ) -> LogicalRect {
        self.context
            .text
            .cursor_rect(request, byte_offset, self.context.scale_factor)
    }

    pub fn buffer(&self) -> &B {
        &self.context.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut B {
        &mut self.context.buffer
    }
}

impl<B: PixelBuffer> RenderContext<B> {
    fn finish_frame(&mut self) {
        self.text.finish_frame();
        if self.has_dead_images {
            self.images.retain(|_, image| image.live);
            self.has_dead_images = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bullseye::{
        Color, ImageFormat, ImagePixels, LogicalRect, TextOptions, TextStyle,
        widgets::{BorderRadius, ImageFit, ImageRequest, ImageSampling, Rectangle},
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

        fn process_line(
            &mut self,
            line: usize,
            range: Range<usize>,
            process: impl FnOnce(&mut [u32]),
        ) {
            self.lines.push(line);
            let start = line * self.width;
            process(&mut self.pixels[start + range.start..start + range.end]);
        }
    }

    #[test]
    fn renderer_supports_custom_pixel_layouts() {
        let font = Font::from_bytes(
            include_bytes!("../../example/assets/Montserrat-Regular.ttf") as &[u8],
            FontSettings::default(),
        )
        .unwrap();
        let mut renderer = Renderer::new(
            VecBuffer::<BgrPixel>::new(32, 24),
            RendererConfig::new(font),
        );
        let clip = PhysicalRect {
            x: 0,
            y: 0,
            width: 32,
            height: 24,
        };
        renderer.draw_rectangle(
            &Rectangle {
                area: LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: 32.0,
                    height: 24.0,
                },
                background: Color::from_rgba8(12, 34, 56, 255),
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
                radius: BorderRadius::default(),
                opacity: 1.0,
            },
            &[clip],
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
            &[clip],
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
        let font = Font::from_bytes(
            include_bytes!("../../example/assets/Montserrat-Regular.ttf") as &[u8],
            FontSettings::default(),
        )
        .unwrap();
        let mut renderer = Renderer::new(VecBuffer::<u32>::new(1, 1), RendererConfig::new(font));
        let texture = ImageData::new(
            bullseye::ImagePixels::Static(&PIXEL),
            bullseye::ImageFormat::Rgba8,
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
            bullseye::ImagePixels::Static(&PIXEL),
            bullseye::ImageFormat::Rgba8,
            1,
            1,
        ));
        assert_ne!(second, first);
    }

    #[test]
    fn frame_is_rendered_once_per_affected_line_in_order() {
        let font = Font::from_bytes(
            include_bytes!("../../example/assets/Montserrat-Regular.ttf") as &[u8],
            FontSettings::default(),
        )
        .unwrap();
        let mut renderer = Renderer::new(
            TrackingBuffer {
                pixels: vec![0; 16],
                lines: Vec::new(),
                width: 4,
                height: 4,
            },
            RendererConfig::new(font),
        )
        .strategy(Scanline::default());
        renderer.draw_rectangle(
            &Rectangle::new(LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
            })
            .background(Color::WHITE),
            &[
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
            ],
        );
        renderer.end_frame();

        assert_eq!(renderer.buffer().lines, [0, 2]);
    }

    #[test]
    fn dropped_image_remains_valid_until_frame_end() {
        static PIXEL: [u8; 4] = [255, 0, 0, 255];
        let font = Font::from_bytes(
            include_bytes!("../../example/assets/Montserrat-Regular.ttf") as &[u8],
            FontSettings::default(),
        )
        .unwrap();
        let mut renderer = Renderer::new(VecBuffer::<u32>::new(1, 1), RendererConfig::new(font))
            .strategy(Scanline::default());
        let image = renderer.create_image(ImageData::new(
            ImagePixels::Static(&PIXEL),
            ImageFormat::Rgba8,
            1,
            1,
        ));
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
            },
            &[PhysicalRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }],
        );
        renderer.drop_image(image);
        renderer.end_frame();

        assert_eq!(renderer.buffer().pixels()[0], 0x00ff_0000);
        let image = RendererImageId::from(KeyData::from_ffi(image.0));
        assert!(!renderer.context.images.contains_key(image));
    }

    #[test]
    fn text_source_can_drop_before_frame_end() {
        let font = Font::from_bytes(
            include_bytes!("../../example/assets/Montserrat-Regular.ttf") as &[u8],
            FontSettings::default(),
        )
        .unwrap();
        let mut renderer = Renderer::new(VecBuffer::<u32>::new(32, 24), RendererConfig::new(font))
            .strategy(Scanline::default());
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
                &[PhysicalRect {
                    x: 0,
                    y: 0,
                    width: 32,
                    height: 24,
                }],
            );
        }
        renderer.end_frame();

        assert!(renderer.buffer().pixels().iter().any(|pixel| *pixel != 0));
    }
}
