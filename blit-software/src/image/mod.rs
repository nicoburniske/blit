mod prepared;

pub use prepared::Prepared;

use blit::{
    ImageData, PhysicalRect,
    widgets::{ImageFit, ImageRequest, ImageTiling},
};

use crate::PixelBuffer;
use prepared::Patch;

pub fn prepare(
    request: &ImageRequest,
    texture: &ImageData,
    clip: PhysicalRect,
    scale_factor: f32,
    mut emit: impl FnMut(Prepared, PhysicalRect),
) {
    let geometry = request.area.to_physical(scale_factor);
    let source = PhysicalRect {
        x: 0,
        y: 0,
        width: texture.size.width,
        height: texture.size.height,
    };
    if geometry.width <= 0
        || geometry.height <= 0
        || source.width <= 0
        || source.height <= 0
        || request.opacity <= 0.0
    {
        return;
    }

    let mut record = |prepared: Prepared| {
        if let Some(clip) = prepared.bounds.intersection(clip) {
            emit(prepared, clip);
        }
    };

    if let Some(slice) = request.nine_slice {
        assert!(slice.left as i32 + slice.right as i32 <= source.width);
        assert!(slice.top as i32 + slice.bottom as i32 <= source.height);
        let (left, right) = fit_borders(
            (slice.left as f32 * scale_factor).round() as i32,
            (slice.right as f32 * scale_factor).round() as i32,
            geometry.width,
        );
        let (top, bottom) = fit_borders(
            (slice.top as f32 * scale_factor).round() as i32,
            (slice.bottom as f32 * scale_factor).round() as i32,
            geometry.height,
        );
        let source_x = [
            0,
            slice.left as i32,
            source.width - slice.right as i32,
            source.width,
        ];
        let source_y = [
            0,
            slice.top as i32,
            source.height - slice.bottom as i32,
            source.height,
        ];
        let destination_x = [
            geometry.x,
            geometry.x + left,
            geometry.x + geometry.width - right,
            geometry.x + geometry.width,
        ];
        let destination_y = [
            geometry.y,
            geometry.y + top,
            geometry.y + geometry.height - bottom,
            geometry.y + geometry.height,
        ];
        for row in 0..3 {
            for column in 0..3 {
                let source = PhysicalRect {
                    x: source_x[column],
                    y: source_y[row],
                    width: source_x[column + 1] - source_x[column],
                    height: source_y[row + 1] - source_y[row],
                };
                let display = PhysicalRect {
                    x: destination_x[column],
                    y: destination_y[row],
                    width: destination_x[column + 1] - destination_x[column],
                    height: destination_y[row + 1] - destination_y[row],
                };
                if let Some(prepared) = Prepared::new(
                    request,
                    texture,
                    Patch {
                        source,
                        display,
                        bounds: display,
                        horizontal_tiling: if column == 1 {
                            request.horizontal_tiling
                        } else {
                            ImageTiling::None
                        },
                        vertical_tiling: if row == 1 {
                            request.vertical_tiling
                        } else {
                            ImageTiling::None
                        },
                    },
                    scale_factor,
                ) {
                    record(prepared);
                }
            }
        }
        return;
    }

    let tiled = request.horizontal_tiling != ImageTiling::None
        || request.vertical_tiling != ImageTiling::None;
    let display = if tiled {
        geometry
    } else {
        match request.fit {
            ImageFit::Fill => geometry,
            ImageFit::Contain | ImageFit::Cover => {
                let horizontal = geometry.width as f32 / source.width as f32;
                let vertical = geometry.height as f32 / source.height as f32;
                let scale = if request.fit == ImageFit::Contain {
                    horizontal.min(vertical)
                } else {
                    horizontal.max(vertical)
                };
                let width = (source.width as f32 * scale).round().max(1.0) as i32;
                let height = (source.height as f32 * scale).round().max(1.0) as i32;
                PhysicalRect {
                    x: geometry.x + (geometry.width - width) / 2,
                    y: geometry.y + (geometry.height - height) / 2,
                    width,
                    height,
                }
            }
        }
    };
    let bounds = if request.fit == ImageFit::Cover || tiled {
        display.intersection(geometry).unwrap_or_default()
    } else {
        display
    };
    if let Some(prepared) = Prepared::new(
        request,
        texture,
        Patch {
            source,
            display,
            bounds,
            horizontal_tiling: request.horizontal_tiling,
            vertical_tiling: request.vertical_tiling,
        },
        scale_factor,
    ) {
        record(prepared);
    }
}

pub fn draw<B: PixelBuffer>(
    buffer: &mut B,
    request: &ImageRequest,
    texture: &ImageData,
    clip: PhysicalRect,
    scale_factor: f32,
) {
    prepare(request, texture, clip, scale_factor, |image, clip| {
        image.draw(buffer, texture, clip)
    });
}

fn fit_borders(first: i32, second: i32, available: i32) -> (i32, i32) {
    if first + second <= available {
        (first, second)
    } else {
        let first = (first as f32 * available as f32 / (first + second) as f32).round() as i32;
        (first, available - first)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use blit::{
        Color, ImageFormat, ImageId, ImagePixels, LogicalRect,
        widgets::{ImageFit, ImageSampling, ImageTiling, NineSlice},
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

        fn blend_texture_slice_rgba(
            pixels: &mut [Self],
            source: &[PremultipliedRgbaColor],
            opacity: u8,
        ) {
            RGBA_SPAN_USED.store(true, Ordering::Relaxed);
            for (pixel, source) in pixels.iter_mut().zip(source) {
                pixel.blend(if opacity == 255 {
                    *source
                } else {
                    source.coverage(opacity as u32)
                });
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
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<u32>::new(4, 4);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
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
    fn colorize_uses_rgba_alpha() {
        static PIXELS: [u8; 4] = [16, 8, 4, 128];
        let texture = ImageData::new(
            ImagePixels::Static(&PIXELS),
            ImageFormat::Rgba8Premultiplied,
            1,
            1,
        );
        let request = ImageRequest {
            image: ImageId(0),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: Some(Color::WHITE),
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<u32>::new(1, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            1.0,
        );

        assert_eq!(buffer.pixels()[0], 0x808080);
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
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<u32>::new(3, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            },
            1.0,
        );

        assert_eq!(buffer.pixels(), [0xff0000, 0x800080, 0x0000ff]);
    }

    #[test]
    fn unscaled_premultiplied_image_with_opacity_uses_texture_span() {
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
            opacity: 0.5,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<SpanPixel>::new(2, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            1.0,
        );

        assert!(RGBA_SPAN_USED.load(Ordering::Relaxed));
        assert_eq!(buffer.pixels()[0].0, 0x800000);
        assert_eq!(buffer.pixels()[1].0, 0x004000);
    }

    #[test]
    fn image_repeats_horizontally() {
        static PIXELS: [u8; 6] = [255, 0, 0, 0, 0, 255];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgb8, 2, 1);
        let request = ImageRequest {
            image: ImageId(0),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::Repeat,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<u32>::new(5, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 5,
                height: 1,
            },
            1.0,
        );

        assert_eq!(
            buffer.pixels(),
            [0xff0000, 0x0000ff, 0xff0000, 0x0000ff, 0xff0000]
        );
    }

    #[test]
    fn round_tiling_fits_complete_tiles() {
        static PIXELS: [u8; 9] = [255, 0, 0, 0, 255, 0, 0, 0, 255];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgb8, 3, 1);
        let request = ImageRequest {
            image: ImageId(0),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 8.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::Round,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<u32>::new(8, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 8,
                height: 1,
            },
            1.0,
        );

        assert_eq!(
            buffer.pixels(),
            [
                0xff0000, 0x00ff00, 0x0000ff, 0xff0000, 0x00ff00, 0x0000ff, 0xff0000, 0x00ff00
            ]
        );
    }

    #[test]
    fn nine_slice_preserves_corners() {
        static PIXELS: [u8; 27] = [
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0, 255, 0, 255, 0, 255, 255, 128, 0, 0, 0,
            128, 0, 0, 0, 128,
        ];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgb8, 3, 3);
        let request = ImageRequest {
            image: ImageId(0),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 5.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: Some(NineSlice::uniform(1)),
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<u32>::new(5, 5);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 5,
                height: 5,
            },
            1.0,
        );

        assert_eq!(buffer.pixels()[0], 0xff0000);
        assert_eq!(buffer.pixels()[4], 0x0000ff);
        assert_eq!(buffer.pixels()[20], 0x800000);
        assert_eq!(buffer.pixels()[24], 0x000080);
        assert_eq!(buffer.pixels()[12], 0xff00ff);
    }
}
