mod image;
mod pixel;
mod rectangle;
mod text;

pub use fontdue::{Font, FontSettings};
pub use pixel::{Pixel, PixelBuffer, PremultipliedRgbaColor, VecBuffer};

use bullseye::{
    PhysicalRect, Platform, PlatformImpl, TextRequest,
    widgets::{Image, Rectangle},
};
use text::TextRenderer;

pub struct RendererConfig {
    pub font: Font,
    pub glyph_cache_capacity: usize,
    pub paragraph_cache_capacity: usize,
}

impl RendererConfig {
    pub fn new(font: Font) -> Self {
        Self {
            font,
            glyph_cache_capacity: 1024 * 1024,
            paragraph_cache_capacity: 1024 * 1024,
        }
    }
}

pub struct Renderer<B: PixelBuffer> {
    buffer: B,
    scale_factor: f32,
    text: TextRenderer,
}

impl<B: PixelBuffer> Renderer<B> {
    pub fn new(buffer: B, config: RendererConfig) -> Self {
        Self {
            buffer,
            scale_factor: 1.0,
            text: TextRenderer::new(config),
        }
    }

    pub fn with_scale_factor(mut self, scale_factor: f32) -> Self {
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        self.scale_factor = scale_factor;
        self
    }

    pub fn buffer(&self) -> &B {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut B {
        &mut self.buffer
    }

    pub fn handle(&mut self) -> Platform {
        unsafe { Platform::new(self) }
    }
}

impl<B: PixelBuffer> PlatformImpl for Renderer<B> {
    fn screen(&mut self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: self.buffer.width() as i32,
            height: self.buffer.height() as i32,
        }
    }

    fn scale_factor(&mut self) -> f32 {
        self.scale_factor
    }

    fn draw_rectangle(&mut self, request: &Rectangle, clips: &[PhysicalRect]) {
        rectangle::draw(&mut self.buffer, request, clips, self.scale_factor);
    }

    fn draw_image(&mut self, image: &Image<'_>, clips: &[PhysicalRect]) {
        image::draw(&mut self.buffer, image, clips, self.scale_factor);
    }

    fn draw_text(&mut self, request: &TextRequest<'_>, clips: &[PhysicalRect]) {
        self.text
            .draw(&mut self.buffer, request, clips, self.scale_factor);
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
                color: Color::WHITE,
                style: TextStyle::default(),
                options: TextOptions::default(),
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
    }
}
