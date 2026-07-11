mod image;
mod pixel;
mod rectangle;
mod text;

pub use fontdue::{Font, FontSettings};
pub use pixel::{Pixel, PixelBuffer, PremultipliedRgbaColor, VecBuffer};

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

pub struct Renderer<B: PixelBuffer> {
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

impl<B: PixelBuffer> Renderer<B> {
    pub fn new(buffer: B, config: RendererConfig) -> Self {
        Self {
            buffer,
            scale_factor: 1.0,
            images: SlotMap::with_key(),
            has_dead_images: false,
            text: TextRenderer::new(config),
        }
    }

    pub fn with_scale_factor(mut self, scale_factor: f32) -> Self {
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        self.scale_factor = scale_factor;
        self
    }

    pub fn screen(&self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: self.buffer.width() as i32,
            height: self.buffer.height() as i32,
        }
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn draw_rectangle(&mut self, request: &Rectangle, clips: &[PhysicalRect]) {
        rectangle::draw(&mut self.buffer, request, clips, self.scale_factor);
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageId {
        data.validate();
        let image = self.images.insert(StoredImage { data, live: true });
        ImageId(image.data().as_ffi())
    }

    pub fn drop_image(&mut self, image: ImageId) {
        let image = RendererImageId::from(KeyData::from_ffi(image.0));
        if let Some(image) = self.images.get_mut(image) {
            image.live = false;
            self.has_dead_images = true;
        }
    }

    pub fn draw_image(&mut self, request: &ImageRequest, clips: &[PhysicalRect]) {
        let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
        if let Some(image) = self.images.get(image) {
            image::draw(
                &mut self.buffer,
                request,
                &image.data,
                clips,
                self.scale_factor,
            );
        }
    }

    pub fn end_frame(&mut self) {
        if !self.has_dead_images {
            return;
        }
        self.images.retain(|_, image| image.live);
        self.has_dead_images = false;
    }

    pub fn draw_text(&mut self, request: &TextRequest<'_>, clips: &[PhysicalRect]) {
        self.text
            .draw(&mut self.buffer, request, clips, self.scale_factor);
    }

    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize {
        self.text
            .offset_at_position(request, position, self.scale_factor)
    }

    pub fn text_cursor_rect(
        &mut self,
        request: &TextRequest<'_>,
        byte_offset: usize,
    ) -> LogicalRect {
        self.text
            .cursor_rect(request, byte_offset, self.scale_factor)
    }

    pub fn buffer(&self) -> &B {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut B {
        &mut self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bullseye::{
        Color, LogicalRect, TextOptions, TextStyle,
        widgets::{BorderRadius, Rectangle},
    };

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
        assert!(renderer.images.contains_key(first_key));

        renderer.end_frame();
        assert!(!renderer.images.contains_key(first_key));

        let second = renderer.create_image(ImageData::new(
            bullseye::ImagePixels::Static(&PIXEL),
            bullseye::ImageFormat::Rgba8,
            1,
            1,
        ));
        assert_ne!(second, first);
    }
}
