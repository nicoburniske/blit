mod prepared;

pub use prepared::Prepared;

use bullseye::{ImageData, PhysicalRect, widgets::ImageRequest};

use crate::PixelBuffer;

pub fn draw<B: PixelBuffer>(
    buffer: &mut B,
    request: &ImageRequest,
    texture: &ImageData,
    clips: &[PhysicalRect],
    scale_factor: f32,
) {
    if let Some(image) = Prepared::new(request, texture, scale_factor) {
        image.draw(buffer, texture, clips);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use bullseye::{
        ImageFormat, ImageId, ImagePixels, LogicalRect,
        widgets::{ImageFit, ImageSampling},
    };

    use super::*;
    use crate::{Pixel, PremultipliedRgbaColor, VecBuffer};

    static RGBA_SPAN_USED: AtomicBool = AtomicBool::new(false);

    #[derive(Clone, Copy, Default)]
    struct SpanPixel(u32);

    impl Pixel for SpanPixel {
        fn blend(&mut self, color: PremultipliedRgbaColor) {
            self.0.blend(color)
        }

        fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
            Self(u32::from_rgb(red, green, blue))
        }

        fn blend_texture_slice_rgba(pixels: &mut [Self], source: &[PremultipliedRgbaColor]) {
            RGBA_SPAN_USED.store(true, Ordering::Relaxed);
            for (pixel, source) in pixels.iter_mut().zip(source) {
                pixel.blend(*source);
            }
        }
    }

    #[test]
    fn nearest_scaled_image_respects_clip() {
        static PIXELS: [u8; 16] = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgba8, 2, 2);
        let request = ImageRequest {
            image: ImageId(0),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
        };
        let mut buffer = VecBuffer::<u32>::new(4, 4);

        draw(
            &mut buffer,
            &request,
            &texture,
            &[PhysicalRect {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            }],
            1.0,
        );

        assert_eq!(buffer.pixels()[0], 0);
        assert_eq!(buffer.pixels()[5], 0xff0000);
        assert_eq!(buffer.pixels()[6], 0x00ff00);
        assert_eq!(buffer.pixels()[9], 0x0000ff);
        assert_eq!(buffer.pixels()[10], 0xffffff);
        assert_eq!(buffer.pixels()[15], 0);
    }

    #[test]
    fn bilinear_image_interpolates_source_pixels() {
        static PIXELS: [u8; 8] = [255, 0, 0, 255, 0, 0, 255, 255];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgba8, 2, 1);
        let request = ImageRequest {
            image: ImageId(0),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 3.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Bilinear,
            opacity: 1.0,
        };
        let mut buffer = VecBuffer::<u32>::new(3, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            &[PhysicalRect {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            }],
            1.0,
        );

        assert_eq!(buffer.pixels(), [0xff0000, 0x800080, 0x0000ff]);
    }

    #[test]
    fn unscaled_premultiplied_image_uses_texture_span() {
        static PIXELS: [u8; 8] = [255, 0, 0, 255, 0, 128, 0, 128];
        RGBA_SPAN_USED.store(false, Ordering::Relaxed);
        let texture = ImageData::new(
            ImagePixels::Static(&PIXELS),
            ImageFormat::Rgba8Premultiplied,
            2,
            1,
        );
        let request = ImageRequest {
            image: ImageId(0),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 2.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
        };
        let mut buffer = VecBuffer::<SpanPixel>::new(2, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            &[PhysicalRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            }],
            1.0,
        );

        assert!(RGBA_SPAN_USED.load(Ordering::Relaxed));
        assert_eq!(buffer.pixels()[0].0, 0xff0000);
        assert_eq!(buffer.pixels()[1].0, 0x008000);
    }
}
